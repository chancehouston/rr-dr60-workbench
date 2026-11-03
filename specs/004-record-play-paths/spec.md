# Feature Specification: The record and play paths

**Feature Branch**: `004-record-play-paths`

**Created**: 2026-11-02

**Status**: Draft. Under the owner's autopilot rule (2026-11-02), the decisions take effect at their recommendations once the reviews are applied.

**Input**: Feature 004 of docs/workbench-plan.md: "The recorder, the memory (items, capacity, erase, export and import of the stored form), playback with the speed levels and modes, route, volume and battery, stage bypass during playback." It builds on 001 – 003.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **A device.** Beside 001's fresh engine per render, the session gains one persistent **device engine**. It keeps its memory across operations, the way the recorder keeps its recordings. It is built from the panel's values when it is first used and rebuilt by **Reset device**, which empties the memory. An idle control changed while the device has recordings is applied with the engine's own rules (`reconfigure`). A change the engine refuses for a memory that does not fit (for example, a smaller capacity) is shown in its words. | **One persistent device engine** |
| D2 | **Recording.** **Record** feeds the session's input (from a start position, for a length or to the end) through the engine's recorder at the file's rate, offline and in 10 ms calls. The recorder's own end applies: the stop, the memory full, or the item limit. The new item's number, length, recording counter and digest are shown, with the recorder's gate reports and change log. The record-side live values may be scheduled in the record stream's input clock, as in 003. | **Offline recording into the memory** |
| D3 | **The memory.** A memory view lists each item (number, length in device samples and seconds, recording counter, digest) and the status (capacity used and remaining, items and slots left). Erase and Erase all use the engine's calls. Export writes an item's stored form, the engine's versioned bytes, to a `.rrdr60` file, and Import reads one. Each is refused in the engine's words when the memory, the encoding or the version does not allow it. | **Items, status, erase, export, import** |
| D4 | **Playback.** **Play item N** plays it live through 003's output callback machinery: the device engine's player in 10 ms calls into a FIFO, captured as a **playback take**. While it plays, the live controls of the playback stream change at once and are logged: route, volume, battery, playback speed level and mode, and the stage switches 10 – 13 of engine spec 021. A control the playback stream refuses is disabled with the engine's reason, as in 003. A list of changes in the playback clock can be scheduled, as in 003's D3. Stop ends the playback. | **Live playback with live controls** |
| D5 | **The playback take.** The take holds the output, the change log, the player's reports (frames and finished) and the meter rows. It is shown and exported like a render: WAV f32 plus a **playback sidecar**. The playback sidecar holds: the identity (workbench version, engine version and commit); the stored form's digest (`recording_digest`) and its exported bytes' file name; the device's settings at the start of playback; the play controls' values at the start; the change lines in the playback clock; and the produced count. Re-playing offline (import the stored form into a fresh device built with the same settings, start the playback, apply the changes, play the same produced count) reproduces the take bit for bit. | **The playback take and its sidecar** |
| D6 | **Readings.** For an item: its length, the playback length at each speed level in both modes (`playback_length_samples`, `_in_mode`), and the record and playback latencies. While playing: the place in the recording, the speed level and mode in force, and the change log so far. | **The engine's figures** |
| D7 | **Not in 004.** The processing record of engine spec 020 (writing, reading and re-rendering a recording's record) is feature 005. The microphone is 007. | **Deferred** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Record a file into the memory and play it back (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a session with a file, **When** the user records it, **Then** the memory lists a new item with its length, counter and digest. The item's playback equals the engine's own record-then-play of the same input, sample for sample (`record_then_play_equals_engine`).
2. **Given** several items, **When** the user erases one, **Then** the memory and status update as the engine reports (`erase_updates_memory`).
3. **Given** a full memory, **When** the user records, **Then** the recorder stops at the memory's end and the item is kept, as the engine does (`memory_full_stops`).
4. **Given** an item, **When** it is exported and imported into a fresh device, **Then** its digest and its playback are identical (`export_import_roundtrip`).

### User Story 2 - Play live with the speed and stage controls (Priority: P2)

**Acceptance Scenarios**:

1. **Given** an item, **When** it is played live and the speed level changes from 3 to 5 and back, **Then** the changes land and are logged in the playback clock (`playback_change_logged`).
2. **Given** a playback take with changes, **When** its playback sidecar is re-played offline, **Then** the output equals the take bit for bit (`playback_take_reproduces`).
3. **Given** playback, **When** stage 12 (the speaker) is switched off live, **Then** the engine's 20 ms blend is in the take and logged (`stage_bypass_live`).
4. **Given** a control the playback stream refuses, **When** the panel shows it during playback, **Then** it is disabled with the engine's reason (`playback_disabled_reasons`).

### Edge Cases

- Recording into a full memory, or past the item limit: the engine's stop, shown.
- Importing a stored form of an unsupported version or another encoding: refused in the engine's words, and the memory is unchanged.
- An idle control changed with recordings present: the engine's `reconfigure` decides. A refusal is shown, and the device is unchanged.
- Erasing during playback: refused, as the engine does.
- A playback that reaches the end: the take ends with the drain, and the run stops by itself once drained.

## Requirements *(mandatory)*

- **FR-001**: The session MUST keep one device engine as D1 describes, with Reset device.
- **FR-002**: Recording MUST follow D2 and equal the engine's record path (`record_then_play_equals_engine`).
- **FR-003**: The memory view and its calls MUST follow D3, with the engine's refusals.
- **FR-004**: Playback MUST follow D4 through a real-time-safe callback (no allocation; tested without a device).
- **FR-005**: A playback take MUST reproduce from its playback sidecar (D5).
- **FR-006**: The readings of D6 MUST be shown, each as the engine's figure.
- **FR-007**: The feature MUST bring into use the recorder, memory and player methods: `start_recording`, `record_with_changes`, `stop_recording_with_changes`, `start_playback`, `play_with_changes`, `stop_playback`, `erase`, `erase_all`, `export_len`, `export`, `import`, `memory_status`, `recording_length`, `recording_counter_of`, `recording_digest`, `playback_length_samples`, `playback_length_samples_in_mode`, `playback_latency_samples`, `record_latency_samples`, `set_playback_speed`, `set_playback_speed_mode`, `playback_speed`, `playback_speed_mode`, `state`. The reach test MUST print the ratio (constitution II).
- **FR-008**: Every new string MUST pass the wording check.

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, a recorded item's playback equals the engine's own record-then-play.
- **SC-002**: At seven rates, a playback take with seeded speed, route, volume and stage changes reproduces bit for bit from its sidecar.
- **SC-003**: The playback callback allocates nothing over 10⁵ simulated callbacks.
- **SC-004**: The reach test prints at least 40/70.
- **SC-005**: The owner's desk checklist: record, play, change the speed live, bypass a stage, erase, export, import, reproduce a playback take.

## Assumptions

- The engine is pinned at `a06318c`. Its memory holds 60 minutes and 99 items by default (S-001).
- Alternatives considered:
  - **D1**: a fresh device per operation. It loses the memory, so it would not be a recorder.
  - **D2**: live recording through a callback. Without the microphone it adds nothing, and it is in 007.
  - **D5**: reusing the processing record. That record describes recording, not playback, and it is 005.
