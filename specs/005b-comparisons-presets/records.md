# Records: comparisons and presets

Recorded on 2026-11-03, with the engine at `a06318c`.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 169 green, 3 ignored |
| `tests/compare.rs` | 18 green in about 12 s; `tests/preset.rs` 3 green |
| reach | REACH (005b's `Pipeline::speech_codec_info` for the coder's delay; FR-008 said no new method: restated) |
| engine issue | #78 (the one-pass reports exclude the coder's delay, and the rule for when the latency includes it is crate-private) |

## Figures

- **`spectral_difference_reads_gain`**: earphone route, uniform noise at −20 dBFS RMS, volume 1.0 and 0.5, both orders, at seven rates.
  - Premise: the gate reports are equal.
  - Every sample: B = g·A within half an f32 ULP of each side.
  - Bins within 60 dB of both maxima: worst error **6.05e-5 dB** (asserted 0.001).
  - The 120 dB window: worst **4.16e-2 dB**, the f32 narrowing floor (reported).
  - Null depth: worst **3.75e-7 dB** from 20·log10|1 − g| (asserted 0.001).
- **`null_codec_latency_reported`**: the coder off against on.
  - s is the coder's published delay: 180 at 6 kHz, 1440 at 48 kHz.
  - The depth: −8.0 dB at 6 kHz, −9.7 dB at 48 kHz.
  - Every sample differs.
- **`null_start_aligned`**: a start of 100 ms of zeros on the quiet front gives s = −S_B and a bit-for-bit null, at 6, 44.1 and 48 kHz.
- **`null_latency_aligned`**: on hand-built renders, a delay of k gives s = k and nothing differs.
- **`null_prefix_drop_mode`**: a −43 dBFS tone under thresholds −43 / −40 dBFS, codec off and on, at 6 and 48 kHz.
  - Premise: the first reports agree.
  - The prefix nulls bit for bit, and the first real difference is not before its end.
- **`null_end_paused`**: the prefix ends at the paused side's end, with the label set.
- **`codec_reports_agree`**: the coder off against on with equal gates gives no prefix.
- **`spectral_different_runs`**: drop against mute on noise with a 2 s gap.
  - The runs' positions differ, the frames are the same, and the ratio reads 0.000 dB in the voice band.
  - This pins the run mapping, not Welch variance.
- **`diff_lists_one`**: a one-hot over every control lists exactly that control; the player's live values read as absent on a one-pass request.
- **`ab_three_takes`**: all nine ordered pairs in both modes, with different starts, the before-start asymmetry, and the end clamps.
- **`third_take_packs_and_plays`** and `monitor_alloc`: the three takes, with no allocation.

## Reviews, applied

- **Spec and plan** (coach 1 – 13, hardware Q1 – Q10): the spec's first Clarifications session.
- **Post-implementation** (coach 1 – 12, hardware B1 – B6): the spec's second session.
  - **Coach**:
    - **1**: the Compare tab's cache keys on the requests too.
    - **2**: Load preset is refused while a live take runs.
    - **3**: the prefix test asserts its premise and that the prefix nulls.
    - **4**: the empty-span and zero-difference text.
    - **5**: Keep as B says that it stopped the playback.
    - **6**: the preset's left-at-default check.
    - **7**: the end clamps.
    - **8**: the spectrum refused on no output.
    - **9**: the channel, length and clock are written as the sidecar writes them.
    - **10**: each side's input span.
    - **11**: the A/B cycle's hover text.
    - **12**: the coder's published delay as an independent oracle for s.
  - **Hardware**:
    - **B1**: the end pause.
    - **B2**: the reports' latency less the coder's delay, with the conservative prefix end; engine issue #78.
    - **B3**: the skip as a label, not a cut, and the `LiveInfo::skips` doc corrected.
    - **B4**: the codec-on prefix.
    - **B5**: the different-runs test.
    - **B6**: the request race, documented as accepted (one buffer at most).
    - Packing: `pack`'s debug assertion.
- **Follow-up**: 001's A/B map passes the full latency to `onepass_output_to_input`, so with the coder on, a switch inside a splice's coder-delay window lands early (hardware B2). Engine issue #78 covers the latency; the workbench's map is to be revisited once the engine answers.
