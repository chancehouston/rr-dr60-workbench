# Plan: Engine 0.94 adoption

All changes in `workbench_core` (logic and tests) and the thin `workbench` binary.

- **R-01 (D1)** `Cargo.toml`'s `rr_dr60` rev `ad1ba8d401228c91480f12bd8ff3df68ac489e7f`; CHANGELOG `### Changed (010)`.
- **R-02 (D2, D4)** `records.rs`: `Recipe` and `filler` deleted; `plan_of(bytes, readout)` (the two refusals, then `Plan::from_record`); `Prepared.plan`; `re_render(plan, fed, progress, cancel)` on `ReRender::new` / `feed_f32` / `finish`; `From<Error>` maps `ImportedRecord`, `UnknownComponent` and `InputShort` to their variants.
- **R-03 (D3)** `records::Outcome { digest: Option<u64>, expected, verdict, matches_original, rows_landed }`, `verdict_words()`; `main.rs`'s result line.
- **R-04 (D5)** `device.rs`: `Device::new_at_counter`, `encoding`, `fill_to_remaining` (`Pipeline::fill_to_remaining(` by name: the device defines a method of the same name), and the plain `Pipeline::record` when `changes` is empty and nothing is pending on the record stream; `main.rs`: the status line's encoding, **Fill the memory** with a "leave (s)" field, **Reset device at counter** with a counter field.
- **R-05 (D6, D7)** `tests/records.rs` on the plan; `tests/device.rs`: `plain_record_twin`, `fill_and_encoding`, `new_at_counter_twin`; `tests/reach.rs`: 75 and `fill_to_remaining` listed; `restore_slot` private.
- **R-06** Records: CHANGELOG, CLAUDE.md, the plan of record's table (010), records.md.
