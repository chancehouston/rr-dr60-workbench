# Records: taps, scopes and meters

Recorded on 2026-11-02 on the development Mac, with the engine at `a06318c`.

## The gate

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 77 green, 2 ignored (the release timings) |
| reach | **14/70** |
| SC-004, release, one minute at 48 kHz | render with the timeline 0.540 s (6014 rows), spectrum pair 0.064 s, spectrogram 0.051 s, tap browser 3.732 s |
| 001's `render_speed` at the 10 ms block | 0.657 s (was 0.543 s at 4096) |
| Launch | the window runs 5 s with an empty log |

## Figures, as the tests printed them

- **Spectrum calibration**: a full-scale 1 kHz sine reads −0.0000 / +0.0000 dB at bin 200 at all seven rates (N 1200 … 19 200, 9 frames).
- **Noise density**: σ 0.1, 30 s at 48 kHz: −52.0434 dB, against −52.0412 expected from σ and the realized-variance figure (within 0.05 dB).
- **Against a naive DFT**: within 1e-6 dB at six bins.
- **The engine identity** (48 kHz, `after_record`, every band and level stage bypassed): the worst |ratio| over 100 – 2400 Hz is **0.0034 dB**, with latency 476. The synthetic identity (a delay of 237) reads 0 within 1e-9 in every drawn bin.
- **The limiter burst** (+9 dBFS, 1 kHz, from its peak): the deepest gain reduction is 9.87 dB and the highest hold 0.843.
- **The reach pin** at 16 kHz, 12 taps × three cells:
  - every field predicted unreached stayed at rest in every row;
  - every field predicted reached moved, except the output clamp on the earphone route, whose route peak stays at or under 1.0;
  - the battery ceiling equals `ceiling_factor(V)` in every row at 3.0 and 2.4 V.
- **The tap browser** at seven rates: every accepted tap's digest equals a direct render. Exactly one tap is refused on the speaker route (`after_earphone`), with the engine's words.

## Restatements

- The tail row rule: a row is tail when its input span starts at or after the input's end (plan review, coach 12).
- **001's block**: the render block is now rate ÷ 100. A 001 sidecar's `block 4096` is informational. The output digest is the same at either block at seven rates (`block_independent_restore`), and sidecar v1 is unchanged.
