# Implementation Plan: Comparisons and presets

**Branch**: `005b-comparisons-presets` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)

## Summary

The session keeps a comparison render B beside the current render A (D1). A new core module compares two renders: a parameter diff of their requests (D2), a null test (D3), and a spectral difference built on 002's spectrum (D4). The A/B playback gains a third take with a switch rule through the input (D5). Presets are a sidecar's control section under their own header, restored by the sidecar's control rules (D6).

## Technical context

Rust 2024, egui/eframe 0.37 and egui_plot, as before. Engine pinned at `a06318c`. No new dependencies and no new `Pipeline` calls (FR-008).

## Constitution check

- **I**: nothing is played or exported from the difference. The null test and the spectral difference are documented analyses of the engine's outputs, with their units and methods (D3, D4). The diff lists engine readings, and the gain oracle is the engine's published law (`output::earphone_gain_db`).
- **II**: no new methods; the ratio stays 50/70.
- **III**: presets write every control as the sidecar does.
- **IV**: the playback callback's `fill` gains a third take, still atomics and slices only, no allocation.

## Research

- **R-01 The comparison module** `crates/workbench_core/src/analysis/compare.rs`:
  - `reading(req, i)`: `control_map::settings_value(&req.settings, i)`, otherwise the request's `engine_values` entry.
  - `param_diff(a, b) -> ParamDiff { controls: Vec<(identifier, ControlReading, ControlReading)>, input, channel, start, len, tail, a_only, b_only }`. `a_only` and `b_only` are multiset differences of the `Scheduled` lists, sorted by (clock, position, control, tag); the value is compared by bits.
  - `shift(a, b) -> i64` = (L_B − L_A) − (S_B − S_A).
  - `null(a: &[f32], b: &[f32], s: i64, prefix: Option<u64>) -> NullReport`. The overlap is the indices i ≥ max(0, −s) with i < len_A and i + s < len_B. The per-span figures are `Figures { samples, differing, first, peak, peak_at, rms_db, depth_db: Option<f64> }`, with sequential f64 sums. The peak is in dBFS, `20·log10` (|d| = 0 gives −∞), and depth is `None` when RMS(A) = 0.
  - `prefix_end(a, b, s)`: the first index j where the two renders' events differ, by each event's input position (S + `onepass_output_to_input(output_position)`) and `input_length`. The prefix ends at A's output position of that event, or of B's mapped into A's coordinates by −s, whichever is earlier. With equal reports there is no prefix: it equals the overlap.
  - `compare(a, b, align) -> Result<Comparison, Refused>`: refused when the rates differ. The parameter diff is available on its own.
  - `spectral(a, b) -> Result<SpectrumPair, Refused>`: each side's `render_runs`, so splices, muted regions and the latency are excluded, cut at a live take's first skip; 002's `ratio_db`, with A in `original` and B in `render`, so the ratio is B ÷ A (hardware Q9).
- **R-02 The session** (`session.rs`): `b: Option<Render>`, `keep_as_b()` (a clone of A, an `Arc` copy of the samples), `b()`, `clear_b()`. `open` drops B wherever it drops the render.
- **R-03 The A/B** (`ab.rs`, `monitor.rs`):
  - `Take::B`, packed in bits 62 and 63 of the cursor word; `Takes.b`.
  - `ab::switch(from, pos, to, a, b, input_len, mode) -> u64` holds D5's rule. A render's input position is `start + to_original(o)`, clamped to the input. Into a render it is `to_render(i − start)`, saturating at the render's start, and the render's own map clamps to its end.
  - The window calls it for every switch.
- **R-04 Presets** (`preset.rs`):
  - `write(panel)`: the magic, `workbench.version`, `engine.version`, `engine.commit`, and a `control` line per row with `fmt_reading(row.value)`.
  - `read(text)`: the magic and version checked, unknown keys refused, controls parsed as `sidecar::read` does.
  - `restore(preset, panel)`: `sidecar::restore_controls`, the control half of `sidecar::restore`, factored out so both share one rule set. The rate is reported, never applied.
  - The sidecar reader already refuses a preset (another magic): `NotASidecar`.
- **R-05 The window**:
  - The Session tab gains **Keep as B** and **Play B**; the switch cycles through the takes in the order original → A → B.
  - A **Compare** tab shows:
    - B's and A's request summaries;
    - the parameter diff grid;
    - the alignment choice and the null figures for both spans;
    - the spectral difference plot (ratio and both spectra), with the analysis label.
  - The file bar gains **Save preset…** and **Load preset…**.
- **R-06 The gain oracle** (US1 AS4; hardware review Q1, Q4, Q5). On the earphone route stage 11 runs the 32 Hz high-pass, the loaded law's scale and the follower's 120 / 280 Hz corners. Their coefficients are position-independent, so by linearity the output scales by g, and a fresh chain takes the position at once (engine spec 011). The test asserts:
  - the premise (equal gate reports);
  - B = g·A on every sample, within half an f32 ULP of each side;
  - the ratio within 60 dB of both maxima, to 0.001 dB;
  - the null depth's closed form;
  - both orders.
  The full-window maximum is reported as the narrowing floor.
- **R-07 Recorded in review** (coach 1 – 13, hardware Q1 – Q10): see the spec's Clarifications. `NullTest` gained `whole: Option`, `cut_at_skip`, `incomplete` and `starts_differ`. The spectrum's runs are `render_runs` cut at the first skip. `ab::input_at` clamps to the take's own fed length first. `preset::PresetError` and `session::PresetLoadError` replace strings. The code for R-04 was written while these reviews ran, before tasks.md existed; this records it (coach's process note).

## Project structure

```text
crates/workbench_core/src/analysis/compare.rs   # new
crates/workbench_core/src/preset.rs             # new
crates/workbench_core/src/sidecar.rs            # restore_controls factored out
crates/workbench_core/src/session.rs            # B
crates/workbench_core/src/ab.rs, monitor.rs     # the third take
crates/workbench_core/tests/compare.rs          # US1, US2
crates/workbench_core/tests/preset.rs           # US3
crates/workbench/src/main.rs, views.rs          # Keep as B, the Compare tab, presets
```

## Testing

The spec's names, in `tests/compare.rs` and `tests/preset.rs`. `null_figures` uses hand-built vectors, never the module's own output. `spectral_difference_reads_gain` runs at seven rates, with its tolerance of 0.001 dB reproduced and its observed maximum recorded. The callback's allocation test (`monitor_alloc`) covers the third take.
