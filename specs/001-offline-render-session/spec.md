# Feature Specification: The offline render session

**Feature Branch**: `001-offline-render-session`

**Created**: 2026-11-02

**Status**: Draft. The draft reviews have been applied (coach 1 – 17, hardware B1 – B10), and `/speckit-clarify` answered four questions (2026-11-02). Signed off by the owner 2026-11-02.

**Input**: User description: "Feature 001 of docs/workbench-plan.md: the offline render session. Open a mono or multichannel audio file at one of the engine's supported host rates; edit every setting through a panel generated at run time from the engine's control manifest; render it through the default one-pass engine in the background with progress and cancel; play the original and the render with an instant A/B switch at the same position; export the render sample for sample with a sidecar (engine version and pinned commit, host rate, seed, every control value by identifier, the input file's name and digest, the tap) and reopen a sidecar to restore the settings."

## Clarifications

### Session 2026-11-02

- Q: When the user switches between the original and the render, where does playback continue? → A: At the same moment of content through the engine's one-pass map (latency plus the removed pauses); "same seconds" stays an option (D6, FR-010, SC-005 as written).
- Q: Does a render feed silence after the input so the file's end comes out? → A: Yes, on by default: latency + drain samples of digital silence, labelled, recorded in the sidecar, and the user can turn it off (D7).
- Q: Are the tap and the component slot editable in 001? → A: Both, like every manifest control; the tap's incompatibility with the route is refused beside both controls (D4, US2 AS4, AS6); the plan of record amended.
- Q: What makes a reopened sidecar's input "the same"? → A: The digest of the converted engine-input samples; the raw-file digest is shown for information only (D9, FR-014).

### Session 2026-11-02 (plan reviews: dated restatements)

- FR-002, the float mix: "rounds once" holds for integer files (exact `i64` sum; one correct rounding, research R-02). Float files are summed in double precision in channel order, divided, and rounded to single precision, which is deterministic but not a single rounding.
- FR-009, wording: the engine's wording denylist contains the stem "event", so the screen says **"splices"** (drop mode) or **"muted regions"** (mute mode). "Gate events" stays the requirement's term only.
- FR-009, "samples removed" is restated as **"not emitted"**: input + tail − output. It counts removed pauses, dropped tail silence and samples held at a final pause, and it is 0 in mute mode, with the gate bypassed and at the taps before the gate.
- FR-009, "gate not reached": the engine's own rule decides it (the gate runs when it is enabled and the tap is after the gate). This includes the record-filter tap and a switched-off gate, not only the taps before the gate (research R-07).
- Engine reach: `process` and `reconfigure` are added, because the check engine uses both (research R-08), and the denominator excludes the engine's hidden test hooks (research R-14).
- D7, the tail: it is 0 at the taps before the gate, where the engine has no latency and nothing after the gate runs (plan P12).

## Decisions for sign-off

Read this section first. Each row gives the recommendation, and the Assumptions section lists the alternatives. Rows marked † were changed or added by the draft reviews.

| # | Decision | Recommended |
|---|---|---|
| D1 † | **Input formats.** The session opens WAV, AIFF and FLAC files with 16-, 24- or 32-bit integer samples or 32-bit float samples. It refuses lossy formats (MP3, AAC), because a lossy decoder adds its own artifacts and those would be read as the engine's. It also refuses 8-bit integers, 20-bit FLAC and 64-bit float, with a message. | **WAV, AIFF, FLAC; lossy and the odd widths refused** |
| D2 | **Channels.** The engine takes one channel. The session uses **channel 1** by default, and a picker offers every other channel and **"mix"**, the mean of all channels. Mix is labelled as a conversion at the boundary, with a note that anti-phase content cancels, and the sidecar records the choice. "Original" in A/B means the selected channel after conversion. | **Channel 1 by default; any channel or the mean** |
| D3 | **Sample rates.** A file at one of the engine's seven supported host rates (6, 8, 16, 44.1, 48, 88.2, 96 kHz) renders at that rate. Any other rate is **refused** with the supported list. The workbench does not resample (constitution I). | **Refuse unsupported rates** |
| D4 † | **Which controls the panel offers.** Every control of the engine's control manifest appears, grouped by stage, in manifest order, **the tap included**. A control whose paths exclude the one-pass (the memory controls, the playback speed and its mode) is shown disabled with the reason "not on the one-pass". The host rate is read-only: it is the file's rate. The seed is editable. The component slot's controls are offered and written through the engine's component path. Every value applies before the render begins. A control's timing is shown for information, and scheduling during a render is feature 003. | **Every control, the tap and the slot included; the unavailable ones disabled with a reason** |
| D5 † | **When a render runs.** A render starts on the **Render** button. It is marked stale whenever the control values, the seed, the channel or the input differ from the render's own. It runs in the background with progress and can be cancelled, and a cancelled render leaves the previous one in place. | **Explicit render; stale marker** |
| D6 † | **A/B playback.** One transport plays the original or the render, and one key switches between them instantly. **The switch maps the position through the engine's one-pass map**: the engine's latency, plus the input that drop mode removed before that point. The same moment of content is therefore heard in both takes. Raw "same seconds" is an option. The engine publishes the map, so the workbench does no alignment analysis of its own. Nothing is level matched. The monitor volume acts on the output device only. The output device's own rate conversion is the only conversion at the boundary, and the session states it when the rates differ. | **The engine's map by default; same seconds as an option** |
| D7 † | **The tail.** The one-pass emits each input sample only after the engine's latency, and its output chain then rings on. To bring every input sample out, the session appends **T = latency + drain** samples of digital silence after the input. The tail is on by default, labelled "tail: the engine's latency and drain", recorded in the sidecar, and can be turned off. | **Tail on by default** |
| D8 † | **The export.** A 32-bit float mono WAV at the file's rate holds the engine's output samples unaltered. A **sidecar** with the same name is written beside it: a versioned, human-readable text file holding the workbench version; the engine version and full commit; the host rate; the seed; the recording counter (0, a fresh engine); the tail length; the block size; the channel choice; the conversion rule's version; every control by identifier, with its value as the engine reads it back; the component slot and its parameters; the input file's name, length, a digest of the raw file and a digest of the converted engine input; and the render's length, latency, gate events, samples removed, peak and a digest of the output. | **WAV f32 + a versioned text sidecar** |
| D9 † | **Reopening a sidecar.** Opening a sidecar restores every value it holds. With the same converted-input digest, channel and engine commit, the next render reproduces the export bit for bit. The session lists each difference: a different engine commit (the render may then differ), an unknown identifier (skipped), a control the engine has that the sidecar lacks (left at its default), a value the engine refuses (that control left unchanged), and a different converted-input digest (the raw-file digest is shown for information only). | **Restore, with every difference listed** |
| D10 | **Sessions.** One session per window, with one file and one current render. Several renders, presets and history are feature 005. | **One file, one render** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Render a file and hear the difference (Priority: P1)

A developer or enthusiast opens a voice recording, renders it through the default engine, and switches between the original and the render at the same moment of content, to hear what the emulated recorder does to it.

**Why this priority**: It is the smallest loop that makes the workbench useful, and every later feature builds on a session with an input and a render.

**Independent Test**: Open generated test files at each supported rate and render them with the defaults. Each render equals the engine's direct output for the same calls, sample for sample. Then play and switch on the Mac.

**Acceptance Scenarios**:

1. **Given** a mono WAV at 48 kHz that ends on a tone with no trailing silence, **When** the user opens it and presses Render, **Then** the render shows its length, the engine's latency, the tail, the splices, the samples not emitted, whether the gate is paused at the end, and its peak. It equals the oracle of FR-007 sample for sample, and the tone's end is in it.
2. **Given** a rendered session that is playing the original, **When** the user switches to the render, **Then** playback continues at the render position that the engine's one-pass map gives for that input position, clamped to the render's end. With "same seconds" chosen, it continues at the same time instead.
3. **Given** a stereo file, **When** it is opened, **Then** channel 1 is used, and the picker offers channel 2 and "mix", with its label.
4. **Given** a file at 22.05 kHz, an MP3 or an 8-bit WAV, **When** the user opens it, **Then** it is refused with a message naming what is supported, and the session is unchanged.
5. **Given** a long file rendering, **When** the user cancels, **Then** the render stops within a second and the previous render, if any, stays.

---

### User Story 2 - Change settings from the engine's own panel (Priority: P2)

The user changes the engine's values from a panel the engine itself describes, renders again, and hears the change.

**Why this priority**: Exploring and tuning means changing the model's values. The panel must stay correct as the engine grows, without anyone copying ranges into the workbench.

**Independent Test**: The panel model presents every control of the manifest with its type, range, default, options and unit. Each control's value reaches the engine through that control's own path. A control the panel model lacks makes a test fail.

**Acceptance Scenarios**:

1. **Given** a fresh session, **When** the panel opens, **Then** every control of the manifest is listed in its stage group with its default, unit, kind, timing and description, and its sources and assumptions are available on request.
2. **Given** the panel, **When** the user sets the voice-activation sensitivity to 5 and renders, **Then** the render equals the oracle with that one setting changed.
3. **Given** the panel, **When** the user enters a value the engine refuses, **Then** the engine's refusal is shown beside that control in the engine's terms, and Render is unavailable until the value is fixed or reverted.
4. **Given** the tap set to the earphone tap while the route is the speaker, **When** the panel checks the values, **Then** the engine's "tap not on the route" refusal is shown beside both controls, and Render is unavailable.
5. **Given** the memory controls and the playback speed, **When** the panel is shown, **Then** they are disabled with the reason "not on the one-pass".
6. **Given** the band filter inserted in the component slot with a changed parameter, **When** the user renders, **Then** the render equals the oracle with the same slot and parameter.
7. **Given** values changed since the last render, **When** the user looks at the session, **Then** it shows the stale marker, and "reset to defaults" restores every control to the manifest's default.

---

### User Story 3 - Export a render and reproduce it later (Priority: P3)

The user exports a render with its sidecar, to share it or quote it in an issue. Later, the user or someone else opens the sidecar with the same input and gets the same render.

**Why this priority**: A finding is worth something only if someone can reproduce it (constitution III).

**Independent Test**: Export, then read the WAV back: it equals the render bit for bit. In a fresh session, open the same input and the sidecar and render: the output is identical. Doctored sidecars report each difference.

**Acceptance Scenarios**:

1. **Given** a render, **When** the user exports it, **Then** a 32-bit float WAV at the input's rate holds exactly the render's samples, and the sidecar beside it holds every field of D8.
2. **Given** an export and its sidecar, **When** a fresh session opens the same input and the sidecar and renders, **Then** the render equals the exported samples bit for bit, and its output digest equals the sidecar's.
3. **Given** a sidecar with a different engine commit, one unknown identifier and one missing control, **When** it is opened, **Then** the session lists the commit difference with "the render may differ", the skipped identifier and the control left at its default, and applies the rest.
4. **Given** a sidecar whose converted-input digest differs from the loaded file, **When** it is opened, **Then** the values are restored and the session warns that the input differs.
5. **Given** a file that is not a sidecar, or a sidecar of an unknown version, **When** it is opened, **Then** it is refused with a message and the session is unchanged.

### Edge Cases

- **An empty file** (zero samples) is accepted. The render is the tail's output, or empty with the tail off. The export writes that render and a sidecar.
- **Float samples beyond ±1.0** pass to the engine unchanged, and its own stages decide what happens. **NaN, ±infinity and subnormal samples** are zeroed by the engine. The session counts them, shows the count before the render, and passes them unchanged.
- **A render or an original above full scale.** The default speaker route reads about 2.0 on full-scale material, and the output clamp is off by default. The session shows the sample peak and warns that the output device clips above 1.0. It never clamps, and the export keeps the samples.
- **Drop mode pausing everything.** The render may be only the tail's output. The session says so and shows "paused at end".
- **A tap where the gate does not run** (the taps before the gate, the record-filter tap, or the gate switched off). The session shows "gate not reached" instead of a count of zero.
- **A very long file** (60 minutes at 96 kHz, 1.38 GB per 32-bit copy). It opens and renders with progress. Memory stays within three copies (the selected input, the previous render and the new render) plus the engine's footprint. Other channels are decoded on demand, so a channel change decodes the file again. An export that would exceed the WAV format's 4 GB limit is refused.
- **The output device changes or disappears** during playback. Playback stops with a message, and the session survives.
- **The export target already exists.** The user confirms the overwrite. The WAV is never left without its sidecar, or the reverse.
- **A file with metadata chunks or many channels.** Metadata is ignored, and every channel is offered.
- **Values changed during a render.** The running render keeps the values it started with, and the stale marker appears when it finishes.
- **A continuous value of −0.0.** It is written and read back as the engine stores it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The session MUST open the formats of D1 and refuse every other format and every rate outside the engine's supported host rates, with a message that names what is supported (D1, D3).
- **FR-002**: The session MUST convert the file's samples to the engine's input by x ÷ 2^(b−1), where b is the container width. 16-bit and 24-bit samples are exact, and the 24-bit conversion MUST equal the engine's published feed-point rule bit for bit (a test against the engine's function, not a copy of it). A 32-bit integer is rounded once to nearest even, which is labelled as a boundary conversion. Float samples pass unchanged. "Mix" sums integers exactly and floats in double precision, in channel order, divides by the channel count, and rounds once. A test MUST pin each rule.
- **FR-003**: The session MUST offer every channel of the file and "mix", with channel 1 as the default (D2).
- **FR-004**: The settings panel MUST be generated from the engine's control manifest at run time. It shows, for every control, its name, identifier, stage, kind, timing, value type, range or options, default, unit and description, with its sources and assumptions on request, and an editor for the value type. A test MUST fail if the engine has a control that the panel model does not present.
- **FR-005**: Controls whose paths exclude the one-pass MUST be disabled with the reason. The host rate MUST be read-only and equal to the file's rate (D4).
- **FR-006**: Each control MUST be written through the engine's own path for its class: a settings field, a plain live control (route, volume, battery voltage) or the component slot. The panel model MUST derive the class from the engine and never from a list in the workbench. Values MUST be checked by the engine itself, one control at a time (the defaults plus that value) and then as a whole, including the tap's compatibility with the route. Each refusal is shown beside its control in the engine's terms, and rendering MUST be unavailable while any refusal stands.
- **FR-007**: Each render MUST use a fresh engine built from the panel's settings, with recording counter 0. Before the first processing call it MUST apply, in this order: the component slot and its parameters, the route, the volume and the battery voltage. It then processes the selected channel followed by the tail (D7) and concatenates what each call produced. **The oracle** makes the same calls on a fresh engine with the whole input in one call and again at block size 1. The render MUST equal both, sample for sample, at every supported rate.
- **FR-008**: A render MUST run off the interface, show progress at least every half second, be cancellable within one second, and on cancel leave the previous render unchanged (D5).
- **FR-009**: The session MUST show these readings, each labelled with how it is obtained:
  - **Length**: input + tail in mute mode, with the gate bypassed, or at a tap before the gate; input + tail − removed in drop mode.
  - **Latency**: the engine's figure for the session's settings and tap. It covers the conversion, the voice-band stages at 1 kHz and the speech coder, and excludes the host-rate stages' small group delay.
  - **Tail** length.
  - **Gate events** (shown as "splices" or "muted regions"): summed from the engine's reports; "gate not reached" where the engine's rule says the gate does not run.
  - **Not emitted**: input + tail − output (removed pauses, dropped tail, samples held at a final pause).
  - **Paused at end**: the gate's state after the last call.
  - **Sample peak** of the render: a workbench analysis of the engine's output.
- **FR-010**: Playback MUST play the original or the render on the default output device. It MUST switch between them at the position given by the engine's one-pass map, or by the same time when that option is chosen, clamped to the target's end (D6). It MUST alter no sample apart from the device's own rate conversion and a monitor volume that is never exported, and it MUST state each of these on screen when it applies. It MUST warn when the playing take's peak exceeds 1.0.
- **FR-011**: The playback callback MUST allocate nothing, take no locks and do no I/O. Its buffers MUST exist before playback starts, and dropouts MUST be counted and shown (constitution IV).
- **FR-012**: Export MUST write a 32-bit float mono WAV at the file's rate holding exactly the render's samples, and the sidecar of D8 beside it. The pair counts as written only when both files are complete, and a failure MUST leave no half-written pair. An export over the format's size limit MUST be refused.
- **FR-013**: The sidecar MUST be a versioned, human-readable text format holding every field of D8. Each control is keyed by its manifest identifier, with its value as the engine reads it back after the write. Each continuous value is written as its shortest round-trip decimal and its exact bits, and the seed as an unsigned 64-bit integer. The read-back test MUST cover −0.0 and the ends of every range.
- **FR-014**: Opening a sidecar MUST restore every value it holds that the engine accepts and list each difference of D9. With the same converted-input digest, channel and engine commit, the next render MUST equal the exported samples bit for bit. A malformed or unknown-version sidecar MUST be refused with the session unchanged.
- **FR-015**: The workbench MUST pass no wall-clock time or OS randomness to the engine. The seed shown is the seed used.
- **FR-016**: Every workbench string MUST pass the engine's wording denylist (a test). Boundary conversions (mix, the 32-bit rounding, the device rate, the monitor volume) and the workbench's own figures MUST be labelled as such (constitution VI).
- **FR-017**: The feature MUST bring into use the engine items listed under "Engine reach". The constitution's reach test MUST land in this feature and print the ratio of public `Pipeline` methods called (constitution II).

### Engine reach

- `Pipeline`: `new`, `process`, `process_with_events`, `reconfigure`, `latency_samples`, `drain_samples`, `max_events`, `settings`, `control_value`, `set_control_value`, `set_component`, `set_component_parameter`, `component_parameter_value`, `recording_counter`, `footprint_bytes`.
- Every `Tap` value, through the tap control.
- Outside `Pipeline`: `controls()`, `Settings::validate`, the settings-by-index read and write, `setting_control`, `input::samples_from_i24` and `timeline::onepass_output_to_input`.

### Key Entities

- **Session**: one input file, the selected channel, the current control values and component slot, the tail choice, and the current render (or none).
- **Input**: the file's name, format, rate, channel count, length, raw-file digest, the selected channel's converted samples and their digest, and the count of non-finite and subnormal samples.
- **Control values**: one value per control in the manifest, keyed by identifier, with its class (settings field, plain live control, component), and its engine status (accepted, refused with the reason, not on the one-pass).
- **Render**: the output samples, the values, seed, tail and block size it was made with, the readings of FR-009, the output digest, and whether it is stale.
- **Sidecar**: the versioned text record of D8 that belongs to an export.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: At each of the seven supported rates, a render equals the oracle of FR-007 sample for sample. The generated test files are a tone that ends with no trailing silence, noise, and a voiced burst with pauses, each in drop and in mute mode.
- **SC-002**: An exported WAV read back equals the render bit for bit. A fresh session that opens the same input and the sidecar reproduces it bit for bit, with an equal output digest, at every supported rate.
- **SC-003**: 100 % of the engine's controls appear in the panel model, and a control added to the engine appears with no workbench change, as a test against the manifest proves.
- **SC-004**: A one-minute 48 kHz file renders in under 10 seconds on the development Mac with the default settings, and progress updates at least twice a second.
- **SC-005**: In the library, the A/B switch maps each original position to the render position that the engine's one-pass map gives (or to the same time), exactly, for every position in the tests. On the Mac, the switch is heard within one device buffer, checked on the desk checklist.
- **SC-006**: In 100 % of the edge-case tests, every refusal (format, rate, value, tap and route, sidecar, export size) gives a message that names what is wrong and leaves the session unchanged.
- **SC-007**: The owner completes the desk checklist on the Mac: open, render, A/B, change a setting, export, reopen the sidecar, reproduce.

## Assumptions

- The engine is the pinned rr-dr60-emu commit (0.92.0). Its control manifest, settings access by index (engine spec 025), component slot (spec 024), one-pass processing and published maps are used as they are, and the workbench changes none of them. Engine figures the plan quotes (the default latency, drain and tail per rate; the speaker peak on the stimuli) are read from a harness run first.
- A fresh engine per render is required for reproducibility. Resetting or reconfiguring an engine keeps the route, volume, battery, component slot and recording counter from earlier renders, and the hiss is keyed by the seed and the counter.
- Alternatives considered:
  - **D1**: WAV only (excludes common lossless files); lossy formats with a warning (decoder artifacts would be read as the engine's).
  - **D2**: always the mean (hides a channel's own content); refusing multichannel files (needlessly strict).
  - **D3**: resampling at the boundary (convenient, but signal processing of the workbench's own). It is deferred, labelled, if wanted.
  - **D4**: the tap disabled until feature 002 (fewer controls, but the tap is a manifest control and its reach costs nothing); the component slot deferred to feature 006 (the plan of record moves it to 001).
  - **D5**: rendering automatically after each change (surprising on long files). It could become an option later.
  - **D6**: raw same seconds by default (simpler, but the render lags by the latency and by every removed pause, so the takes drift apart in content); a short crossfade on the switch (it alters the sound); level matching (it alters the engine's levels).
  - **D7**: no tail (the end of every file would be lost); a fixed tail (wrong at some rates and settings).
  - **D8**: the engine's processing record (spec 020) as the sidecar. It describes recordings on the record path, not one-pass renders. Feature 004 writes the engine's record when it renders through the record path.
- The development Mac is Apple silicon with the default output device. Other platforms are not tested in 001.
- The digests are cryptographic-strength hashes. The plan picks the hash.
