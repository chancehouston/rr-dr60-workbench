//! The A/B position map (spec 001 D6, FR-010; research R-06).
//!
//! Render → original is the engine's `timeline::onepass_output_to_input` on the gate reports'
//! clock (`render::output_to_input`, spec 008 D6). Original → render inverts it by binary search
//! on the unclamped key `o + removed(o − C)` (C the coder's delay inside the latency), which is
//! strictly increasing in `o`, so input 0 maps to the latency and not to 0.

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
    to_render_raw(
        i,
        u64::from(r.latency_samples),
        u64::from(r.codec_delay_samples),
        &r.events,
        r.drop_mode,
        len,
    )
}

/// [`to_render`] on raw parts (the tests' entry point): `codec` is the coder's delay inside
/// `latency` (the reports' positions exclude it).
pub fn to_render_raw(
    i: u64,
    latency: u64,
    codec: u64,
    events: &[VasEvent],
    drop: bool,
    len: u64,
) -> u64 {
    let target = i + latency;
    let key = |o: u64| {
        o + if o >= codec {
            removed(o - codec, events, drop)
        } else {
            0
        }
    };
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
    let (i, _) = crate::render::output_to_input(o, r);
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

/// The render's marks, from its reports and the engine's map on the reports' clock (spec 008
/// D6): a report's position excludes the coder's delay `C`, so its splice reaches the output at
/// the position + `C`, and its input span starts at the position less the latency less `C`.
pub fn marks(r: &Render) -> Vec<Mark> {
    let c = u64::from(r.codec_delay_samples);
    let l = u64::from(r.latency_samples).saturating_sub(c);
    let mut before = 0u64;
    r.events
        .iter()
        .map(|e| {
            if r.drop_mode {
                let start = (e.output_position + before).saturating_sub(l);
                before += e.input_length;
                Mark {
                    output_position: e.output_position + c,
                    input_span: (start, start + e.input_length),
                    removed: true,
                }
            } else {
                let start = e.output_position.saturating_sub(l);
                Mark {
                    output_position: e.output_position + c,
                    input_span: (start, start + e.input_length),
                    removed: false,
                }
            }
        })
        .collect()
}

/// The input position of render position `o` (spec 005b D5): the render's start plus its map,
/// clamped to the input. In `SameSeconds` it is input time: the start plus `o`.
pub fn input_at(o: u64, r: &Render, mode: AbMode, input_len: u64) -> u64 {
    // The take's own fed length first, then its start, then the input (hardware review Q10).
    let own = input_len.saturating_sub(r.request.start);
    (r.request.start + to_original(o, r, mode, own)).min(input_len)
}

/// The render position of input position `i` (spec 005b D5): the map of `i` less the render's
/// start, at the render's start before it, clamped to its end.
pub fn render_at(i: u64, r: &Render, mode: AbMode) -> u64 {
    to_render(i.saturating_sub(r.request.start), r, mode)
}

/// The position in take `to` for position `pos` in take `from` (spec 005b D5): through the input,
/// the original being the input itself. A render take that is absent maps to 0.
pub fn switch(
    from: crate::monitor::Take,
    pos: u64,
    to: crate::monitor::Take,
    a: Option<&Render>,
    b: Option<&Render>,
    input_len: u64,
    mode: AbMode,
) -> u64 {
    use crate::monitor::Take;
    let of = |t: Take| match t {
        Take::Original => None,
        Take::Render => a,
        Take::B => b,
    };
    let input = match from {
        Take::Original => pos.min(input_len),
        t => of(t).map_or(0, |r| input_at(pos, r, mode, input_len)),
    };
    match to {
        Take::Original => input,
        t => of(t).map_or(0, |r| render_at(input, r, mode)),
    }
}
