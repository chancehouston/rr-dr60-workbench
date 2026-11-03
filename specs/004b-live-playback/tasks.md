# Tasks: Live playback

- [X] T001 `Due::feed_on_with` reports each scheduled change's outcome; `feed_on` keeps its behaviour on top of it (`crates/workbench_core/src/render.rs`).
- [X] T002 [US1] Tests first in `crates/workbench_core/tests/live_play.rs`: `live_play_equals_offline`, `live_commands_reproduce`, `live_refusal_reported`, `live_take_replays`, `live_stopped_early`; `live_play_allocates_nothing` in `tests/live_play_alloc.rs`.
- [X] T003 [US1] `crates/workbench_core/src/live_play.rs`: `LivePlayer` (the clone, 10 ms calls, FIFO, commands, the scheduled-order record), `PlayQueues`, `PlayTakeBuilder`; `Device::live_player` (`device.rs`).
- [X] T004 [US1] The playback sidecar records a take stopped early; `replay` compares the prefix.
- [X] T005 [US1] `crates/workbench/src/playback.rs`: start a live player in the output callback; the Device tab: Play live, the live controls, Stop, the take.
- [X] T006 `wording` and `reach` updated. Records.
- [X] T007 The gate.
- [X] T008 Post-implementation review (B1 – B10), applied before the PR (records.md).

## Phase 2: Convergence

- [X] T009 Record the review's restatements of D5 and D7 (and 004 D4) in the spec's clarifications, per D5 / D7 (partial)
- [X] T010 Record that 004b has no `plan.md`: its design is D1 – D7 and the records, per plan: artifacts (partial)
