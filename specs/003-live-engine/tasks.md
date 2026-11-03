---
description: "Tasks for feature 003, the live engine"
---

# Tasks: The live engine

Test-first per story, tied to the plan's Traceability.

- [X] T001 Add `rtrb` 0.4 (core).
- [X] T002 `render.rs`:
  - the shared `run_chunk` with `Due` (per-clock cursors, scheduling just in time, refusals passed to a callback; a hard error offline);
  - `RenderRequest.{start, len, changes}` (the whole file stays the input; plan review B2);
  - `Render.{log, live}`;
  - `differs` covers the new fields.
- [X] T003 [US1] Tests in `live.rs`: `live_equals_render` (seven rates, a mid-file start, seeded sizes), `live_change_logged`, `live_record_side_change`, `live_disabled_reasons` (with plan review B1's tap case), `live_take_reproduces` (seven rates, seeded commands, stopped by hand, samples, reports, log and rows), `overflow_marks_incomplete`; and `live_alloc.rs`.
- [X] T004 [US1] `live.rs`:
  - `LiveCore` (10 ms chunks, the FIFO, the budget, the "now" commands with tags, the replies, the queues sized for a second's stall plus the worst callback);
  - `TakeBuilder::{drain, finish}`;
  - `live_reason` (timing first, then a dry run of every option);
  - `value_of`.
- [X] T005 [US2] Tests `schedule_lands_exactly`, `schedule_limit_refused` and `timeline_lists_reports_and_skips`; `csv::timeline_csv`.
- [X] T006 Sidecar version 2: write `live`, `start`, `fed` and the change lines; read versions 1 and 2; refuse tag 0 and a mismatch between decimal and bits. `Session::{plain_request, accept_live, range, clear_range}`, with staleness judged against the values at the stop.
- [X] T007 `playback.rs`: `start_with` (a fill closure) and `start_live`. `playback_check` adds a silent live run.
- [X] T008 The view: Go live / Stop live with a start position, edits sent as commands, disabled reasons, the latest meter row, the schedule list, the timeline with its CSV, and the incomplete label.
- [X] T009 `reach` lists the 003 methods (20/70). `live_speed` (release).
- [X] T010 Records, README, CLAUDE.md, CHANGELOG.
- [ ] T011 Post-implementation review, applied before the PR.
