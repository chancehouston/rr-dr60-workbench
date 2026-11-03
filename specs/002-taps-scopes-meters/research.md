# Research: taps, scopes and meters

Phase 0 of the plan. The engine is pinned at `a06318c`. The figures marked "verified" come from the reviewers' scratch arithmetic, and the harness reproduces them before the records quote them.

## R-01 The FFT: rustfft 6.4

- **Decision**: `rustfft` 6.4 (MIT/Apache) in `workbench_core`, planned once per size and reused.
- **Rationale**: mixed-radix sizes (1200 … 19 200) are first-class in rustfft. The result is deterministic on one host. SIMD paths may differ in the last bits between CPUs, so exports agree within 1e-6 dB, which a test bounds against a naive DFT on one size.
- **Rejected**: a hand-written radix-2 FFT (it cannot do N = rate ÷ 5).

## R-02 The spectrum (D6)

- **Frames**: N = rate ÷ 5, Hann w[n] = 0.5 − 0.5·cos(2πn/N) (periodic), hop N/2. Power per bin is averaged over the frames.
- **Scale**: P_k = |X_k|² · 4/(Σw)², so a sine of amplitude A at a bin centre reads A² (20·log10 A dB). White noise of variance σ² gives E[P_k] = 4σ²·Σw²/(Σw)² = 4σ²·ENBW/N, with ENBW = N·Σw²/(Σw)² = 1.5 bins. With N = 9600 and σ² = 1 that is 10·log10(6/9600) = −32.04 dB, and the hardware review reproduced −52.045 against −52.041 for its σ. "dB/Hz" subtracts 10·log10(1.5 × 5 Hz).
- **Calibration test**: bin 200 = 1 kHz at every rate, and a full-scale 1 kHz sine reads 0 dB within 0.05. The verified alternative at N = 8192 reads −0.627 dB, which is why the size rule changed.
- **Noise test**: 30 s of seeded Gaussian noise, the mean of the bins from 10 % to 90 % of Nyquist, within 0.05 dB (a per-bin spread of 0.18 dB over 60 s).
- **Kept spans** (drop mode): the original's frames are taken only from input spans the render kept, from the render's reports. A render frame is dropped when it contains a splice position. Both counts are kept.
- **The ratio**: render dB − original dB per bin, drawn only where both sides are within 120 dB of their own range's largest bin.

## R-03 The meter timeline (D1 – D4)

- **Block** rate ÷ 100: the seven rates' greatest common divisor is 100 (verified), and every call carries 60 device samples.
- **Rows**: `rest_meters()` after the values are applied, then `meters()` after each call. Each row stores the input span [in₀, in₁) of the call and its output span [out₀, out₁).
- **Kinds and sides**, from the field docs (`pipeline.rs:227-286`):

  | Field | Kind | Side | Reached (the engine's docs, pinned by behavior) |
  |---|---|---|---|
  | `input_peak` | max | input | always |
  | `limiter_input_peak` | max | input | not at `AfterMicCapsule` |
  | `gain_reduction_db` | max | input | limiter on and the tap after it |
  | `hold_charge` | state | input | as above |
  | `record_filter_peak` | max | device | the tap at or after stage 4 |
  | `adc_clamp_count` | count | device | as above, clamp on |
  | `vas_paused` | state | device | `gate_reached` (001) |
  | `hang_remaining_samples` | state | device | `gate_reached` |
  | `reconstruction_peak` | max | output | the tap after stage 10 |
  | `route_peak` | max | output | `RouteOutput` only |
  | `output_clamp_count` | count | output | `RouteOutput` with the clamp on |
  | `amp_clip_count` | count | output | speaker route, stage 11 on, at a tap that runs the knee |
  | `battery_ceiling` | state | output | always (a state of the engine) |

- **The pin test**: at every tap × the relevant switches, a stimulus that drives every field: a +12 dBFS 1 kHz burst, then 3 s of silence, with the volume at 1.0 and the output clamp on.
  - A field predicted unreached must equal its rest value in every row (strong).
  - A field predicted reached must move from rest at least once. Where this stimulus cannot move it, the test states why.
- **Field names**: parsed from `format!("{:?}", rest_meters())` (the struct is `#[non_exhaustive]`) and compared with the table above.

## R-04 The identity render (US2 AS3)

- **Candidate**: at 48 kHz, the tap `AfterRecord` with the microphone stages, the slot, the limiter, the record filter and the ADC clamp all bypassed. The input then passes the conversion to 6 kHz and back, a band-limit and a delay, with no other stage. Its ratio should read 0 dB within 0.1 below about 2.4 kHz.
- The implementation confirms or replaces this candidate before the test pins it. If the conversion's own passband ripple exceeds 0.1 dB, the tolerance comes from the engine's published conversion figures, quoted.

## R-05 Envelopes (D5)

Tiers of min/max pairs at 1, 16, 256 and 4096 samples per pair, built once per take. A view picks the finest tier that is no finer than one pixel and folds it to columns. The memory is about 2/16 of a take. Bounds test: every column's [min, max] contains every sample of its span.

## R-06 The spectrogram (D7)

N = rate ÷ 25 (40 ms; 10 ms wideband: rate ÷ 100), hop rate ÷ 100 (10 ms), Hann, the same scale as R-02. Output: u8 indices of −120 … 0 dB in 0.5 dB steps (0 below, 240 at 0 dB, 255 saturated above), at most 4096 columns (a column is the maximum over its frames), computed on a thread with progress and cancel.

## R-07 The tap browser (D8)

For each option of the tap control: a request with that tap, rendered by 001's `render`, its readings, `output_sha256`, and a spectrum of the whole render without its tail, with the original side from that tap's own kept spans. Refusals (`TapNotOnRoute`) come from 001's panel check, run on a copy of the panel. Cancel is checked between taps.

## R-08 The stages (D9)

`rr_dr60::manifest()`: the header fields and `stages()`, shown verbatim. A test compares every shown string with the manifest's fields, and the controls per stage come from `controls()` by `stage`.

## R-09 CSV (D12)

UTF-8 with `\n` line ends. `#` comment lines for the method and the engine commit, one header row, comma-separated numbers (f32 as shortest round-trip decimals). The render's sidecar is written beside each CSV, with the same stem plus `.sidecar.txt`, through 001's `sidecar::write`.
