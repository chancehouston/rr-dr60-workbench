# Feature Specification: Taps, scopes and meters

**Feature Branch**: `002-taps-scopes-meters`

**Created**: 2026-11-02

**Status**: Draft. Under the owner's autopilot rule (2026-11-02), the decisions below take effect at their recommendations once the reviews are applied.

**Input**: Feature 002 of docs/workbench-plan.md: "Waveform and spectrum views of the input, the tap and the difference. The per-call meters plotted over time. The stage manifest as a panel per stage, with its sources and assumptions." It builds on feature 001's session.

## Decisions for sign-off

| # | Decision | Recommended |
|---|---|---|
| D1 | **Meter timeline.** A render reads the engine's meters after every processing call, plus the engine's rest meters before the first call. The render's block becomes **10 ms of host samples** (rate ÷ 100: 60 at 6 kHz, 480 at 48 kHz, 960 at 96 kHz) instead of 4096, so the meters have a 10 ms resolution. The output is independent of the block, as 001's oracle at block sizes 0 and 1 shows. The sidecar records the block. | **10 ms blocks; a meter row per call** |
| D2 | **Meter figures.** Every field of the engine's meter read-out is plotted against time, with its unit and the engine's own one-line meaning: the input peak, limiter-input peak, gain reduction (dB), hold charge (0 – 1), record-filter peak, ADC-clamp count, gate paused, hang remaining, reconstruction peak, route peak, output-clamp count, amplifier-clip count and battery ceiling. A peak is shown linear and in dBFS. The fields are read from the engine at run time; when a field is added to the engine, a test fails until the plan names it. | **All thirteen, with units** |
| D3 | **Waveforms.** The original and the render are drawn as two stacked waveform strips on one time axis: a min/max envelope per pixel column, with zoom, scroll and the playhead. The render's strip is placed on the original's clock through the engine's one-pass map (001's map, display only), so content lines up. "Raw" places it on its own clock. A removed pause shows as a gap marker. | **Stacked, aligned by the engine's map** |
| D4 | **Spectra and the difference.** An averaged magnitude spectrum of a selected time range of each take: Hann window, a fixed resolution of about 5.9 Hz (FFT size scaled with the rate: 1024 at 6 kHz, 8192 at 48 kHz), 50 % overlap, dB relative to a full-scale sine (a full-scale sine at a bin centre reads 0 dB), on a log frequency axis up to Nyquist. The original's spectrum, the render's and their **difference** (render − original, in dB) are drawn together. The render's range is the original's range mapped through the engine's map. A time-domain null test is feature 005, where two renders of one input can be aligned exactly. | **Welch spectra and the spectral difference** |
| D5 | **Spectrogram.** A short-time spectrum image of either take: Hann, 20 ms frames (FFT size scaled with the rate), 50 % overlap, −120 … 0 dB in a fixed colour scale. Display only, computed in the background for long files. | **Included** |
| D6 | **Tap browser.** "Render at every tap" renders the session's input at each tap of the engine's tap control, on a fresh engine per tap with the session's values. A tap the route refuses is skipped with the engine's reason. Each tap keeps its readings (length, latency, tail, splices, not emitted, peak) and its spectrum, not its samples, so memory stays small. To listen to a tap, the user sets it in the panel and renders. | **Every tap; readings and spectra only** |
| D7 | **Stage panels.** A Stages view lists the stage manifest's header (library version, device rate, counts by status, its basis sentence, links) and one panel per stage, verbatim from the engine: number, name, status, path, confidence, description, the "not modeled here" note, the controls note, model version, sources and assumptions. Each panel also lists that stage's controls from the control manifest. Taps are not attached to stages, because the engine publishes no mapping. | **Verbatim, with the stage's controls** |
| D8 | **Layout.** The central area gains tabs: Session (001), Scopes (waveforms, spectra, spectrogram), Meters, Taps and Stages. The controls panel stays on the left. | **Tabs** |
| D9 | **Analysis lives in the core** and never on the audio path. The FFT is `rustfft` (MIT/Apache), and the analysis is deterministic. Each figure states its method on screen and in its export. | **Core analysis, stated methods** |
| D10 | **Export of figures.** Every view's data can be exported as CSV with a header naming the method, rate, range, window and engine commit, for quoting in an issue: the spectra and difference, the meter timeline and the tap table. Pictures are not exported. | **CSV with a method header** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what the engine did, over time (Priority: P1)

After a render, the user sees the two waveforms aligned, and the engine's meters over time: where the limiter reduced gain, where the gate paused, where the output clipped.

**Independent Test**: Render known stimuli and compare the meter timeline with the engine's meters read directly after the same calls. Check the timeline's length and times. Check that the waveform envelopes bound the samples.

**Acceptance Scenarios**:

1. **Given** a render, **When** the Meters tab opens, **Then** every engine meter field is plotted over time with its unit and meaning. The first point is the engine's rest read-out, and each later point equals the engine's read-out after that call.
2. **Given** a burst at +9 dBFS into the default engine, **When** it renders, **Then** the gain-reduction trace rises above 0 dB during the burst and the hold charge rises, as the engine reports.
3. **Given** a render in drop mode with pauses, **When** the Scopes tab shows the waveforms, **Then** the render's strip lines up with the original's content through the engine's map, and each removed pause is marked.
4. **Given** any zoom, **When** the waveform is drawn, **Then** each pixel column's envelope contains every sample of its span (min ≤ sample ≤ max).

### User Story 2 - Compare spectra and see the difference (Priority: P2)

The user selects a range and compares the spectra of the original and the render, with their difference, to see the band-limiting, the noise floor and the codec's character.

**Independent Test**: The spectrum of a full-scale sine at a bin centre reads 0 dB at its frequency. White noise reads its expected density. The difference of a take with itself is 0 dB. The values are exported to CSV and read back.

**Acceptance Scenarios**:

1. **Given** a full-scale 1 kHz sine, **When** its spectrum is computed, **Then** it reads 0 ± 0.05 dB at 1 kHz, at every supported rate.
2. **Given** a render of white noise, **When** the spectra are shown, **Then** the difference curve shows the engine's band (the low and high roll-offs) and the ranges are mapped through the engine's map.
3. **Given** a take compared with itself, **When** the difference is computed, **Then** it is 0 dB at every bin where the spectrum is above −200 dB.
4. **Given** a spectrum, **When** it is exported, **Then** the CSV holds every bin (frequency, original dB, render dB, difference dB) under a header naming the method, window, FFT size, overlap, rate, range and engine commit.

### User Story 3 - Every tap and every stage (Priority: P3)

The user renders the input at every tap to see where each change happens, and reads each stage's own description, status and evidence.

**Independent Test**: The tap browser's readings at each tap equal a direct render at that tap. The Stages view's text equals the engine's stage manifest.

**Acceptance Scenarios**:

1. **Given** an input, **When** "Render at every tap" runs, **Then** every tap of the engine's tap control has a row: either its readings and spectrum, equal to a session render at that tap, or the engine's refusal for the route.
2. **Given** the Stages view, **When** it opens, **Then** the header and all fifteen stage panels show the engine's own text, and each panel lists its controls.
3. **Given** a tap browser run, **When** the tap table is exported, **Then** the CSV holds one row per tap with the readings and the engine commit.

### Edge Cases

- **An empty render** (every sample paused): the views show "no samples". The meter timeline still has its rest point and one row per call.
- **A very long file:** the envelope is computed in tiers for display, and the spectrogram is computed in the background with progress. Memory for the analysis stays below one f32 copy of the input.
- **A range shorter than one FFT frame:** the spectrum is refused with "select at least N ms".
- **Silence:** the spectrum reads at the analysis floor and the difference is undefined; bins below −200 dB on either side are not drawn.
- **A tap the route refuses** in the tap browser: the engine's reason is shown, and the other taps still run.
- **A cancel during the tap browser** stops after the current tap, and the finished rows stay.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A render MUST record a meter timeline: the engine's rest read-out before the first call, then the engine's read-out after every call, each with the input and output positions at the end of its call (D1).
- **FR-002**: The render block MUST be rate ÷ 100 host samples (D1), and the render MUST stay equal to 001's oracles.
- **FR-003**: The Meters view MUST plot every meter field over time with its unit and meaning (D2). A test MUST fail if the engine's meter read-out has a field the view does not name.
- **FR-004**: The Scopes view MUST draw both takes' envelopes on one time axis, the render aligned through the engine's map or raw, with removed pauses marked and the playhead shown (D3). Each pixel column's envelope MUST bound its samples.
- **FR-005**: The spectrum MUST follow D4's method. A full-scale sine at a bin centre MUST read 0 ± 0.05 dB, and the difference of a take with itself MUST read 0 dB.
- **FR-006**: The spectrogram MUST follow D5's method, in the background for long inputs, with progress.
- **FR-007**: The tap browser MUST render every tap of the engine's tap control on a fresh engine with the session's values. It keeps each tap's readings and spectrum, reports the route's refusals, and can be cancelled between taps (D6).
- **FR-008**: The Stages view MUST show the stage manifest's header and every stage verbatim, with each stage's controls (D7).
- **FR-009**: The spectra, the meter timeline and the tap table MUST export as CSV with a method header (D10).
- **FR-010**: All analysis MUST live in the core with no UI dependency, be deterministic, and never touch the audio the engine receives or the workbench exports (D9; constitution I, V).
- **FR-011**: Every new string MUST pass the wording check, and every figure MUST state its method (constitution VI).
- **FR-012**: The feature MUST bring into use `Pipeline::meters` and `Pipeline::rest_meters` and the stage manifest, and the reach test MUST print the new ratio (constitution II).

### Key Entities

- **Meter row**: an engine meter read-out, with the call's end positions in the input and output clocks.
- **Spectrum**: the bins (frequency, dB) of one take over one range, with the method's parameters.
- **Tap result**: a tap's identifier, its readings or the engine's refusal, and its spectrum.

## Success Criteria *(mandatory)*

- **SC-001**: The meter timeline equals the engine's read-outs call for call, at seven rates.
- **SC-002**: The spectrum's calibration reads 0 ± 0.05 dB on a full-scale sine at every rate, and white noise reads within 0.5 dB of its expected density.
- **SC-003**: The tap browser's readings equal direct renders at every tap, at seven rates.
- **SC-004**: A one-minute 48 kHz render with the meter timeline still takes under 10 s. The tap browser on one minute takes under 60 s, and a spectrum of one minute under 1 s, in a release build.
- **SC-005**: The Stages view's text equals the engine's manifest for every stage, as a test proves.
- **SC-006**: The owner's desk checklist: render, inspect meters, waveforms, spectra, spectrogram, run the tap browser, read the stages, export each CSV.

## Assumptions

- The engine is still pinned at `a06318c` (0.92.0), whose meter read-out (spec 016, extended by 018) has the thirteen fields of D2.
- Alternatives considered:
  - **D1**: 4096 blocks (85 ms at 48 kHz: too coarse for a limiter's attack); a block of 1 (too slow).
  - **D3**: overlaid traces (unreadable when the takes differ in level).
  - **D4**: a fixed FFT size at every rate (resolution would vary 16×); a time-domain difference (meaningless across drop-mode splices and the coder's delay, so it is deferred to 005).
  - **D6**: keeping each tap's samples (up to twelve copies of the input).
  - **D7**: a guessed tap-to-stage mapping (not published by the engine).
