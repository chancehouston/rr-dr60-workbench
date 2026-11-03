# Feature Specification: Processing records

**Feature Branch**: `005-processing-records`

**Created**: 2026-11-03

**Status**: The draft reviews (coach C1 – C14, hardware B1 – B8) and the plan reviews (coach P1 – P10, hardware Q1 – Q7) are applied. Under the owner's autopilot rule (2026-11-02), the decisions take effect at their recommendations.

**Input**: Feature 005 of docs/workbench-plan.md: "Write, read and re-render processing records (spec 020). Two renders side by side with a null test, a spectral difference and a parameter diff. Settings presets saved as files." It builds on 004 (the device and its recordings).

## Clarifications

### Session 2026-11-03 (autopilot, the draft reviews)

- **The feature is split** (C6). **005** covers the processing records: write, read and re-render. **005b** covers the comparisons of two renders and the settings presets; they depend only on 001 and 002. 005's draft decisions D5 – D9 for them, with the reviews' findings, are kept in `specs/005b-comparisons-presets/draft-decisions.md`. The plan of record gains the 005b row.
- **The recipe is ported** (C5, B3). The engine's re-render recipe and its filler exist only in the engine's unpublished harness and in its Swift package. The workbench mirrors them (Swift's `ReRender.reRender` and `fill`), and an engine issue asks for a public re-render (constitution I).
- **"Differs" is tested in the core** (C1, B2). An edited record fails its checksum and is refused, so the deltas are applied to the read-out in the core, as the engine's harness does.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **The run a recording keeps.** Each recording the device makes keeps its **run**, all that the engine's processing record needs:<br>• the recorder's gate reports, the stop flush's included, read into a buffer sized by `max_record_events` for the call's length (the stop's by one second of input); a count above the buffer is an error, never a truncation (B1);<br>• the record stream's change log, the stop's entries included;<br>• the stop's fields, by the engine's own conversions: `StopFields::from(&StopOutcome)` after a stop call, `StopFields::from(&RecordInfo)` at the recorder's self-stop (never built by hand);<br>• the recording counter before the start (`recording_counter`);<br>• where its input came from: the input file's name, the converted input's SHA-256, the channel, the conversion rule's version and the start offset in the file.<br>A run belongs to its item by the **recording counter**, which no erase or renumbering moves. An imported item has no run. Runs live in memory only: **Reset device** and quitting forget them (save the record first). | **One run per recorded item, by counter** |
| D2 | **Writing a processing record.** For an item, **Save processing record…** writes, beside each other:<br>• `<stem>.record.txt`: the engine's processing record, byte for byte as `processing_record_write` returns it, its length from `processing_record_len`;<br>• `<stem>.record-input.txt` (`rr-dr60-workbench record-input 1`): the record file's name and SHA-256, the input file's name, the converted input's SHA-256, the channel, the conversion rule's version, the start offset, the fed count, and `feed_point i24` or `feed_point f32`;<br>• `<stem>.record-input.i24`, only when `feed_point` is `i24`: the fed span as little-endian 32-bit integers holding 24-bit values. The feed point is `i24` when every fed sample round-trips through the engine's `input::i24_from_sample` and `samples_from_i24` bit for bit (16- and 24-bit integer files; the per-sample check decides, mixes included), so the engine's own recipes (its Swift `reRender`, its `re_render` example) can re-render it too. Otherwise the record re-renders in the workbench only, and the record-input file says so.<br>An imported item's record is the engine's imported-origin record, written with an empty run, and its record-input file names no input. Refusals are in the engine's words. | **The engine's bytes, the input named, the i24 span when exact** |
| D3 | **Reading a processing record.** **Open processing record…** reads a record with the device engine's `processing_record_read` and shows the read-out:<br>• the identity: the form version, the library version, each stage's model version, the host rate, the origin, the seed and the recording counter;<br>• the stored form: its version, the coder's model version, the encoding, the unit count and the digest;<br>• the run: the remaining capacity at the start, the gate's splices (or muted regions) and the change entries counted, and the stop's fields;<br>• every flag the engine set, by its name; the unknown setting names, listed;<br>• the slot: the component and its parameters;<br>• a **settings diff** against the panel: the setting controls and the slot, compared as engine readings (spec 025, so −0.0 equals 0.0), each difference with both values as the sidecar writes them; the live controls (route, volume, battery, speed, mode) listed as "not in a record" (C8).<br>**Check against item…** calls `recording_digest_matches` for an item of the device and shows whether they belong together. A refused record (an unknown version, a malformed line, a missing field, a checksum mismatch) shows the engine's reason. | **The engine's read-out, every field and flag** |
| D4 | **Re-rendering a processing record.** **Re-render…** takes a record and its record-input file (D2) and follows the engine's recipe:<br>1. build an engine with `new_at_counter` from the record's settings and counter;<br>2. if its remaining capacity is above the record's, **fill by import**: a throwaway 6 kHz engine with the record stage, the ADC clamp, the limiter, the gate and the microphone stages off and the codec as the record's, `capacity_s` = missing ÷ 6000 + 1, records the missing count of zeros, stops, exports, and the re-render engine imports it. The filler must be kept at exactly the missing length, and the remaining capacity must then equal the record's; otherwise the re-render is refused in words. The fill is never by recording (that moves the counter). It takes one item slot;<br>3. restore the slot: every parameter index the engine accepts, until it answers `NoSuchComponent`, then the component, before the schedule and the start;<br>4. schedule the record's change entries on the record stream, refilling while fewer than 64 are pending, before the start and after every call;<br>5. feed the session's input from the start offset, in 10 ms calls (`Pipeline::record`), up to the stop position;<br>6. stop as the record did: at the recorder's self-stop no stop call; otherwise a stop call (also when a self-stop was expected and did not happen, so the result honestly differs).<br>It then compares the kept item's digest with the record's (a stop that keeps nothing differs) and reports **reproduces** or **differs**, with both digests.<br>• **Refused in words**: no record-input file, or one whose record SHA-256 is not this record's; a session input whose SHA-256 or channel is not the record's input; an imported-origin record (it has no run); a component this engine does not know.<br>• **Shown before the run**: the flags `LIBRARY_DIFFERS`, `STAGE_MODEL_DIFFERS`, `HOST_RATE_DIFFERS` and unknown setting names. The run proceeds, and a "differs" under them is labelled expected (C10).<br>The re-render runs in the background with progress and cancel, as 001's render does. | **The engine's recipe, mirrored; the digest compared** |
| D5 | **Not in 005**: the comparisons and presets (005b), the coverage close-out (006) and the microphone (007). | **Deferred** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Write a processing record and re-render it (Priority: P1)

A user records an item on the device with record-side changes, saves its processing record, and later re-renders it from the record and the input file to show that the record and the recording belong together.

**Independent Test**: record an item at each rate, write its record, re-render it into a fresh engine, and compare the digests.

**Acceptance Scenarios**:

1. **Given** a recorded item, **When** its processing record is written, **Then** the bytes equal what a direct engine writes for the same input and calls, and the record reads back with no flag set (`record_written_as_engine`).
2. **Given** a written record and the session's input, **When** it is re-rendered, **Then** the digest equals the item's in sixteen cells (codec on/off × changes or none × drop/mute × stop call/self-stop) at seven rates (`re_render_reproduces`).
3. **Given** a record's read-out with one of the engine's effective deltas, **When** it is re-rendered, **Then** it differs. The deltas are: the seed +1; the counter +1; the knee to −30 dBFS; the threshold −3 dB; with the codec off, the hiss entry moved ⌈rate ÷ 6000⌉ host samples; with the codec on, the knee entry moved 0.1 s; the stop one frame earlier; in the self-stop cell, no fill. The twins (another volume, route, speed, battery, the output clamp toggled) still reproduce (`re_render_detects_deltas`).
4. **Given** a session input that is not the record's (another SHA-256 or channel), or no record-input file, **When** a re-render is asked, **Then** it is refused in words, naming the record's input file (`re_render_refuses_other_input`).
5. **Given** items erased and renumbered after a recording, **When** a later item's record is written, **Then** it carries that item's own run (`runs_follow_the_counter`).
6. **Given** an imported item, **When** its record is written, **Then** it is the engine's imported-origin record, and its re-render is refused in words (`imported_item_record`).
7. **Given** a 16-bit mono input, **When** a record is written, **Then** the `.i24` span is written, and feeding it through `samples_from_i24` gives the fed samples bit for bit; for a float input the feed point is `f32` and no span is written (`feed_point_rule`).

### User Story 2 - Read a processing record (Priority: P2)

**Independent Test**: open written, doctored and foreign records against a device and a panel.

**Acceptance Scenarios**:

1. **Given** a record, **When** it is opened, **Then** every read-out field and every flag the engine set is shown, and the settings diff against the panel lists exactly the settings that differ (`record_readout_shown`, `settings_diff_lists_differences`).
2. **Given** a record and the device's items, **When** it is checked against an item, **Then** the result is the engine's `recording_digest_matches` (`check_against_item`).
3. **Given** a damaged record (a changed byte, a missing line, an unknown version), **When** it is opened, **Then** it is refused with the engine's reason (`record_refusals`).
4. **Given** a record made at another host rate than the device, **When** it is opened, **Then** `HOST_RATE_DIFFERS` is shown. A re-render reproduces once the record's own input is open, because it builds its own engine at the record's rate (`record_other_rate`; plan review Q4: in the window the device's rate is the open file's, so the two happen one after the other).

### Edge Cases

- **A recording that kept nothing** has no item, so no record.
- **A record written before an erase**: the record names its digest, so **Check against item** tells whether an item still is that recording.
- **A recording whose input started past 0** re-renders from the start offset its record-input file names.
- **A memory whose remaining capacity is already below the record's** (a re-render engine with a smaller capacity in its settings cannot happen: the settings are the record's) needs no fill.
- **A record with more than 64 change entries** schedules them as the pending count allows.

## Requirements *(mandatory)*

- **FR-001**: The device MUST keep a run per recorded item, by counter, as D1 lists (`runs_follow_the_counter`, `record_written_as_engine`).
- **FR-002**: Writing a processing record MUST follow D2 (`record_written_as_engine`, `feed_point_rule`, `imported_item_record`).
- **FR-003**: Reading a record MUST follow D3 (`record_readout_shown`, `settings_diff_lists_differences`, `check_against_item`, `record_refusals`).
- **FR-004**: Re-rendering MUST follow D4 and reproduce the digest in every cell at seven rates (`re_render_reproduces`, `re_render_detects_deltas`, `re_render_refuses_other_input`).
- **FR-005**: The feature MUST bring into use `processing_record_len`, `processing_record_write`, `processing_record_read`, `recording_digest_matches`, `new_at_counter`, `encoding`, `record`, `set_component` and `set_component_parameter`, each called as `Pipeline::name(` where the workbench defines a method of the same name. The reach test MUST print the ratio (constitution II).
- **FR-006**: Every new string MUST pass the wording check (the denylist's stem "event": the screen says "splices" or "muted regions").
- **FR-007**: An engine issue MUST ask for a public re-render (and fill) in the engine, naming the workbench's mirror (constitution I).

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, every cell of US1 AS2 re-renders to the item's digest, every delta of AS3 differs and every twin reproduces.
- **SC-002**: The reach test prints at least 50/70.
- **SC-003**: The owner's desk checklist: record with a change, save the record, reset the device, open the record, re-render it; check it against an item.

## Assumptions

- The engine is pinned at `a06318c` (0.92.0); its processing record is form version 2.
- The run's positions are in the record stream's input clock from the recording's start, so the start offset in the file is the workbench's to keep (D2).
- Tests use `capacity_s` = 2, so the memory fills quickly at seven rates.
