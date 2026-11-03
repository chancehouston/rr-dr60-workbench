# Feature Specification: The record and play paths

**Feature Branch**: `004-record-play-paths`

**Created**: 2026-11-02

**Status**: The draft review (B1 – B11) is applied. Under the owner's autopilot rule (2026-11-02), the decisions take effect at their recommendations.

**Input**: Feature 004 of docs/workbench-plan.md: "The recorder, the memory (items, capacity, erase, export and import of the stored form), playback with the speed levels and modes, route, volume and battery, stage bypass during playback." It builds on 001 – 003.

## Clarifications

### Session 2026-11-02 (autopilot)

- **The feature is split** (the review's scope advice):
  - **004** is the recorder, the memory, export and import, and **offline playback** with a schedule in the playback clock, giving a playback take that reproduces from its sidecar.
  - **Live playback** (the device engine moved into the callback, and the memory calls disabled meanwhile) is a new **004b** in the plan of record.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **A device.** The session gains one persistent **device engine** beside 001's fresh engine per render. It keeps its memory across operations. It is built from the panel's values when first used, and rebuilt (emptied) by **Reset device**. An idle control changed while the memory holds recordings goes through the engine's `reconfigure`, and its refusal is shown in the engine's words. Where the codec switch's refusal reads as a capacity problem (`StoredExceedsCapacity`), the screen adds: "one memory holds one encoding; erase first". A file at another rate reconfigures the device's rate and keeps the memory. | **One persistent device engine** |
| D2 | **Recording.** **Record** feeds the session's input from a start position (to its end, or for a length) through the device's recorder in 10 ms calls. The record-side values can be scheduled in the record stream's input clock, as in 003.<br>• **Refused at the start** (in the engine's words): a full memory (`MemoryFull`), or no slots left (`NoRecordingSlots`).<br>• **The recorder's own self-stop** when the memory fills (`RecordInfo.stopped`): the item is kept, and the workbench stops feeding without calling stop.<br>• **Otherwise** the workbench calls `stop_recording_with_changes` at the input's end.<br>• A stop that keeps nothing (`DiscardedEmpty`) adds no item, and says so.<br>The new item's number, length, recording counter and digest are shown, with the recorder's reports and change log. | **Offline recording into the memory** |
| D3 | **The memory.** A view lists each item: its number, its length in device samples and seconds, its recording counter ("imported" when the engine has none), and its digest. It also shows the status: capacity used and remaining, items, and slots left. Erase and Erase all use the engine's calls, which renumber the later items. Export writes an item's stored form (the engine's versioned bytes) to a `.rrdr60` file, and Import reads one. Refusals are in the engine's words (encoding, version, fit). | **Items, status, erase, export, import** |
| D4 | **Offline playback.** **Play to take** renders item N through the device's player, in 10 ms calls, until `PlayInfo.finished` (the drain included), into a **playback take**: the output, the change log, the frames and the meter rows.<br>• **Before the playback**, the playback controls (route, volume, battery, speed level and mode, the stage switches 10 – 13 of engine spec 021) are set from the panel.<br>• **During it**, a list of changes in the playback clock is applied just in time, as in 003's D3, on `Stream::Playback`. Changes land at the stream's position, with the engine's transitions and waits; the log gives each landing.<br>• **The playback length** is the engine's (`playback_length_samples_in_mode`), and the speed changes move it.<br>The take is shown, played (001's A/B) and exported like a render. | **Play to take, offline** |
| D5 | **The playback take and its sidecar.**<br>• **What it records**: a **playback sidecar** (`rr-dr60-workbench playback 1`) records the device's `settings()` and the five live playback getters read **immediately before `start_playback`** (route, volume, battery, level, mode), never the panel. It also records the item's `recording_digest`, the stored form's file name and SHA-256 (written beside it at export), the requested changes (clock, position, control, value, tag ≥ 1), and the produced count.<br>• **Replay**: import the stored form into a fresh device built from the recorded settings (the same encoding, so the item fits), check that `recording_digest` equals the recorded value, apply the five values with the plain setters, start the playback, feed the changes just in time and play to the produced count. That reproduces the output and the log bit for bit. The engine's import promises the original's length and playback output (`transport.rs:1186`); the counter and the memory position do not touch the player. | **A sidecar that replays** |
| D6 | **Readings.** For an item: its length, the playback length at each level in both modes, and the record and playback latencies. For a playback take: the frames, the change log and the place reached. | **The engine's figures** |
| D7 | **Not in 004**: live playback (004b), the processing record (005) and the microphone (007). | **Deferred** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Record into the memory and manage it (Priority: P1)

**Acceptance Scenarios**:

1. **Given** a file, **When** it is recorded, **Then** the item's stored form equals a direct engine's, fed the same span in one call and in irregular calls and stopped the same way (`recording_partition_twin`). Its playback also equals 001's one-pass render under engine spec 008 FR-009's recipe: the record latency, `silence_fed_samples` + 121 device samples of silence, and the output skipped by 180·rate/6000, on an engine built at the item's counter (`record_then_play_equals_one_pass`). This is confirmed at plan time in drop mode.
2. **Given** a memory with less time left than the input, **When** it is recorded, **Then** the recorder self-stops and the item is kept (`memory_full_self_stops`).
3. **Given** a full memory or no slots, **When** Record is pressed, **Then** it is refused in the engine's words (`record_refused_when_full`).
4. **Given** several items, **When** one is erased, **Then** the list renumbers and the status updates as the engine reports (`erase_updates_memory`).
5. **Given** an item, **When** it is exported and imported into a fresh device, **Then** the digest and the playback are identical (`export_import_roundtrip`).
6. **Given** a recording that keeps nothing, **When** it stops, **Then** no item is added, with the engine's reason (`empty_recording_discarded`).

### User Story 2 - Play to a take, with changes, and reproduce it (Priority: P2)

**Acceptance Scenarios**:

1. **Given** an item and a list of speed, route, volume and stage changes in the playback clock, **When** it is played to a take, **Then** each change is logged at its landing (`playback_changes_logged`).
2. **Given** a playback take, **When** its playback sidecar is replayed (import, settings, values, changes), **Then** the output and the log equal the take's bit for bit (`playback_take_reproduces`).
3. **Given** a playback whose values were changed by an earlier take, **When** a new take starts, **Then** its sidecar records the values in force at its start (`playback_start_values_recorded`).

### Edge Cases

- **Importing an unsupported version or another encoding**: refused, with the memory unchanged.
- **An idle change with recordings present**: the engine's `reconfigure` decides, and the device is unchanged on a refusal.
- **Erasing an item renumbers the later ones**: the view and the sidecars name items by number at the time of the take, and by digest.
- **A playback take whose speed changes move its length**: the take ends at `finished`.

## Requirements *(mandatory)*

- **FR-001**: The session MUST keep one device engine (D1), with Reset device.
- **FR-002**: Recording MUST follow D2, with the engine's refusals and self-stop (`recording_partition_twin`, `record_then_play_equals_one_pass`).
- **FR-003**: The memory view and its calls MUST follow D3.
- **FR-004**: Play to take MUST follow D4, through a shared chunk routine on the playback stream (003's `Due`, parameterized by stream).
- **FR-005**: A playback take MUST replay from its playback sidecar bit for bit (D5).
- **FR-006**: The readings of D6 MUST be shown, each as the engine's figure.
- **FR-007**: The feature MUST bring into use:
  - the recorder: `start_recording`, `record_with_changes`, `stop_recording_with_changes`, `max_record_events`;
  - the memory: `erase`, `erase_all`, `export_len`, `export`, `import`, `memory_status`, `recording_length`, `recording_counter_of`, `recording_digest`, `new_at_counter`;
  - playback: `start_playback`, `play_with_changes`, `stop_playback`, `playback_length_samples`, `playback_length_samples_in_mode`;
  - the latencies: `playback_latency_samples`, `record_latency_samples`;
  - the plain setters and getters: `set_route`, `set_volume`, `set_battery_volts`, `set_playback_speed`, `set_playback_speed_mode`, `route`, `volume`, `battery_volts`, `playback_speed`, `playback_speed_mode`;
  - `state`.

  The reach test MUST print the ratio (constitution II).
- **FR-008**: Every new string MUST pass the wording check.

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, recordings equal their partition twins, and their playback equals the one-pass under the recipe.
- **SC-002**: At seven rates, playback takes with seeded changes replay bit for bit from their sidecars.
- **SC-003**: The reach test prints at least 40/70.
- **SC-004**: The owner's desk checklist: record, list, erase, export, import, play to a take with a speed change and a stage bypass, then export and replay it.

## Assumptions

- The engine is pinned at `a06318c`. Erase and import are not real-time safe. That matters only for 004b, which moves the device into the callback.
- Tests use `capacity_s` = 2, so the memory fills quickly at seven rates.
