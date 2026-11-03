# Data model: taps, scopes and meters

All of it lives in `workbench_core` with no UI dependency.

- **MeterRow** `{ meters: rr_dr60::Meters, in_span: (u64, u64), out_span: (u64, u64), tail: bool }`. `Render.meters: Vec<MeterRow>`. Row 0 is the rest point, with empty spans at 0.
- **MeterField** (a constant table in `meters.rs`) `{ id: &'static str, unit, kind: Max | Count | State, side: Input | Device | Output, note: &'static str (the rustdoc note, labelled), get: fn(&Meters) -> f64 }`. `reached(field, &Settings) -> bool` is the R-03 rule.
- **Envelope** `{ tiers: Vec<(u32 /*samples per pair*/, Vec<(f32, f32)>)> }`.
- **Spectrum** `{ rate, n, overlap: 0.5, frames: u32, power: Vec<f64> /*per bin, linear, sine-calibrated*/ }`, with `db(k)` and `freq(k) = 5·k`.
- **SpectrumPair** `{ original: Spectrum, render: Spectrum, range_in: (u64, u64), range_out: (u64, u64), dropped_frames: u32 }`, with `ratio_db(k) -> Option<f64>` (the 120 dB window).
- **Spectrogram** `{ cols: u32, bins: u32, data: Vec<u8>, frame_n, hop }`.
- **TapResult** `{ tap: &'static str, outcome: Ok(Readings { length, latency, tail, splices, not_emitted, peak, sha256, spectrum }) | Refused(String) }`. `TapTable { request: RenderRequest, rows: Vec<TapResult> }` goes stale by 001's rule.
