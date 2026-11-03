//! Every string the workbench writes (spec 001 FR-016; research R-13). The wording test checks
//! each entry of [`all`] against the engine's denylist and the supplementary list. The engine's
//! own strings (control names, descriptions) are shown as the engine gives them and are checked
//! by the engine's tests.

use rr_dr60::controls::{ControlKind, ControlTiming};

/// A file could not be read.
pub const OPEN_IO: &str = "The file could not be read";
/// An unsupported container or codec.
pub const OPEN_FORMAT: &str = "This file format is not supported";
/// The supported containers.
pub const SUPPORTED_FORMATS: &str =
    "Supported: WAV, AIFF and FLAC (lossy and companded formats are refused).";
/// An unsupported sample type.
pub const OPEN_SAMPLE_TYPE: &str = "This sample type is not supported";
/// The supported sample types.
pub const SUPPORTED_SAMPLE_TYPES: &str =
    "Supported: 16-, 24- or 32-bit integer and 32-bit float samples.";
/// An unsupported sample rate.
pub const OPEN_RATE: &str = "This sample rate is not one of the engine's rates";
/// A missing channel.
pub const OPEN_CHANNEL: &str = "The file has no channel";
/// A decoder failure.
pub const OPEN_DECODE: &str = "The file could not be decoded";
/// The mix's label (D2).
pub const MIX_LABEL: &str =
    "Mix: the mean of all channels, a conversion at the boundary (anti-phase content cancels)";
/// The 32-bit integer label (FR-002).
pub const I32_ROUNDING: &str =
    "32-bit integer samples are rounded once to 32-bit float (a conversion at the boundary)";
/// The tail's label (D7).
pub const TAIL_LABEL: &str = "Tail: the engine's one-pass drain, as silence after the input, so \
every input sample is emitted and settles while the gate stays open. In drop mode it holds only \
while the hang outlasts it: a pause that begins before the kept samples are out holds them until \
the next resume";
/// Shown instead of a splice count when the gate does not run (FR-009).
pub const GATE_NOT_REACHED: &str = "Gate not reached at this configuration";
/// Splices (drop mode).
pub const SPLICES: &str = "Splices";
/// Muted regions (mute mode).
pub const MUTED_REGIONS: &str = "Muted regions";
/// The not-emitted reading's label (FR-009, restated).
pub const NOT_EMITTED: &str =
    "Not emitted: removed pauses, dropped tail and samples held at a final pause";
/// The peak's label.
pub const PEAK_LABEL: &str = "Sample peak (workbench analysis of the engine's output)";
/// The peak warning.
pub const PEAK_WARNING: &str = "Above 1.0: the output device clips this on playback (the export \
keeps the samples)";
/// The playback conversion note (R-04).
pub const DEVICE_RATE_NOTE: &str = "Playback is converted by macOS to the device's rate \
(Mastering quality); the conversion can raise peaks above the sample peak";
/// The monitor volume's label.
pub const MONITOR_VOLUME: &str = "Monitor volume (playback only, never exported)";
/// Playback is unavailable on this platform (P2).
pub const PLAYBACK_UNAVAILABLE: &str =
    "Playback is available on macOS only in this version; render and export still work";
/// The output device changed or was lost.
pub const DEVICE_LOST: &str = "The output device changed or was lost; playback stopped";
/// Not on the one-pass (D4).
pub const NOT_ON_ONE_PASS: &str = "Not on the one-pass";
/// The host rate is the file's (D4).
pub const HOST_RATE_READ_ONLY: &str = "The file's rate";
/// The stale marker (D5).
pub const STALE: &str = "Settings changed since the render";
/// The engine map option (D6).
pub const AB_ENGINE_MAP: &str = "Same moment (the engine's one-pass map)";
/// The same-seconds option (D6).
pub const AB_SAME_SECONDS: &str = "Same seconds";
/// Overloads counted on the device.
pub const OVERLOADS: &str = "Device overloads";
/// Live (003 D2): an idle control.
pub const LIVE_IDLE: &str = "Idle: stop to change";
/// Live: a control the one-pass does not run.
pub const LIVE_NOT_ONE_PASS: &str = "Not on the one-pass";
/// Live: a stage off at the start or beyond the tap.
pub const LIVE_STAGE_OFF: &str = "Stage not running in this configuration";
/// Live: a route a named-route tap cannot read.
pub const LIVE_TAP_ROUTE: &str = "Tap not on this route";
/// Live: a band-filter parameter with the slot empty.
pub const LIVE_SLOT_EMPTY: &str = "Slot empty";
/// Live (003 D4): silence the budget inserted.
pub const LIVE_SKIPPING: &str = "Gate paused: skipping (silence, not engine output)";
/// Live (003 D8): a take that cannot be reproduced.
pub const LIVE_INCOMPLETE: &str =
    "Incomplete (not reproducible): a queue or report buffer overflowed";
/// An empty take in the views.
pub const NO_SAMPLES: &str = "No samples: the take is empty";
/// A sidecar opened before its input.
pub const OPEN_INPUT_FIRST: &str = "Open the input file before its sidecar";
/// Exporting a stale render (coach 2: allowed, from the render's own request).
pub const STALE_EXPORT: &str =
    "This exports the last render as it was made, with its own settings, not the current panel";

/// The headroom hint beside the peak warning (hardware review B8): the monitor volume that keeps
/// the device below full scale. The workbench never scales automatically.
pub fn headroom_hint(peak: f32) -> String {
    format!(
        "Set the monitor volume to {:.2} or lower to hear it without the device clipping",
        1.0 / peak.max(1.0)
    )
}

/// The supported rates, from the engine.
pub fn supported_rates() -> String {
    let rates: Vec<String> = rr_dr60::SUPPORTED_HOST_RATES
        .iter()
        .map(|r| format!("{r}"))
        .collect();
    format!("Supported rates (Hz): {}.", rates.join(", "))
}

/// The kind's words as the engine's control manifest uses them.
pub fn kind_label(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::DeviceControl => "device control",
        ControlKind::UnitCondition => "unit condition",
        ControlKind::ModelInternal => "model internal",
        ControlKind::EmulatorOption => "emulator option",
        ControlKind::HostParameter => "host parameter",
        _ => "unknown kind",
    }
}

/// The timing's words as the engine's control manifest uses them.
pub fn timing_label(timing: ControlTiming) -> &'static str {
    match timing {
        ControlTiming::Live => "live",
        ControlTiming::Idle => "idle",
        ControlTiming::IdleMemoryDependent => "idle, memory-dependent",
        ControlTiming::HostFixed => "host-fixed",
        _ => "unknown timing",
    }
}

/// Every constant string above, for the wording test.
pub fn all() -> Vec<String> {
    let mut v: Vec<String> = [
        OPEN_IO,
        OPEN_FORMAT,
        SUPPORTED_FORMATS,
        OPEN_SAMPLE_TYPE,
        SUPPORTED_SAMPLE_TYPES,
        OPEN_RATE,
        OPEN_CHANNEL,
        OPEN_DECODE,
        MIX_LABEL,
        I32_ROUNDING,
        TAIL_LABEL,
        GATE_NOT_REACHED,
        SPLICES,
        MUTED_REGIONS,
        NOT_EMITTED,
        PEAK_LABEL,
        PEAK_WARNING,
        DEVICE_RATE_NOTE,
        MONITOR_VOLUME,
        PLAYBACK_UNAVAILABLE,
        DEVICE_LOST,
        NOT_ON_ONE_PASS,
        HOST_RATE_READ_ONLY,
        STALE,
        AB_ENGINE_MAP,
        AB_SAME_SECONDS,
        OVERLOADS,
        OPEN_INPUT_FIRST,
        NO_SAMPLES,
        LIVE_IDLE,
        LIVE_NOT_ONE_PASS,
        LIVE_STAGE_OFF,
        LIVE_TAP_ROUTE,
        LIVE_SLOT_EMPTY,
        LIVE_SKIPPING,
        LIVE_INCOMPLETE,
        STALE_EXPORT,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    v.push(supported_rates());
    v.push(headroom_hint(2.0));
    v
}
