# Feature Specification: Taps, scopes and meters

**Feature Branch**: `002-taps-scopes-meters`

**Created**: 2026-11-02

**Status**: The draft reviews are applied (coach 1 – 17, hardware B1 – B13). Under the owner's autopilot rule (2026-11-02), the decisions take effect at their recommendations. `/speckit-clarify` found nothing left for the owner (see Clarifications).

**Input**: Feature 002 of docs/workbench-plan.md: "Waveform and spectrum views of the input, the tap and the difference. The per-call meters plotted over time. The stage manifest as a panel per stage, with its sources and assumptions." It builds on feature 001's session.

## Clarifications

### Session 2026-11-02 (autopilot)

- The reviews' questions were answered at their recommendations:
  - **5 Hz spectrum bins** (N = rate ÷ 5).
  - **Meter meanings shown as a labelled workbench note** from the engine's rustdoc, with an engine issue for a published table (rr-dr60-emu#75).
  - **No meter block finer than 10 ms.** That is the shortest block that is a whole number of samples at all seven rates.
- No other ambiguity remains that would change the plan.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **The meter timeline.** A render reads the engine's meters after every processing call, plus the engine's **rest read-out** (read after the values are applied, never `Meters::default()`, whose battery ceiling is 0.0) before the first call. The render's block becomes **rate ÷ 100 host samples, 10 ms**: 60 / 80 / 160 / 441 / 480 / 882 / 960. That is the shortest block that is whole at all seven rates, and it carries exactly 60 device samples per call. The output does not depend on the block, as 001's oracle (the whole input in one call, and block size 1) shows. A sidecar's block is informational: a restore renders at rate ÷ 100 and the output digest still matches. A row stores its call's start and end positions in the input and output clocks. A cancelled render keeps no timeline. | **10 ms calls; the rest point first** |
| D2 | **The meter fields.** Every field of the engine's read-out is shown with its engine identifier, its unit, and its kind: a **maximum** or **count** over the call's processed samples, or a **state** at the call's end. A meaning is shown too, labelled "workbench note, from the engine's rustdoc at a06318c", until the engine publishes one (rr-dr60-emu#75). The units:<br>• peaks are **sample peaks**, linear and in dBFS;<br>• the route peak is relative to the route's scale;<br>• gain reduction is in dB;<br>• the hold charge is a fraction of the model's 40 dB hold, with its landmarks 0.13 / 0.73 / 0.84 / 0.90;<br>• the hang is in device samples at 6 kHz, with its ms beside it;<br>• counts are given per 10 ms call and as a fraction of the call's samples in their own domain. | **All thirteen, with identifier, unit, kind** |
| D3 | **Placing the figures in time.** A maximum or count is drawn as a step across its call's interval, and a state as a point at its call's end, each on its own clock:<br>• **input side** (input peak, limiter-input peak, gain reduction, hold charge) on the call's input span;<br>• **device point** (record-filter peak, ADC-clamp count, gate paused, hang) on the input span, with the note "up to the record path's latency late";<br>• **output side** (reconstruction peak, route peak, output-clamp, amplifier-clip, battery ceiling) on the call's output span, mapped to the input clock through the engine's one-pass map.<br>Tail calls are marked. A 10 ms maximum cannot show the limiter's 0.5 – 3 ms dead time; the waveform at the after-AGC tap can, and the method text says so. | **Steps and points, each on its clock** |
| D4 | **Figures a tap does not reach.** A field whose point the configuration does not run (the route peak away from the route-output tap, the reconstruction peak before stage 10, the gate fields when the gate does not run, the amplifier clip off the speaker route or with stage 11 bypassed, the limiter input at the capsule tap) is drawn greyed as "not reached (rest)". The rule is the engine's, quoted with its source lines like 001's `gate_reached`, pinned by behavior, and filed as rr-dr60-emu#75. | **Greyed "not reached"** |
| D5 | **Waveforms.** The original and the render are two stacked strips on the original's clock: a min/max envelope per pixel column, with zoom, scroll and the playhead. The render is placed through the engine's one-pass map, which aligns within about one device sample (a few with the coder on); "Raw" places it on its own clock. Removed pauses (drop mode) and muted regions (mute mode) are marked. Render samples that map before input 0 are drawn at 0 with a marker, and tail samples past the original's end are labelled "tail". | **Stacked, aligned by the engine's map** |
| D6 | **Spectra and their ratio.**<br>• **Method**: FFT size **N = rate ÷ 5** (1200 / 1600 / 3200 / 8820 / 9600 / 17 640 / 19 200), so the bins are 5 Hz at every rate and 1 kHz is bin 200. Hann window, 50 % overlap, power averaged (Welch).<br>• **Scale**: dB re full-scale sine **per 5 Hz bin (ENBW 7.5 Hz)**. A sine of amplitude A at a bin centre reads 20·log10 A. White noise of variance σ² reads 10·log10(4σ²·ENBW/N), where ENBW = 1.5 bins. A "dB/Hz" toggle subtracts 10·log10(7.5).<br>• **Tone levels**: Hann's scalloping loss reaches 1.42 dB between bins, so tone levels are not read off this view, and the method text says so.<br>• **Ranges**: the original's frames come from **kept spans only**, and render frames that contain a splice are dropped. Both frame counts are shown.<br>• **The ratio**: the curve is the **spectral ratio, render ÷ original (not a transfer function)**. Its offset includes the route's scale and the volume, and below the band it shows the capsule's hiss. A ratio bin is drawn only where both sides are within 120 dB of their own largest bin.<br>• **Minimum range**: 200 ms.<br>A time-domain null test is feature 005. | **5 Hz Welch bins; the stated ratio** |
| D7 | **Spectrogram.** **40 ms frames** (N = rate ÷ 25) with a 10 ms hop, Hann, and a 10 ms "wideband" toggle. The colours run −120 … 0 dB re full-scale sine per bin and state the ENBW; values above 0 dB saturate to the top colour, and the legend says so. It is stored as u8 colour indices, reduced to at most 4096 time columns, recomputed for the zoomed range, and computed in the background with progress. | **40 ms frames, u8 columns** |
| D8 | **The tap browser.** "Render at every tap" renders the input at each tap of the engine's tap control, on a fresh engine per tap (recording counter 0, so every tap carries the same hiss) with the session's values. The tap the route refuses is skipped, with the engine's reason. Each tap keeps its readings, its **output digest** and its spectrum, taken over the take without its tail and mapped with that tap's own latency and reports, but not its samples. The table goes stale under 001's D5 rule. | **Every tap; readings, digest, spectrum** |
| D9 | **Stage panels.** A Stages view shows the stage manifest's header (library version, device rate, counts by status, basis sentence, links) and one panel per stage of the manifest (15 at the pin), verbatim: number, identifier, name, status, path, confidence, description, the coverage note ("not modeled here"), the controls note, model version, sources and assumptions. Each panel also lists that stage's controls from the control manifest, and the controls with stage 0 are listed under "no stage". No tap is attached to a stage: the engine publishes no mapping. | **Verbatim, with each stage's controls** |
| D10 | **Layout.** Tabs in the central area: Session (001), Scopes (waveforms, spectra, spectrogram), Meters, Taps and Stages. | **Tabs** |
| D11 | **Analysis lives in the core**, never on the audio path. It is deterministic on one host; exported figures agree across hosts within 1e-6 dB. The plan picks the FFT library. | **Core analysis, stated methods** |
| D12 | **CSV export, reproducible.** The spectra and their ratio, the meter timeline and the tap table export as CSV. The header carries the method's parameters, and the render's full sidecar is written beside the CSV, so the figure can be reproduced (constitution III). | **CSV + the render's sidecar** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - See what the engine did, over time (Priority: P1)

After a render, the user sees the two waveforms aligned, and the engine's meters over time: where the limiter reduced gain, where the gate paused, where the output clipped.

**Independent Test**: The meter timeline equals the engine's read-outs, call for call. The envelopes bound the samples. The reach rule matches what the engine does.

**Acceptance Scenarios**:

1. **Given** a render, **When** the Meters tab opens, **Then** every engine meter field is shown with its identifier, unit and kind. The first point is the rest read-out, and each later row equals the engine's read-out after that call (`meters_timeline_equals_engine`).
2. **Given** a 1 kHz burst starting on its peak at +9 dBFS into the default engine, **When** it renders, **Then** the gain reduction exceeds 5 dB during the burst and the hold charge rises (`meters_limiter_burst`).
3. **Given** a render at a tap that does not reach a field, **When** the Meters tab shows it, **Then** that field is greyed "not reached", and the reach rule agrees with the engine's behavior at every tap (`meter_reach_pinned`).
4. **Given** a drop-mode render with pauses, **When** the Scopes tab draws the waveforms, **Then** the render lines up through the engine's map and every removed pause is marked (`waveform_alignment_marks`).
5. **Given** any zoom, **When** an envelope is drawn, **Then** each column's min/max contains every sample of its span (`envelope_bounds`).
6. **Given** the engine's meter read-out, **When** the field list is compared, **Then** a field the view does not name fails the test (`meter_fields_named`).

### User Story 2 - Compare spectra (Priority: P2)

The user selects a range and compares the spectra of the original and the render, with their ratio, to see the band-limiting, the noise floor and the coder's character.

**Independent Test**: The calibration on sines and noise, an identity render through a nonzero alignment, the ratio's arithmetic and range mapping, and the CSV round trip.

**Acceptance Scenarios**:

1. **Given** a full-scale 1 kHz sine, **When** its spectrum is computed, **Then** bin 200 reads 0 ± 0.05 dB at every supported rate (`spectrum_calibration_bin_200`).
2. **Given** 30 s of seeded Gaussian noise of variance σ², **When** its spectrum is computed, **Then** the mean power over the bins from 10 % to 90 % of Nyquist reads 10·log10(4σ²·1.5/N) within 0.05 dB (`spectrum_noise_density`).
3. **Given** an identity configuration with nonzero latency, **When** its ratio is computed, **Then** it reads 0 ± 0.1 dB across the passband (`spectrum_identity_ratio`). The identity is found in the plan, for example the stages bypassed to the record filter tap.
4. **Given** any render, **When** the ratio is drawn, **Then** it equals render dB − original dB, bin for bin, with the render's range equal to the map of the selected range, kept spans only and splice frames dropped (`spectrum_ratio_arithmetic`).
5. **Given** a spectrum, **When** it is exported, **Then** the CSV holds every bin under a header naming the method, and the render's sidecar is written beside it (`spectrum_csv_roundtrip`).

### User Story 3 - Every tap and every stage (Priority: P3)

The user renders the input at every tap to see where each change happens, and reads each stage's own description, status and evidence.

**Acceptance Scenarios**:

1. **Given** an input, **When** "Render at every tap" runs, **Then** every tap of the tap control has a row: its readings and an output digest equal to a session render at that tap, or the engine's refusal for the route (`tap_browser_digest_equals_render`).
2. **Given** the Stages view, **When** it opens, **Then** the header and every stage of the manifest show the engine's own text, and each panel lists its controls (`stages_verbatim`).
3. **Given** a tap browser run, **When** the table is exported, **Then** the CSV holds one row per tap with the sidecar beside it (`tap_csv_roundtrip`).

### Edge Cases

- **An empty render:** the views say "no samples", and the timeline still has its rest point and its rows.
- **A long file:** the envelope is computed in tiers, and the spectrogram has u8 columns, at most 4096 of them. The analysis memory stays below one f32 copy of the input.
- **A range under 200 ms** is refused, with the minimum named.
- **Silence:** ratio bins outside the 120 dB window are not drawn.
- **A tap the route refuses:** the engine's reason is shown, and the other taps run.
- **A cancel** during the tap browser stops after the current tap, and the finished rows stay.
- **A sidecar from 001** (block 4096) restores, renders at rate ÷ 100 and reproduces the digest (`block_independent_restore`).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: A render MUST record the meter timeline of D1, with the rest point first and one row per call, each with its call's input and output spans (`meters_timeline_equals_engine`).
- **FR-002**: The render block MUST be rate ÷ 100, the render MUST stay equal to 001's oracles, and a 001 sidecar MUST reproduce its digest (`render_oracle`, `block_independent_restore`).
- **FR-003**: The Meters view MUST show the fields per D2 and D3, and MUST fail a test when the engine has a field the view does not name (`meter_fields_named`).
- **FR-004**: An unreached field MUST be greyed per D4, by a rule pinned by behavior (`meter_reach_pinned`).
- **FR-005**: The waveforms MUST follow D5, and their envelopes MUST bound their samples (`envelope_bounds`, `waveform_alignment_marks`).
- **FR-006**: The spectra MUST follow D6 and pass AS1 – AS4 of US2.
- **FR-007**: The spectrogram MUST follow D7, computed in the background with progress (`spectrogram_columns`).
- **FR-008**: The tap browser MUST follow D8 and be cancellable between taps (`tap_browser_digest_equals_render`, `tap_browser_cancel`).
- **FR-009**: The Stages view MUST follow D9 (`stages_verbatim`).
- **FR-010**: The CSV exports MUST follow D12 (`spectrum_csv_roundtrip`, `meters_csv_roundtrip`, `tap_csv_roundtrip`).
- **FR-011**: All analysis MUST be in the core with no UI dependency, MUST be deterministic, and MUST never touch the engine's input or the export (D11; constitution I, V).
- **FR-012**: Every new string MUST pass the wording check, and every figure MUST state its method (constitution VI; `wording`).
- **FR-013**: The feature MUST call `Pipeline::meters` and `Pipeline::rest_meters` and read the stage manifest, and the reach test MUST print 14/70 (constitution II; `reach`).

### Key Entities

- **Meter row**: an engine read-out with its call's input and output spans, and whether it is a tail call.
- **Spectrum**: the bins of one take over one range, with N, the window, the overlap, the frame count and the scale.
- **Tap result**: a tap's identifier, its readings, output digest and spectrum, or the engine's refusal.

## Success Criteria *(mandatory)*

- **SC-001**: The meter timeline equals the engine's read-outs call for call at seven rates.
- **SC-002**: The spectrum reads 0 ± 0.05 dB at bin 200 on a full-scale 1 kHz sine at every rate, and the noise density within 0.05 dB.
- **SC-003**: The tap browser's output digests equal direct renders at every tap at seven rates.
- **SC-004**: In a release build:
  - a one-minute 48 kHz render with the timeline takes under 10 s;
  - the tap browser on one minute, under 60 s;
  - a spectrum of one minute, under 1 s;
  - a spectrogram of one minute, under 5 s.
- **SC-005**: The Stages view equals the engine's manifest text for every stage.
- **SC-006**: The owner's desk checklist covers rendering, the meters, the waveforms, the spectra, the spectrogram, the tap browser, the stages and each CSV.

## Assumptions

- The engine is still pinned at `a06318c` (0.92.0). Its meter read-out (spec 016, extended by 018) has thirteen fields.
- Rejected alternatives:
  - **D1**: 4096 blocks (85 ms at 48 kHz).
  - **D5**: overlaid traces.
  - **D6**: power-of-two sizes. 1 kHz would fall between bins (−0.63 dB) and some rates would need fractional sizes.
  - A time-domain difference (deferred to 005).
  - **D7**: 20 ms frames (voice harmonics unresolved).
  - **D8**: keeping each tap's samples.
  - **D9**: a guessed tap-to-stage map.
