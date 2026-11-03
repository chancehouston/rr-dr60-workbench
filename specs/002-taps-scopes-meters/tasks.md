---
description: "Tasks for feature 002, taps, scopes and meters"
---

# Tasks: Taps, scopes and meters

**Input**: [spec.md](spec.md), [plan.md](plan.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/).
**Tests**: test-first per story, each tied to the plan's Traceability. The plan review (coach 16) noted that implementation began before this file: the meter timeline and the 10 ms block were started while the plan reviews ran, and this list records the order the work took, with the tests written before each module's completion.

## Phase 1: Setup

- [X] T001 Add `rustfft` 6.4 (core) and `egui_plot` 0.38 (binary) to the workspace and the crates.

## Phase 2: Foundational

- [X] T002 `render::block_for(rate)` = rate ÷ 100; `RenderRequest::new` uses it (D1).
- [X] T003 `Render.meters`: the rest read-out after the values are applied, then one `MeterRow` per call with its input and output spans. A row is a tail row when its input span starts at or after the input's end (`render.rs`, `meters.rs`).

## Phase 3: US1, the meters and the waveforms (P1)

- [X] T004 [US1] Tests `meters::{meters_timeline_equals_engine, meter_fields_named, meters_limiter_burst, meter_reach_pinned, block_independent_restore, empty_render_rows}`.
- [X] T005 [US1] `meters::{FIELDS, reached, engine_field_names, route_of}`, from the engine's rustdoc, pinned by behavior; issue rr-dr60-emu#75.
- [X] T006 [US1] Tests `envelope::{envelope_bounds, waveform_alignment_marks, analysis_memory_below_input}`.
- [X] T007 [US1] `analysis::envelope` (tiers of 16 / 256 / 4096) and `ab::marks`.

## Phase 4: US2, the spectra (P2)

- [X] T008 [US2] Tests `spectrum::{spectrum_calibration_bin_200, spectrum_noise_density, matches_naive_dft, spectrum_identity_ratio_exact, spectrum_identity_ratio, spectrum_ratio_arithmetic}` and `spectrogram::spectrogram_columns`.
- [X] T009 [US2] `analysis::spectrum` (N = rate ÷ 5; kept spans and splice-free runs from the engine's map) and `analysis::spectrogram` (u8 scale, −120 … +7 dB).

## Phase 5: US3, the taps and the stages (P3)

- [X] T010 [US3] Tests `taps::{tap_browser_digest_equals_render, tap_browser_cancel}` and `stages::stages_verbatim`.
- [X] T011 [US3] `taps::browse` (the engine's refusal text) and `stages`.

## Phase 6: Exports, views and polish

- [X] T012 Tests `csv::{spectrum_csv_roundtrip, meters_csv_roundtrip, tap_csv_roundtrip}`; implement `csv` with the sidecar beside each file.
- [X] T013 `crates/workbench/src/views.rs`: the tabs Scopes, Meters, Taps and Stages; `main.rs` wiring.
- [X] T014 `reach` lists `meters` and `rest_meters` and asserts ≥ 14 (14/70). `source_scan::analysis_only_reads`. `render_oracle` adds block 4096.
- [X] T015 `speed_002` (release) and a re-run of 001's `render_speed` at the 10 ms block.
- [X] T016 Records: `records.md`; 001's sidecar contract note on the block; README and CLAUDE.md; CHANGELOG.
- [X] T017 Post-implementation reviews (coach, hardware), applied before the PR.
