# Implementation Plan: The live engine

**Branch**: `003-live-engine` | **Spec**: [spec.md](spec.md), in force under autopilot

## Decisions (rationale in research.md)

| # | Decision |
|---|---|
| P1 | One chunk routine shared by the render and the live core, with just-in-time scheduling (R-01). |
| P2 | `RenderRequest.{start, changes}` and `Render.{log, live}`: a take is a render of its range and changes (R-02). |
| P3 | `rtrb` 0.4 for the queues (R-03). |
| P4 | Disabled reasons come from a dry-run schedule on a check engine (R-04). |
| P5 | Sidecar version 2 (R-05; contract). |
| P6 | A fill closure for `Player` (R-06). |

## Constitution Check

| Principle | Status |
|---|---|
| I. The engine renders; the skips are labelled silence, never engine output. | Pass |
| II. Reach adds `schedule_change`, `process_with_events_and_changes`, `stream_position`, `input_position`, `pending_changes`, `clear_schedule` (and `max_events`, already used). | Pass |
| III. A live take reproduces bit for bit from its sidecar v2. | Pass |
| IV. The callback is pre-allocated and tested without a device; overflows are counted. | Pass |
| V. `LiveCore` is in the core, tested by simulated callbacks. | Pass |
| VI. The labels: "gate paused: skipping", "incomplete (not reproducible)". | Pass |

## Project Structure

```text
crates/workbench_core/src/
  live.rs            (new) LiveCore, commands, the take
  render.rs          run_chunk, start, changes, log, live
  sidecar.rs         version 2
  session.rs         accept a take
crates/workbench_core/tests/
  live.rs, live_alloc.rs, sidecar_v2.rs, live_speed.rs
crates/workbench/src/
  playback.rs        the fill closure
  main.rs, views.rs  the live controls, the schedule list, the timeline
```

## Traceability

| Requirement | Test |
|---|---|
| FR-001, FR-002, US1 AS1 | `live::live_equals_render` (seven rates, seeded sizes) |
| FR-003, US1 AS2 – AS4 | `live::{live_change_logged, live_record_side_change, live_disabled_reasons}` |
| FR-004, US2 AS1, AS2 | `live::{schedule_lands_exactly, schedule_limit_refused}` |
| FR-005, US2 AS3 | `live::timeline_lists_reports_and_skips` |
| FR-006, FR-007, SC-001, US1 AS5 | `live::live_take_reproduces` (seven rates, seeded commands, stopped mid-file), `sidecar_v2::{grammar, v1_still_read}` |
| FR-008, US2 AS4 | `live::timeline_csv_roundtrip` |
| FR-009, SC-002, US1 AS6 | `live_alloc::live_callback_allocates_nothing` (own binary, run single-threaded), `live::overflow_marks_incomplete` |
| FR-010 | `reach` |
| FR-011 | `wording` |
| SC-004 | `live_speed` (release, ignored) |
| SC-003, SC-005 | the desk checklist |
