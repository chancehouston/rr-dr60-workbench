# Implementation Plan: Taps, scopes and meters

**Branch**: `002-taps-scopes-meters` | **Date**: 2026-11-02 | **Spec**: [spec.md](spec.md) (in force under autopilot)

## Decisions (in force under autopilot; the rationale is in research.md)

| # | Decision |
|---|---|
| P1 | `rustfft` 6.4 in `workbench_core` for every FFT. N = rate ÷ 5 for spectra and rate ÷ 25 (or ÷ 100) for the spectrogram, each planned once (R-01). |
| P2 | The meter timeline is a field of `Render` (`Vec<MeterRow>`). The render block becomes `rate / 100` (R-03). |
| P3 | The meter field table is the workbench's, quoted from the engine's rustdoc, with a `reached` rule pinned by behavior. The engine's own table is rr-dr60-emu#75 (R-03). |
| P4 | `egui_plot` 0.38 for the meter and spectrum plots, and egui painting for the envelopes and the spectrogram texture (binary only). |
| P5 | The analysis runs on worker threads with progress and cancel, like 001's render (spectrogram, tap browser). The spectra of a selection are computed on demand. |
| P6 | CSV with `#` method lines, and 001's sidecar beside it (R-09). |

## Technical Context

Rust stable. New dependencies: `rustfft` 6.4 (core) and `egui_plot` 0.38 (binary). Testing with `cargo test`. Performance goals per SC-004. Memory below one f32 copy for the analysis (the envelope at about 1/8, the spectrogram u8 at ≤ 4096 columns).

## Constitution Check

| Principle | Status |
|---|---|
| I. No DSP on the audio path: the analysis reads the render and never feeds the engine or the export. The meter descriptions are labelled workbench notes until the engine publishes them (#75). | Pass |
| II. `meters`, `rest_meters` and the stage manifest come into use; reach 14/70. | Pass |
| III. Every CSV carries the method and the render's sidecar. | Pass |
| IV. Playback is unchanged. | Pass |
| V. All analysis is in the core, tested without a window. | Pass |
| VI. Methods are stated; "sample peak", "dB re full-scale sine", "spectral ratio (not a transfer function)", "not reached". | Pass |

## Project Structure

```
crates/workbench_core/src/{meters.rs, taps.rs, stages.rs, csv.rs, analysis/{mod.rs, envelope.rs, spectrum.rs, spectrogram.rs}}
crates/workbench_core/tests/{meters.rs, spectrum.rs, envelope.rs, spectrogram.rs, taps.rs, stages.rs, csv.rs, speed_002.rs}
crates/workbench/src/{main.rs (tabs), views/{scopes.rs, meters.rs, taps.rs, stages.rs}}
```

## Traceability

| Requirement | Test |
|---|---|
| FR-001, SC-001, US1 AS1 | `meters::meters_timeline_equals_engine` (seven rates) |
| FR-002 | `render_oracle` (unchanged), `meters::block_independent_restore` |
| FR-003, US1 AS6 | `meters::meter_fields_named` |
| FR-004, US1 AS3 | `meters::meter_reach_pinned` |
| US1 AS2 | `meters::meters_limiter_burst` |
| FR-005, US1 AS4, AS5 | `envelope::envelope_bounds`, `envelope::waveform_alignment_marks` |
| FR-006, SC-002, US2 AS1 – AS4 | `spectrum::{spectrum_calibration_bin_200, spectrum_noise_density, spectrum_identity_ratio, spectrum_ratio_arithmetic, matches_naive_dft}` |
| FR-007 | `spectrogram::spectrogram_columns` |
| FR-008, SC-003, US3 AS1 | `taps::{tap_browser_digest_equals_render, tap_browser_cancel}` |
| FR-009, SC-005, US3 AS2 | `stages::stages_verbatim` |
| FR-010, US2 AS5, US3 AS3 | `csv::{spectrum_csv_roundtrip, meters_csv_roundtrip, tap_csv_roundtrip}` |
| FR-011 | `source_scan`, the analysis only reads `Render` and `Input` |
| FR-012 | `wording` |
| FR-013 | `reach` (14/70) |
| SC-004 | `speed_002` (release, ignored) |
| SC-006 | the desk checklist |
