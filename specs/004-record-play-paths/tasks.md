---
description: "Tasks for feature 004, the record and play paths"
---

# Tasks: The record and play paths

- [X] T001 `Due::feed_on` with a stream parameter (the record stream's input clock, the player's output clock).
- [X] T002 [US1] Tests in `device.rs`: `recording_partition_twin` (seven rates, one call and irregular calls, against a direct engine's stored form), `memory_full_self_stops`, `record_refused_when_no_slots`, `erase_updates_memory`, `export_import_roundtrip` (and another encoding refused), `empty_recording_discarded`.
- [X] T003 [US1] `device.rs`:
  - `Device::{new, settings, sync, status, items, record, erase, erase_all, export, import, readings}`;
  - `RecordEnd`, `RecordOutcome`, `Item`.
- [X] T004 [US2] Tests `playback_partition_twin`, `playback_take_reproduces` (seven rates, seeded speed, route, volume, stage-12 and mode changes; replayed through the sidecar and the stored form), `playback_start_values_recorded`.
- [X] T005 [US2] `Device::play_to_take` (values by index, then the start state, then the changes just in time on `Stream::Playback`), `playback_sidecar`, `read_playback`, `replay`.
- [X] T006 The panel's rows that are not on the one-pass are settable (the device uses them). The sidecar restore applies them. The round-trip test is updated.
- [X] T007 The Device tab:
  - build, apply settings, reset;
  - record from a position;
  - the item list with play to take, export and erase; erase all; import;
  - the readings;
  - the take: listen, and export (WAV, stored form, playback sidecar).
- [X] T008 `reach` lists the 004 methods (41/70). Records.
- [X] T009 Post-implementation review (B1 – B10), applied before the PR (records.md).

## Phase 3: Convergence

- [X] T010 Show the playback take's change log and the place reached in the Device tab, and carry `place` on `PlayTake`, per D6 / FR-006 (partial)
- [X] T011 Add `playback_changes_logged` (each change logged at its position with its tag and stored value; the place reached equals the item's length) per US2/AC1 (missing)
- [X] T012 Record that 004 has no `plan.md`: its design decisions are D1 – D7 in the spec and the review records, per plan: artifacts (partial)
