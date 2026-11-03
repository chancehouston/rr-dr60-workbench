//! The meter timeline (spec 002 D1 – D4; research R-03).
//!
//! A render reads the engine's rest read-out, then the read-out after every 10 ms call. The
//! field table below is the workbench's: identifiers, units, kinds and sides quoted from the
//! engine's rustdoc at `a06318c` (`pipeline.rs:227-286`) until the engine publishes one
//! (rr-dr60-emu#75). The `reached` rule is pinned by behavior in the tests.

use rr_dr60::{Meters, Route, Settings, Tap};

/// One row of the timeline: a read-out and its call's spans.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterRow {
    /// The engine's read-out (row 0: the rest read-out).
    pub meters: Meters,
    /// The call's input span `[start, end)` in host samples.
    pub in_span: (u64, u64),
    /// The call's output span `[start, end)` in host samples.
    pub out_span: (u64, u64),
    /// The call reaches into the tail.
    pub tail: bool,
}

/// How a field is read (D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The largest value over the call's processed samples (a sample peak, or dB).
    Max,
    /// A count over the call's processed samples.
    Count,
    /// A state at the call's end.
    State,
}

/// Which clock a field belongs to (D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The input side, at the host rate before the conversion.
    Input,
    /// A device-rate point (stages 4 – 5): up to the record path's latency late.
    Device,
    /// The output side, at the host rate after the conversion.
    Output,
}

/// One meter field.
#[derive(Clone, Copy, Debug)]
pub struct MeterField {
    /// The engine's field identifier.
    pub id: &'static str,
    /// The unit as shown.
    pub unit: &'static str,
    /// Max, count or state.
    pub kind: Kind,
    /// Its clock.
    pub side: Side,
    /// A workbench note from the engine's rustdoc (labelled as such on screen).
    pub note: &'static str,
    /// The value as f64.
    pub get: fn(&Meters) -> f64,
}

/// The thirteen fields at the pin, in the engine's declaration order.
pub const FIELDS: [MeterField; 13] = [
    MeterField {
        id: "input_peak",
        unit: "sample peak",
        kind: Kind::Max,
        side: Side::Input,
        note: "the largest absolute input sample after sanitizing",
        get: |m| f64::from(m.input_peak),
    },
    MeterField {
        id: "limiter_input_peak",
        unit: "sample peak",
        kind: Kind::Max,
        side: Side::Input,
        note: "the largest absolute sample entering stage 3 (after stages 1 – 2 and the slot; at after_mic_amp: stage 2's output, before the slot)",
        get: |m| f64::from(m.limiter_input_peak),
    },
    MeterField {
        id: "gain_reduction_db",
        unit: "dB",
        kind: Kind::Max,
        side: Side::Input,
        note: "the deepest attenuation the limiter applied in the call",
        get: |m| f64::from(m.gain_reduction_db),
    },
    MeterField {
        id: "hold_charge",
        unit: "fraction of the 40 dB hold",
        kind: Kind::State,
        side: Side::Input,
        note: "the limiter's hold at the call's end (0.13 unity floor, 0.73 1 dB, 0.84 10 dB, 0.90 20 dB)",
        get: |m| f64::from(m.hold_charge),
    },
    MeterField {
        id: "record_filter_peak",
        unit: "sample peak",
        kind: Kind::Max,
        side: Side::Device,
        note: "the largest device sample after stage 4's band-limit, before the converter's clamp",
        get: |m| f64::from(m.record_filter_peak),
    },
    MeterField {
        id: "adc_clamp_count",
        unit: "device samples per call",
        kind: Kind::Count,
        side: Side::Device,
        note: "the device samples the converter's full-scale clamp changed",
        get: |m| f64::from(m.adc_clamp_count),
    },
    MeterField {
        id: "vas_paused",
        unit: "paused (1) or recording (0)",
        kind: Kind::State,
        side: Side::Device,
        note: "the voice activation's state at the call's end",
        get: |m| if m.vas_paused { 1.0 } else { 0.0 },
    },
    MeterField {
        id: "hang_remaining_samples",
        unit: "device samples (6 kHz)",
        kind: Kind::State,
        side: Side::Device,
        note: "the non-sound device samples the gate keeps before pausing",
        get: |m| m.hang_remaining_samples as f64,
    },
    MeterField {
        id: "reconstruction_peak",
        unit: "sample peak",
        kind: Kind::Max,
        side: Side::Output,
        note: "the largest host sample after stage 10 and the conversion, before the output chain",
        get: |m| f64::from(m.reconstruction_peak),
    },
    MeterField {
        id: "route_peak",
        unit: "sample peak re the route's scale",
        kind: Kind::Max,
        side: Side::Output,
        note: "the largest sample at the route's output before the optional clamp (1.0: the speaker's clip ceiling or the earphone scale)",
        get: |m| f64::from(m.route_peak),
    },
    MeterField {
        id: "output_clamp_count",
        unit: "host samples per call",
        kind: Kind::Count,
        side: Side::Output,
        note: "the host samples the optional output clamp bounded (an emulator option)",
        get: |m| f64::from(m.output_clamp_count),
    },
    MeterField {
        id: "amp_clip_count",
        unit: "host samples per call",
        kind: Kind::Count,
        side: Side::Output,
        note: "the host samples the speaker amplifier's knee or ceiling changed",
        get: |m| f64::from(m.amp_clip_count),
    },
    MeterField {
        id: "battery_ceiling",
        unit: "factor (1.0 at 3.0 V)",
        kind: Kind::State,
        side: Side::Output,
        note: "the battery's ceiling factor in force at the call's end",
        get: |m| f64::from(m.battery_ceiling),
    },
];

/// The taps before the gate's device-rate stages (no conversion runs).
fn front(t: Tap) -> bool {
    matches!(
        t,
        Tap::AfterAgc | Tap::AfterMicCapsule | Tap::AfterMicAmp | Tap::AfterInsert
    )
}

/// The taps after stage 10 (the engine's `is_after_stage_10`, `settings.rs:109-117`).
fn after_stage_10(t: Tap) -> bool {
    matches!(
        t,
        Tap::AfterPlayback
            | Tap::RouteOutput
            | Tap::AfterVolumeAmp
            | Tap::AfterSpeaker
            | Tap::AfterEarphone
    )
}

/// Whether the configuration reaches a field's point (D4): the engine's documented rules
/// (`pipeline.rs:227-286`), pinned by behavior (`meter_reach_pinned`); rr-dr60-emu#75 asks for
/// the engine to publish them.
pub fn reached(field: &MeterField, s: &Settings, route: Route) -> bool {
    let t = s.tap;
    match field.id {
        "input_peak" | "battery_ceiling" => true,
        "limiter_input_peak" => t != Tap::AfterMicCapsule,
        "gain_reduction_db" | "hold_charge" => {
            s.agc.enabled
                && !matches!(
                    t,
                    Tap::AfterMicCapsule | Tap::AfterMicAmp | Tap::AfterInsert
                )
        }
        "record_filter_peak" => !front(t),
        "adc_clamp_count" => !front(t) && s.adc_clamp_enabled,
        "vas_paused" | "hang_remaining_samples" => crate::render::gate_reached(s),
        "reconstruction_peak" => after_stage_10(t),
        "route_peak" => t == Tap::RouteOutput,
        "output_clamp_count" => t == Tap::RouteOutput && s.output.clamp_enabled,
        "amp_clip_count" => {
            route == Route::Speaker
                && s.output.volume_amp_enabled
                && matches!(
                    t,
                    Tap::RouteOutput | Tap::AfterVolumeAmp | Tap::AfterSpeaker
                )
        }
        _ => true,
    }
}

/// The engine's meter field names, read from its `Debug` output (the struct is
/// `#[non_exhaustive]`, so the workbench cannot destructure it).
pub fn engine_field_names(m: &Meters) -> Vec<String> {
    let dbg = format!("{m:?}");
    let body = dbg
        .split_once('{')
        .and_then(|(_, b)| b.rsplit_once('}'))
        .map_or("", |(b, _)| b);
    body.split(',')
        .filter_map(|kv| kv.split_once(':').map(|(k, _)| k.trim().to_owned()))
        .filter(|k| !k.is_empty())
        .collect()
}

/// The route among a request's engine values (the route is a live control, not a setting).
pub fn route_of(engine_values: &[(u32, rr_dr60::control_map::ControlReading)]) -> Route {
    let Some((idx, entry)) = rr_dr60::controls::controls()
        .iter()
        .enumerate()
        .find(|(_, e)| e.identifier == "route")
    else {
        return Route::Speaker;
    };
    let earphone = match entry.value {
        rr_dr60::controls::ControlValue::Choice { options, .. } => options
            .iter()
            .find(|o| o.identifier == "earphone")
            .map(|o| o.value),
        _ => None,
    };
    match engine_values.iter().find(|(i, _)| *i == idx as u32) {
        Some((_, rr_dr60::control_map::ControlReading::Choice(v))) if Some(*v) == earphone => {
            Route::Earphone
        }
        _ => Route::Speaker,
    }
}

/// Where a row's figure sits on the original's clock (D3; post-implementation review B1): the
/// call's input span for the input side and the device points, and for the output side the
/// call's output span mapped through the engine's map with its last sample (`o1 − 1`), so a call
/// ending at a splice does not stretch across the removed pause. `None` for an output-side figure
/// of a call that emitted nothing.
pub fn placed_span(
    row: &MeterRow,
    field: &MeterField,
    r: &crate::render::Render,
) -> Option<(u64, u64)> {
    match field.side {
        Side::Output => {
            let (o0, o1) = row.out_span;
            if o1 <= o0 {
                return None;
            }
            let to_in = |o: u64| {
                rr_dr60::timeline::onepass_output_to_input(
                    o,
                    r.latency_samples,
                    &r.events,
                    r.drop_mode,
                )
                .0
            };
            Some((to_in(o0), to_in(o1 - 1) + 1))
        }
        _ => (row.in_span.1 > row.in_span.0).then_some(row.in_span),
    }
}

/// The plot points of a field in seconds on the original's clock: the rest point at 0, maxima
/// and counts as steps over their span, states at their span's end, and a break (NaN) at every
/// removed pause or muted region.
pub fn plot_points(field: &MeterField, r: &crate::render::Render) -> Vec<[f64; 2]> {
    let rate = f64::from(r.request.rate_hz);
    let mut pts = Vec::with_capacity(r.meters.len() * 2 + 1);
    if let Some(rest) = r.meters.first() {
        pts.push([0.0, (field.get)(&rest.meters)]);
    }
    let marks = crate::ab::marks(r);
    let mut prev_end: Option<u64> = None;
    for row in r.meters.iter().skip(1) {
        let Some((s, e)) = placed_span(row, field, r) else {
            continue;
        };
        if let Some(pe) = prev_end
            && marks
                .iter()
                .any(|m| m.input_span.0 >= pe && m.input_span.0 < s.max(pe + 1))
        {
            pts.push([pe as f64 / rate, f64::NAN]);
        }
        let v = (field.get)(&row.meters);
        if field.kind == Kind::State {
            pts.push([e as f64 / rate, v]);
        } else {
            pts.push([s as f64 / rate, v]);
            pts.push([e as f64 / rate, v]);
        }
        prev_end = Some(e);
    }
    pts
}

/// A value as shown, with its unit's companions (D2): dBFS beside a sample peak, ms beside the
/// hang, and a count as a fraction of the call's samples in its own domain.
pub fn display(field: &MeterField, row: &MeterRow, rate: u32) -> String {
    let v = (field.get)(&row.meters);
    match (field.id, field.kind) {
        ("hang_remaining_samples", _) => format!("{v} device samples ({:.1} ms)", v / 6.0),
        (_, Kind::Count) => {
            let n = match field.side {
                Side::Device => (row.in_span.1 - row.in_span.0) as f64 * 6000.0 / f64::from(rate),
                Side::Output => (row.out_span.1 - row.out_span.0) as f64,
                Side::Input => (row.in_span.1 - row.in_span.0) as f64,
            };
            format!(
                "{v} ({:.1} % of the call's samples)",
                if n > 0.0 { 100.0 * v / n } else { 0.0 }
            )
        }
        (_, Kind::Max) if field.unit.starts_with("sample peak") => {
            format!("{v:.4} ({:.2} dBFS)", 20.0 * v.max(1e-12).log10())
        }
        _ => format!("{v:.4}"),
    }
}
