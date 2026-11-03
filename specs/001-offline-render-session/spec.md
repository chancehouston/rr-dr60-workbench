# Feature Specification: The offline render session

**Feature Branch**: `001-offline-render-session`

**Created**: 2026-11-02

**Status**: Draft

**Input**: User description: "Feature 001 of docs/workbench-plan.md: the offline render session. Open a mono or multichannel audio file at one of the engine's supported host rates; edit every setting through a panel generated at run time from the engine's control manifest; render it through the default one-pass engine in the background with progress and cancel; play the original and the render with an instant A/B switch at the same position; export the render sample for sample with a sidecar (engine version and pinned commit, host rate, seed, every control value by identifier, the input file's name and digest, the tap) and reopen a sidecar to restore the settings."

## Decisions for sign-off

Read this section first. Each row gives the recommendation, and the Assumptions section lists the alternatives.

| # | Decision | Recommended |
|---|---|---|
| D1 | **Input formats.** WAV and AIFF (integer 16/24/32-bit and 32-bit float) and FLAC are accepted. Lossy formats (MP3, AAC) are refused in 001: a lossy decoder adds its own artifacts, and those would be read as the engine's. | **WAV, AIFF, FLAC; lossy refused** |
| D2 | **Channels.** The engine takes one channel. The session takes **channel 1** by default, and a picker selects any other channel or **"mix"**, the mean of all channels. Mix is labelled as a conversion at the boundary, and the sidecar names the choice. | **Channel 1 by default; any channel or the mean** |
| D3 | **Sample rates.** A file at one of the engine's seven supported host rates (6, 8, 16, 44.1, 48, 88.2, 96 kHz) is rendered at that rate. Any other rate is **refused** with a message that lists the supported rates. The workbench does not resample (constitution I). Resampling at the boundary is a later feature if it is wanted. | **Refuse unsupported rates** |
| D4 | **Which controls the panel offers.** Every control of the engine's control manifest appears, grouped by stage, in manifest order. A control whose paths exclude the one-pass (the memory controls, the playback speed and its mode) is shown **disabled**, with the reason "not on the one-pass". The host rate is shown read-only: it is the file's rate. The seed is editable. The live controls that the one-pass takes (route, volume, battery voltage) are applied before the render begins, and scheduling them during a render is feature 003. The component slot's controls are offered as the manifest describes them. | **Every control shown; unavailable ones disabled with a reason** |
| D5 | **When a render runs.** A render starts on the **Render** button and never on its own. While settings differ from the last render, the session shows "settings changed since the render". A render runs in the background, shows progress, and can be cancelled. A cancelled render leaves the previous render in place. | **Explicit render; stale marker** |
| D6 | **A/B playback.** One transport plays either the original or the render, with one key or button to switch instantly at the same time position in seconds. There is no crossfade and no level matching. The workbench changes no levels; a monitor volume acts on the output device only and never on an export. The output device's own rate conversion is allowed as a boundary conversion, and the session states it when the file's rate differs from the device's. | **Instant switch at the same time; no level matching** |
| D7 | **Different lengths.** In its default drop mode, voice activation removes pauses, so the render can be shorter than the original, and the one-pass adds latency. The A/B switch keeps the position in seconds and clamps it to the shorter take. The session shows both lengths, the engine's latency and the number of splices the engine reported. Aligning the two takes by content is not in 001. | **Same seconds, clamped; lengths and splices shown** |
| D8 | **The export.** The export is a WAV, 32-bit float, mono, at the file's rate, holding the engine's output samples unaltered. A **sidecar** is written beside it with the same name: a versioned, human-readable text file with the workbench version, the engine version and pinned commit, the host rate, the seed, the tap, the channel choice, every control by identifier with its value, the input file's name, length and a content digest, and the render's length, latency and splice count. | **WAV f32 + a versioned text sidecar** |
| D9 | **Reopening a sidecar.** Opening a sidecar restores every control value it holds into the panel. If its input file is the one loaded (by digest), the next render reproduces the export bit for bit. Each difference is reported and nothing is silently accepted: a different engine version, an identifier the engine does not know (skipped and listed), a control the engine has that the sidecar lacks (left at its default and listed), a value the engine refuses (listed, the control left unchanged), or a different input digest. | **Restore with every difference listed** |
| D10 | **Sessions in a window.** One session per window, with one file and one current render. Several renders side by side, presets and history are feature 005. | **One file, one render** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Render a file and hear the difference (Priority: P1)

A developer or enthusiast opens a voice recording, renders it through the default engine, and switches between the original and the render at the same moment to hear what the emulated recorder does to it.

**Why this priority**: This is the smallest loop that makes the workbench useful. Every later feature builds on a session with an input and a render.

**Independent Test**: Open a generated test file at each supported rate, render it with the defaults, and compare the render with the engine's output for the same samples, settings and seed run directly. They are equal sample for sample. Then play and switch takes on the Mac.

**Acceptance Scenarios**:

1. **Given** a mono WAV at 48 kHz, **When** the user opens it and presses Render, **Then** a render appears with its length, the engine's latency and the splice count, and it equals the engine's own output for those samples, the default settings and the default seed, sample for sample.
2. **Given** a rendered session that is playing the original at 3.2 s, **When** the user switches to the render, **Then** playback continues from 3.2 s of the render (or from the render's end, if it is shorter) with no other change.
3. **Given** a stereo file, **When** it is opened, **Then** channel 1 is used, and the picker offers channel 2 and "mix" with its label.
4. **Given** a file at 22.05 kHz or an MP3, **When** the user opens it, **Then** it is refused with a message naming the supported rates or formats, and the session is unchanged.
5. **Given** a long file rendering, **When** the user cancels, **Then** the render stops within a second and the previous render, if any, stays.

---

### User Story 2 - Change settings from the engine's own panel (Priority: P2)

The user changes the engine's settings from a panel the engine itself describes, renders again and hears the change.

**Why this priority**: Exploring and tuning means changing the model's values. The panel must stay correct as the engine grows, without anyone copying ranges into the workbench.

**Independent Test**: For every control in the manifest, the panel model offers the manifest's type, range, default, options and unit. A changed value reaches the engine's settings. A control that is not on the one-pass is disabled with the reason. A test fails if the engine has a control that the panel model lacks.

**Acceptance Scenarios**:

1. **Given** a fresh session, **When** the panel opens, **Then** every control of the manifest is listed in its stage group with its default, unit, kind, timing and description, and its sources and assumptions are available on request.
2. **Given** the panel, **When** the user sets the voice-activation sensitivity to 5 and renders, **Then** the render equals the engine's direct output with that one setting changed.
3. **Given** the panel, **When** the user enters a value the engine refuses, **Then** the engine's refusal is shown beside the control in the engine's terms, and Render is unavailable until it is fixed or reverted.
4. **Given** the memory controls and the playback speed, **When** the panel is shown, **Then** they are disabled with the reason "not on the one-pass".
5. **Given** changed settings since the last render, **When** the user looks at the session, **Then** it shows "settings changed since the render", and a "reset to defaults" restores every control to the manifest's default.

---

### User Story 3 - Export a render and reproduce it later (Priority: P3)

The user exports a render, with a sidecar, to share it or to quote it in an issue. Later, the user or someone else opens the sidecar with the same input and gets the same render.

**Why this priority**: A finding is worth something only if it can be reproduced (constitution III).

**Independent Test**: Export, then read the WAV back: it equals the render bit for bit. Open the sidecar in a fresh session with the same input, render, and the output is identical. Doctored sidecars report each difference.

**Acceptance Scenarios**:

1. **Given** a render, **When** the user exports it, **Then** a 32-bit float WAV at the input's rate holds exactly the render's samples, and a sidecar beside it holds every field of D8.
2. **Given** an export and its sidecar, **When** a fresh session opens the same input and the sidecar and renders, **Then** the render equals the exported samples bit for bit.
3. **Given** a sidecar from a different engine version, with one unknown identifier and one missing control, **When** it is opened, **Then** the session lists the version difference, the skipped identifier and the control left at its default, and applies the rest.
4. **Given** a sidecar whose input digest differs from the loaded file, **When** it is opened, **Then** the settings are restored and the session warns that the input differs.
5. **Given** a sidecar that is not a sidecar, or an unknown sidecar version, **When** it is opened, **Then** it is refused with a message and the session is unchanged.

### Edge Cases

- An empty file (zero samples): it is accepted, and the render is empty. Export writes an empty WAV and a sidecar.
- A file whose samples exceed full scale (float files above ±1.0): the samples pass to the engine unchanged, and the engine's own clamp and limiter decide what happens.
- Every sample paused by voice activation: the render may be empty. The session says so, and A/B playback of the render plays nothing.
- A very long file (60 minutes at 96 kHz): it opens and renders with progress. Memory use is shown, and it stays within two copies of the audio as 32-bit floats plus the engine's own footprint.
- The output device changes or disappears during playback: playback stops with a message, and the session survives.
- The export target already exists: the user confirms the overwrite. A sidecar is never left without its WAV, or the reverse.
- A file with metadata chunks, or with more than eight channels: the metadata is ignored, and every channel is offered in the picker.
- Settings changed during a render: the running render keeps the settings it started with, and the stale marker appears when it finishes.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The session MUST open WAV, AIFF and FLAC files with integer samples of 16, 24 or 32 bits or with 32-bit float samples (D1). It MUST refuse other formats, and files at a rate outside the engine's supported host rates, with a message that names what is supported (D3).
- **FR-002**: The session MUST convert the file's samples to the engine's input exactly: integer samples are scaled to ±1.0 by the published rule (the engine's 24-bit feed-point rule where it applies, and the plan states it for 16 and 32 bits), and float samples pass unchanged. A test MUST pin the conversion for each format.
- **FR-003**: The session MUST offer every channel of the file and "mix" (the mean of all channels, labelled as a boundary conversion), with channel 1 as the default (D2).
- **FR-004**: The settings panel MUST be generated from the engine's control manifest at run time. It shows, for every control, its name, identifier, stage, kind, timing, value type, range or options, default, unit and description, with its sources and assumptions on request, and an editor matching the value type. A test MUST fail if the engine has a control that the panel model does not present.
- **FR-005**: Controls whose paths exclude the one-pass MUST be disabled with the reason. The host rate MUST be read-only and equal to the file's rate (D4).
- **FR-006**: An edited value MUST be checked by the engine itself (its validation and representability rules). The engine's refusal MUST be shown beside the control, and rendering MUST be unavailable while any refusal stands.
- **FR-007**: A render MUST run the default one-pass engine over the whole selected channel with the panel's settings. Its output MUST equal the engine's direct output for the same samples, settings, seed and live-control values, sample for sample, at every supported rate.
- **FR-008**: A render MUST run off the interface, show progress at least every half second, be cancellable within one second, and on cancel leave the previous render unchanged (D5).
- **FR-009**: The session MUST show the render's length, the engine's latency for the session's rate and tap, and the number of splices the engine reported (D7). It MUST mark the render as stale whenever the settings differ from the ones it was made with.
- **FR-010**: Playback MUST play the original or the render on the default output device, and switch between them instantly at the same position in seconds, clamped to the shorter take's end (D6, D7). It MUST alter no sample, apart from the device's rate conversion and a monitor volume that is never exported, and both MUST be stated on screen when they apply.
- **FR-011**: Export MUST write a 32-bit float mono WAV at the file's rate holding exactly the render's samples, and the sidecar of D8 beside it. Each file MUST be complete before the pair counts as written, and a failure MUST leave no half-written pair.
- **FR-012**: The sidecar MUST be a versioned, human-readable text format holding every field of D8, with each control keyed by its manifest identifier and its value written so that it reads back exactly (bit-exact for continuous values).
- **FR-013**: Opening a sidecar MUST restore every value it holds that the engine accepts, and MUST list each difference of D9. With the same input file, the next render MUST equal the exported samples bit for bit. A malformed or unknown-version sidecar MUST be refused with the session unchanged.
- **FR-014**: The workbench MUST pass no wall-clock time or OS randomness to the engine. The seed shown is the seed used.
- **FR-015**: Every string MUST follow the claims policy (constitution VI). Conversions at the boundary (mix, device rate) and the workbench's own figures MUST be labelled as such.
- **FR-016**: The feature MUST state the `Pipeline` methods and `Tap` values it brings into use (constitution II).

### Key Entities

- **Session**: one input file, the selected channel, the current control values, and the current render (or none).
- **Input**: the file's name, format, rate, channel count, length and content digest, and the selected channel's samples as the engine's input.
- **Control values**: one value per control in the manifest, keyed by identifier, with each value's engine status (accepted, refused with the reason, not on the one-pass).
- **Render**: the output samples, the control values and seed it was made with, its length, latency and splice count, and whether it is stale.
- **Sidecar**: the versioned text record of D8 that belongs to an export.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: At each of the seven supported rates, a render equals the engine's direct output, sample for sample, on generated test files (tone, noise, a voiced burst with pauses).
- **SC-002**: An exported WAV read back equals the render bit for bit. A fresh session that opens the same input and the sidecar reproduces it bit for bit, at every supported rate.
- **SC-003**: 100 % of the engine's controls appear in the panel model, and a control added to the engine appears with no workbench change (proven by a test against the manifest).
- **SC-004**: A one-minute 48 kHz file renders in under 10 seconds on the development Mac with the default settings, and progress updates at least twice a second.
- **SC-005**: The A/B switch takes effect within 50 ms and keeps the position within 10 ms of the requested time.
- **SC-006**: Every refusal (format, rate, value, sidecar) gives a message that names what is wrong and leaves the session unchanged, in 100 % of the edge-case tests.
- **SC-007**: The owner completes the desk checklist on the Mac: open, render, A/B, change a setting, export, reopen the sidecar, reproduce.

## Assumptions

- The engine is the pinned rr-dr60-emu commit (0.92.0). Its control manifest, settings access by index (engine spec 025) and one-pass processing are used as published, and the workbench changes none of them.
- Alternatives considered for D1: WAV only (simpler, but excludes common lossless files); accepting lossy formats with a warning (rejected, since decoder artifacts would be read as the engine's).
- Alternatives for D2: always the mean (hides a channel's own content); refusing multichannel files (needlessly strict).
- Alternatives for D3: resampling at the boundary to the nearest supported rate (convenient, but it is signal processing of the workbench's own and changes the band the engine sees). It is deferred to a later feature, labelled, if wanted.
- Alternatives for D5: rendering automatically after each change, debounced (lively on short files, but surprising on long ones). It could come as an option later.
- Alternatives for D6: a short crossfade on the switch (smoother, but it alters the sound at the switch); level matching (it alters levels the engine produced).
- Alternatives for D8: the engine's processing record (spec 020) as the sidecar. It describes recordings on the record path, not one-pass renders, so the sidecar is the workbench's own. When feature 004 renders through the record path, it writes the engine's record as well.
- The one-pass default tap is the route output, the engine's default. Choosing other taps is feature 002, and the sidecar records the tap from 001 on.
- The development Mac is Apple silicon with the default output device. Other platforms are not tested in 001.
- The content digest is a cryptographic-strength hash of the decoded selected-channel samples and of the raw file bytes. The plan picks the hash.
