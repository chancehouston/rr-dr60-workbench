# Plan: Engine 0.93 adoption

All changes in `workbench_core` (logic and tests) and the thin `workbench` binary.

- **R-01 (D2)** `render::gate_reached` deleted; callers use `settings.one_pass_vas_runs()`. `Render.gate_reached` keeps its name (the sidecar key `render.gate_reached` is unchanged).
- **R-02 (D3)** `render::tail_for(p)` returns `u64::from(p.one_pass_drain_samples())`. `text::TAIL_LABEL` reworded. The `tail` tests assert every input sample is emitted and the tail equals the engine's figure.
- **R-03 (D4)** `tests/wording.rs` drops `engine_root`, `const_list`, `suffixes` and `Denylist::load`; its matcher calls `rr_dr60::wording::first_hit` and `first_claim_hit`, then the supplementary list.
- **R-04 (D5)** `meters::FIELDS` built in a const from `rr_dr60::meters::FIELDS` (identifier, unit, kind, sentence) with the workbench's accessors; `Side::Device` for the input fields whose record-call lag is positive, pinned by a test at seven rates. `meters::reached` deleted; `Render` and the live run carry `meter_reach: [bool; 13]` from `Pipeline::meter_reached(i, MeterCall::OnePass)` after the route is set; `Render.meter_lag: [Option<u32>; 13]`. The dB companion keys on the engine's unit "full scale".
- **R-05 (D6)** `render::codec_delay(p)` = `latency_samples − one_pass_report_latency_samples`. A new `render::output_to_input(o, r)` = `onepass_output_to_input(o − C, L − C, events, drop)` for o ≥ C (o < C: the clamped start), used by `ab::to_original`, `meters::placed_span` and `analysis::spectrum`; `compare::reports` unchanged in effect.
- **R-06 (D7)** `stages::stage_fields` and `header_lines` use `label()`; the tour's words likewise.
- **R-07 (D8)** `reach.rs`: 74, and `Pipeline::one_pass_drain_samples(` etc. are called.
- **R-08** Records: CHANGELOG, CLAUDE.md, the plan of record's table (008), records.md.
