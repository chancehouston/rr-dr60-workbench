//! The meter timeline (spec 002 D1 – D4; research R-03).
//!
//! A render reads the engine's rest read-out, then the read-out after every 10 ms call. The
//! field table is the engine's (`rr_dr60::meters::FIELDS`, engine spec 026 D4; spec 008 D5):
//! identifier, unit, kind and sentence. Reach and lag come from the render's own engine
//! (the engine's `meter_reached` / `meter_lag_samples`). The workbench adds only the value
//! accessors and the device-rate split of the input side.

use rr_dr60::Meters;

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

/// One meter field: the engine's description with the workbench's accessor.
#[derive(Clone, Copy, Debug)]
pub struct MeterField {
    /// The engine's field identifier.
    pub id: &'static str,
    /// The engine's unit.
    pub unit: &'static str,
    /// Max, count or state (the engine's kind).
    pub kind: Kind,
    /// Its clock: the engine's side, the input side split at the device-rate points.
    pub side: Side,
    /// The engine's sentence.
    pub note: &'static str,
    /// The value as f64.
    pub get: fn(&Meters) -> f64,
}

const fn kind_of(k: rr_dr60::meters::MeterKind) -> Kind {
    match k {
        rr_dr60::meters::MeterKind::Maximum => Kind::Max,
        rr_dr60::meters::MeterKind::Count => Kind::Count,
        _ => Kind::State,
    }
}

/// The device-rate points (stages 4 – 5), the input-side fields whose record-call lag is the
/// record path's latency to the encoder rather than 0. Pinned against the engine's
/// `meter_lag_samples` at seven rates (`tests/meters.rs`).
const DEVICE_POINTS: [usize; 4] = [4, 5, 6, 7];

const fn side_of(i: usize) -> Side {
    match rr_dr60::meters::FIELDS[i].side {
        rr_dr60::meters::MeterSide::Output => Side::Output,
        _ => {
            let mut k = 0;
            while k < DEVICE_POINTS.len() {
                if DEVICE_POINTS[k] == i {
                    return Side::Device;
                }
                k += 1;
            }
            Side::Input
        }
    }
}

const fn field(i: usize, get: fn(&Meters) -> f64) -> MeterField {
    let e = &rr_dr60::meters::FIELDS[i];
    MeterField {
        id: e.identifier,
        unit: e.unit,
        kind: kind_of(e.kind),
        side: side_of(i),
        note: e.sentence,
        get,
    }
}

/// The thirteen fields, in the engine's declaration order.
pub const FIELDS: [MeterField; 13] = [
    field(0, |m| f64::from(m.input_peak)),
    field(1, |m| f64::from(m.limiter_input_peak)),
    field(2, |m| f64::from(m.gain_reduction_db)),
    field(3, |m| f64::from(m.hold_charge)),
    field(4, |m| f64::from(m.record_filter_peak)),
    field(5, |m| f64::from(m.adc_clamp_count)),
    field(6, |m| if m.vas_paused { 1.0 } else { 0.0 }),
    field(7, |m| m.hang_remaining_samples as f64),
    field(8, |m| f64::from(m.reconstruction_peak)),
    field(9, |m| f64::from(m.route_peak)),
    field(10, |m| f64::from(m.output_clamp_count)),
    field(11, |m| f64::from(m.amp_clip_count)),
    field(12, |m| f64::from(m.battery_ceiling)),
];

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
            let to_in = |o: u64| crate::render::output_to_input(o, r).0;
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
    let mut prev_out = 0u64;
    for row in r.meters.iter().skip(1) {
        let Some((s, e)) = placed_span(row, field, r) else {
            continue;
        };
        // A break where a mark's output position falls after the previous plotted call's
        // output and up to this call's end (the marks sit at the heard point, the report's
        // position plus the coder's delay plus the playback half's latency: engine 0.95, spec 028).
        let out_end = if field.side == Side::Output {
            row.out_span.1
        } else {
            0
        };
        let by_output = field.side == Side::Output
            && marks
                .iter()
                .any(|m| m.output_position >= prev_out && m.output_position < out_end);
        if let Some(pe) = prev_end
            && (by_output
                || marks
                    .iter()
                    .any(|m| m.input_span.0 >= pe && m.input_span.0 < s.max(pe + 1)))
        {
            pts.push([pe as f64 / rate, f64::NAN]);
        }
        if field.side == Side::Output {
            prev_out = row.out_span.1;
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
        (_, Kind::Max) if field.unit.starts_with("full scale") => {
            format!("{v:.4} ({:.2} dBFS)", 20.0 * v.max(1e-12).log10())
        }
        _ => format!("{v:.4}"),
    }
}
