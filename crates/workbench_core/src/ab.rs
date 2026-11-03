//! The A/B position map (spec 001 D6, FR-010; research R-06).
//!
//! Render → original is the engine's `timeline::onepass_output_to_input`. Original → render
//! inverts it by binary search on the unclamped key `o + removed(o)`, which is strictly
//! increasing in `o`, so input 0 maps to the latency and not to 0.

use rr_dr60::VasEvent;

use crate::render::Render;

/// How the switch picks the other take's position (D6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AbMode {
    /// The same moment of content, through the engine's one-pass map.
    #[default]
    EngineMap,
    /// The same sample index (the same seconds: both takes share the rate).
    SameSeconds,
}

/// The input samples the gate removed at or before output `o` (drop mode; 0 in mute mode), the
/// engine map's own rule (`take_while(output_position <= o)`).
fn removed(o: u64, events: &[VasEvent], drop: bool) -> u64 {
    if !drop {
        return 0;
    }
    events
        .iter()
        .take_while(|e| e.output_position <= o)
        .map(|e| e.input_length)
        .sum()
}

/// The render position for original position `i`, clamped to the render's end.
pub fn to_render(i: u64, r: &Render, mode: AbMode) -> u64 {
    let len = r.samples.len() as u64;
    if mode == AbMode::SameSeconds {
        return i.min(len);
    }
    to_render_raw(i, u64::from(r.latency_samples), &r.events, r.drop_mode, len)
}

/// [`to_render`] on raw parts (the tests' entry point).
pub fn to_render_raw(i: u64, latency: u64, events: &[VasEvent], drop: bool, len: u64) -> u64 {
    let target = i + latency;
    let key = |o: u64| o + removed(o, events, drop);
    // Smallest o in [0, len) with key(o) ≥ target; len when none (a final pause, or past the end).
    let (mut lo, mut hi) = (0u64, len);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if key(mid) >= target {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

/// The original position for render position `o`, clamped to the original's end.
pub fn to_original(o: u64, r: &Render, mode: AbMode, input_len: u64) -> u64 {
    if mode == AbMode::SameSeconds {
        return o.min(input_len);
    }
    let (i, _) =
        rr_dr60::timeline::onepass_output_to_input(o, r.latency_samples, &r.events, r.drop_mode);
    i.min(input_len)
}

/// A removed pause (drop mode) or a muted region (mute mode), for the waveform's marks (D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The render position of the splice (drop) or the region's first muted sample (mute).
    pub output_position: u64,
    /// The original positions it covers, `[start, end)`.
    pub input_span: (u64, u64),
    /// Drop mode (a removed pause) or mute mode (a muted region).
    pub removed: bool,
}

/// The render's marks, from its reports and the engine's map.
pub fn marks(r: &Render) -> Vec<Mark> {
    let l = u64::from(r.latency_samples);
    let mut before = 0u64;
    r.events
        .iter()
        .map(|e| {
            if r.drop_mode {
                let start = (e.output_position + before).saturating_sub(l);
                before += e.input_length;
                Mark {
                    output_position: e.output_position,
                    input_span: (start, start + e.input_length),
                    removed: true,
                }
            } else {
                let start = e.output_position.saturating_sub(l);
                Mark {
                    output_position: e.output_position,
                    input_span: (start, start + e.input_length),
                    removed: false,
                }
            }
        })
        .collect()
}
