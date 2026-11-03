---
description: "Tasks for feature 005, the processing records"
---

# Tasks: Processing records

- [X] T001 [US1] `device.rs`: `RecordOutcome.{stop, counter}`, the stop's buffer by one second, the defensive count checks, `record_from` keeping a `Run` by counter for kept and self-stopped recordings (plan R-01).
- [X] T002 [US1] `records.rs`: `RunSource`, `Run`, `feed_point_i24`, `i24_bytes`, the record-input file (write, read; contracts/record-input-v1.md), `files` (plan R-02 – R-04).
- [X] T003 [US1] `records.rs`: `Recipe`, `restore_slot`, `filler`, `re_render`, `prepare` and its refusals (plan R-07, R-07b).
- [X] T004 [US1] Tests in `tests/records.rs`: `record_written_as_engine`, `re_render_reproduces` (sixteen cells, seven rates), `re_render_detects_deltas` (the plan's table, the twins), `re_render_refuses_other_input`, `runs_follow_the_counter`, `imported_item_record`, `feed_point_rule` (with the `.i24` re-render), `re_render_refills_schedule`, `re_render_from_offset`, `re_render_unknown_component`, `re_render_cancel`, `slot_restored`, `live_value_twins_reproduce`, `late_hang_change_keeps_recording`, `filler_has_the_length`, `companion_paths_beside_the_record`.
- [X] T005 [US2] `Device::{read_record, check_record}`, `records::{flag_names, change_rows, unknown_settings, settings_diff}` (plan R-05, R-06).
- [X] T006 [US2] Tests: `record_readout_shown`, `settings_diff_lists_differences`, `check_against_item`, `record_refusals`, `record_other_rate`, `flags_label_expected`, `flag_names_every_bit`, `record_input_roundtrip_and_refusals`, `negative_zero_equals_zero`.
- [X] T007 The Device tab: Save record… per item, the Processing record section (open, read-out, flags, diff, check against item, re-render with progress and cancel) (plan R-08).
- [X] T008 `reach` lists the nine methods (FR-005, at least 50/70); `wording` covers the new strings (FR-006).
- [X] T009 Post-implementation review (coach 1 – 13, hardware B1 – B6), applied before the PR (records.md).
