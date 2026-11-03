//! The render (spec 001 FR-007 – FR-009; research R-07, R-09; data-model "Render").
//!
//! Each render runs on a fresh engine (recording counter 0). The engine values are applied in
//! manifest order before the first call, and the input plus the tail is processed in blocks.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rr_dr60::control_map::ControlReading;
use rr_dr60::{Error, Pipeline, Settings, Tap, VasEvent, VasMode};

use crate::input::{self, Channel};

/// The render's block for a rate: 10 ms of host samples (spec 002 D1). 100 is the seven rates'
/// greatest common divisor, so every call carries exactly 60 device samples. The output does
/// not depend on the block (001's oracles); the block sets the meter timeline's resolution.
pub const fn block_for(rate_hz: u32) -> usize {
    (rate_hz / 100) as usize
}

/// What a render is made from: a snapshot taken at Render (data-model "RenderRequest").
#[derive(Clone, Debug)]
pub struct RenderRequest {
    /// The converted input (a clone of `Input.samples`, no copy).
    pub input: Arc<[f32]>,
    /// The input's digest of its converted samples (staleness, the sidecar; D9).
    pub input_sha256: [u8; 32],
    /// The input file's name, frames and raw digest, so a sidecar is written from the render
    /// alone (post-implementation review, coach 2).
    pub file_name: String,
    /// The input's frames per channel.
    pub frames: u64,
    /// SHA-256 of the input file's bytes.
    pub raw_sha256: [u8; 32],
    /// The input's rate.
    pub rate_hz: u32,
    /// The selected channel.
    pub channel: Channel,
    /// The settings.
    pub settings: Settings,
    /// The live and component values, in manifest order (the slot precedes its parameters; the
    /// live controls are independent on a fresh engine; spec Clarifications, implementation).
    pub engine_values: Vec<(u32, ControlReading)>,
    /// Whether the tail is appended (D7).
    pub tail: bool,
    /// The block size.
    pub block: usize,
}

/// A finished render and its readings (FR-009).
#[derive(Clone, Debug)]
pub struct Render {
    /// The output, built once.
    pub samples: Arc<[f32]>,
    /// The snapshot it was made from.
    pub request: RenderRequest,
    /// The tail's length in host samples (0 when off or where the engine's latency is 0).
    pub tail_samples: u64,
    /// The engine's latency for the settings and tap.
    pub latency_samples: u32,
    /// The engine's playback drain (part of the tail's figure).
    pub drain_samples: u32,
    /// The engine's playback-side latency (the guard around splices in spectra; post-impl B3).
    pub playback_latency_samples: u32,
    /// Splices (drop mode) or muted regions (mute mode): Σ `BlockInfo.events`.
    pub splices_or_regions: u64,
    /// The engine's reports, positions from the start of the render.
    pub events: Vec<VasEvent>,
    /// The gate removes pauses (drop mode).
    pub drop_mode: bool,
    /// The gate runs at this configuration ([`gate_reached`]).
    pub gate_reached: bool,
    /// The gate's state after the last call.
    pub paused_at_end: bool,
    /// Input + tail − output: removed pauses, dropped tail and samples held at a final pause.
    pub not_emitted: u64,
    /// `max |x|` over the output (a workbench analysis).
    pub peak: f32,
    /// SHA-256 of the output as f32 LE bytes.
    pub output_sha256: [u8; 32],
    /// The engine's footprint.
    pub footprint_bytes: usize,
    /// The meter timeline (spec 002 D1): the rest read-out, then one row per call.
    pub meters: Vec<crate::meters::MeterRow>,
}

/// Whether the voice-activation gate runs for `s` — the engine's own rule, quoted because the
/// engine does not publish it (rr-dr60-emu#72). At `a06318c`, `paths.rs:204-206`:
/// `vas.enabled && (tap ∈ {AfterVas, AfterDecoder} || tap.is_after_stage_10())`, where the taps
/// after stage 10 are `AfterPlayback`, `RouteOutput`, `AfterVolumeAmp`, `AfterSpeaker` and
/// `AfterEarphone` (`settings.rs:109-117`). The `readings` test pins it by behavior.
pub fn gate_reached(s: &Settings) -> bool {
    s.vas.enabled
        && matches!(
            s.tap,
            Tap::AfterVas
                | Tap::AfterDecoder
                | Tap::AfterPlayback
                | Tap::RouteOutput
                | Tap::AfterVolumeAmp
                | Tap::AfterSpeaker
                | Tap::AfterEarphone
        )
}

impl RenderRequest {
    /// The staleness predicate (001 D5), shared by renders and the tap table: the input, the
    /// channel, the tail switch, the settings or the engine values differ.
    pub fn differs(&self, other: &RenderRequest) -> bool {
        self.input_sha256 != other.input_sha256
            || self.channel != other.channel
            || self.tail != other.tail
            || self.settings != other.settings
            || self.engine_values != other.engine_values
    }

    /// The request for `input` with `panel`'s values (the session's one constructor).
    pub fn new(input: &input::Input, panel: &crate::panel::Panel, tail: bool) -> Self {
        let (settings, engine_values) = panel.snapshot();
        Self {
            input: input.samples.clone(),
            input_sha256: input.input_sha256,
            file_name: input.file_name.clone(),
            frames: input.frames,
            raw_sha256: input.raw_sha256,
            rate_hz: input.rate_hz,
            channel: input.channel,
            settings,
            engine_values,
            tail,
            block: block_for(input.rate_hz),
        }
    }
}

/// Builds the fresh engine of a render and applies the engine values (research R-09).
///
/// # Errors
///
/// The engine's refusal of the settings or of a value.
pub fn fresh_engine(req: &RenderRequest) -> Result<Pipeline, Error> {
    let mut p = Pipeline::new(req.settings)?;
    assert_eq!(
        p.recording_counter(),
        0,
        "a fresh engine has recording counter 0"
    );
    for &(i, v) in &req.engine_values {
        p.set_control_value(i, v)?;
    }
    Ok(p)
}

/// The tail for an engine: its latency plus its playback drain, or 0 where its latency is 0
/// (the taps before the gate; plan P12). The one-pass has no drain figure of its own
/// (rr-dr60-emu#73); the `tail` tests show every input sample is emitted.
pub fn tail_for(p: &Pipeline) -> u64 {
    let l = p.latency_samples();
    if l == 0 {
        0
    } else {
        u64::from(l) + u64::from(p.drain_samples())
    }
}

/// Renders `req` (FR-007). `progress` receives the input samples consumed; `cancel` is checked
/// per block. `Ok(None)` means cancelled.
///
/// # Errors
///
/// The engine's refusals.
pub fn render(
    req: &RenderRequest,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<Option<Render>, Error> {
    let mut p = fresh_engine(req)?;
    let latency = p.latency_samples();
    let drain = p.drain_samples();
    let playback_latency = p.playback_latency_samples();
    let tail = if req.tail { tail_for(&p) } else { 0 };
    let block = req.block.max(1);
    let total = req.input.len() as u64 + tail;
    let mut out: Vec<f32> = Vec::with_capacity(total as usize);
    let mut events = Vec::new();
    let mut ibuf = vec![0.0f32; block];
    let mut obuf = vec![0.0f32; block];
    let mut ebuf = vec![VasEvent::new(0, 0); p.max_events(block).max(1)];
    let mut fed = 0u64;
    let mut count = 0u64;
    let mut paused = false;
    let input_len = req.input.len() as u64;
    let mut meters = Vec::with_capacity((total / block as u64 + 2) as usize);
    meters.push(crate::meters::MeterRow {
        meters: p.rest_meters(),
        in_span: (0, 0),
        out_span: (0, 0),
        tail: false,
    });
    while fed < total {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let n = (total - fed).min(block as u64) as usize;
        let start = fed as usize;
        let have = req.input.len().saturating_sub(start).min(n);
        if have > 0 {
            ibuf[..have].copy_from_slice(&req.input[start..start + have]);
        }
        ibuf[have..n].fill(0.0);
        let info = p.process_with_events(&ibuf[..n], &mut obuf[..n], &mut ebuf)?;
        let out0 = out.len() as u64;
        out.extend_from_slice(&obuf[..info.produced]);
        meters.push(crate::meters::MeterRow {
            meters: Pipeline::meters(&p),
            in_span: (fed, fed + n as u64),
            out_span: (out0, out.len() as u64),
            tail: fed >= input_len && n > 0,
        });
        events.extend_from_slice(&ebuf[..info.events]);
        count += info.events as u64;
        paused = info.paused;
        fed += n as u64;
        progress.store(fed.min(req.input.len() as u64), Ordering::Relaxed);
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let output_sha256 = input::samples_sha256(&out);
    let not_emitted = total - out.len() as u64;
    Ok(Some(Render {
        samples: out.into(),
        request: req.clone(),
        tail_samples: tail,
        latency_samples: latency,
        drain_samples: drain,
        playback_latency_samples: playback_latency,
        splices_or_regions: count,
        events,
        drop_mode: req.settings.vas.mode == VasMode::Drop,
        gate_reached: gate_reached(&req.settings),
        paused_at_end: paused,
        not_emitted,
        peak,
        output_sha256,
        footprint_bytes: p.footprint_bytes(),
        meters,
    }))
}

impl Render {
    /// True when the session's current values differ from the render's (D5).
    pub fn is_stale(&self, current: &RenderRequest) -> bool {
        self.request.differs(current)
    }
}
