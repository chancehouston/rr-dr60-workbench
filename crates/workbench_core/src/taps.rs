//! The tap browser (spec 002 D8; research R-07): the input rendered at every tap of the
//! engine's tap control, on a fresh engine per tap with the session's values. Each tap keeps
//! its readings, its output digest and its spectrum, not its samples.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rr_dr60::control_map::ControlReading;
use rr_dr60::controls::ControlValue;

use crate::analysis::spectrum::{self, Spectrum};
use crate::render::{self, RenderRequest};

/// A tap's readings.
#[derive(Clone, Debug, PartialEq)]
pub struct TapReadings {
    /// Output samples.
    pub length: u64,
    /// The engine's latency.
    pub latency: u32,
    /// The tail.
    pub tail: u64,
    /// Splices or muted regions (when the gate runs).
    pub splices_or_regions: u64,
    /// Whether the gate runs here.
    pub gate_reached: bool,
    /// Not emitted.
    pub not_emitted: u64,
    /// The sample peak.
    pub peak: f32,
    /// SHA-256 of the output.
    pub sha256: [u8; 32],
    /// The render-side spectrum over the render's own runs (None when too short).
    pub spectrum: Option<Spectrum>,
}

/// One tap's row.
#[derive(Clone, Debug, PartialEq)]
pub struct TapResult {
    /// The tap's option identifier.
    pub tap: &'static str,
    /// Its readings, or the engine's refusal.
    pub outcome: Result<TapReadings, String>,
}

/// The browser's table and the request it was made from (stale by 001's rule).
#[derive(Clone, Debug)]
pub struct TapTable {
    /// The session's request (its tap is the panel's).
    pub request: RenderRequest,
    /// One row per tap, in the tap control's order.
    pub rows: Vec<TapResult>,
    /// The sidecar of the request's own render (the CSV's companion; post-impl coach 1).
    pub sidecar: Option<String>,
}

impl TapTable {
    /// Stale under 001's D5 rule (post-implementation review, coach 2).
    pub fn is_stale(&self, current: &RenderRequest) -> bool {
        self.request.differs(current)
    }
}

/// The number of taps the browser renders.
pub fn tap_count() -> usize {
    tap_control().1.len()
}

/// The tap control's index and options.
fn tap_control() -> (u32, &'static [rr_dr60::controls::ControlOption]) {
    let (i, e) = rr_dr60::controls::controls()
        .iter()
        .enumerate()
        .find(|(_, e)| e.identifier == "tap")
        .expect("the engine has a tap control");
    let ControlValue::Choice { options, .. } = e.value else {
        panic!("the tap control is a choice")
    };
    (i as u32, options)
}

/// Renders `req` at every tap (FR-008). `progress` counts taps done; a cancel stops after the
/// current tap and keeps the finished rows.
pub fn browse(req: &RenderRequest, progress: &AtomicU64, cancel: &AtomicBool) -> TapTable {
    browse_with(req, progress, |_| cancel.load(Ordering::Relaxed))
}

/// [`browse`] with a stop rule asked before each tap (`k` = the taps done).
pub fn browse_with(
    req: &RenderRequest,
    progress: &AtomicU64,
    stop: impl Fn(usize) -> bool,
) -> TapTable {
    let (tap_index, options) = tap_control();
    let own_tap = rr_dr60::control_map::settings_value(&req.settings, tap_index).ok();
    let mut rows = Vec::new();
    let mut sidecar = None;
    for (k, o) in options.iter().enumerate() {
        if stop(k) {
            break;
        }
        let mut q = req.clone();
        if rr_dr60::control_map::set_settings_value(
            &mut q.settings,
            tap_index,
            ControlReading::Choice(o.value),
        )
        .is_err()
        {
            continue;
        }
        let none = AtomicBool::new(false);
        let outcome = match render::render(&q, &AtomicU64::new(0), &none) {
            Ok(Some(r)) => {
                let spectrum = spectrum::render_side(&r).ok();
                if own_tap == Some(ControlReading::Choice(o.value)) {
                    sidecar = Some(crate::sidecar::write(&r));
                }
                Ok(TapReadings {
                    length: r.samples.len() as u64,
                    latency: r.latency_samples,
                    tail: r.tail_samples,
                    splices_or_regions: r.splices_or_regions,
                    gate_reached: r.gate_reached,
                    not_emitted: r.not_emitted,
                    peak: r.peak,
                    sha256: r.output_sha256,
                    spectrum,
                })
            }
            Ok(None) => Err("cancelled".into()),
            // The engine's own refusal, in its words (plan review, coach 15).
            Err(e) => Err(e.to_string()),
        };
        rows.push(TapResult {
            tap: o.identifier,
            outcome,
        });
        progress.store(k as u64 + 1, Ordering::Relaxed);
    }
    TapTable {
        request: req.clone(),
        rows,
        sidecar,
    }
}
