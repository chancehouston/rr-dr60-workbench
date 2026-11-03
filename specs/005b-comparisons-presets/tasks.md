---
description: "Tasks for feature 005b, comparisons and presets"
---

# Tasks: Comparisons and presets

- [X] T001 [US3] `sidecar::restore_controls` factored out of `restore`; `preset.rs` (`write`, `read`, `restore`, `PresetError`); `Session::load_preset` (`PresetLoadError`) (plan R-04, R-07).
- [X] T002 [US3] Tests `preset_roundtrip`, `preset_reports`, `preset_is_not_a_sidecar` in `tests/preset.rs`; `preset_needs_input_and_clears_range` in `tests/compare.rs`.
- [X] T003 [US1] `analysis/compare.rs`: `reading`, `param_diff`, `shift`, `figures`, `overlap`, `prefix_end`, `null_test` (skips, incomplete, starts), `spectral` (plan R-01, R-07).
- [X] T004 [US1] Tests in `tests/compare.rs`: `null_identical`, `null_figures`, `diff_lists_one` (one-hot), `diff_changes_multiset`, `spectral_difference_reads_gain` (seven rates, both orders), `null_codec_latency_reported`, `null_start_aligned`, `null_latency_aligned`, `null_no_overlap`, `null_prefix_drop_mode`, `rates_differ`, `compare_live_skips`.
- [X] T005 [US2] `Session::{b, keep_as_b, clear_b}` and B dropped with the render; `monitor::Take::B` (two bits) and `Takes.b`; `ab::{input_at, render_at, switch}` (plan R-02, R-03).
- [X] T006 [US2] Tests `ab_three_takes`, `b_follows_the_file` (`tests/compare.rs`), `third_take_packs_and_plays` (`tests/monitor.rs`), the third take in `monitor_alloc`.
- [X] T007 The window: Keep as B and Play B, the three-take switch and playhead through `ab::switch`, the Compare tab, Save preset… and Load preset… (plan R-05).
- [X] T008 The gate; the reach ratio unchanged (50/70); the wording check over the new strings (FR-007, FR-008).
- [ ] T009 Post-implementation reviews (coach and hardware), applied before the PR (records.md).
