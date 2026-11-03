---
description: "Tasks for feature 007, the Mac's microphone as the input"
---

# Tasks: The Mac's microphone as the input

- [X] T001 [US1] `live.rs`: the capture mode (`new_capture`, `push`, `feed_chunk` split from `chunk`, `staged`), `capture_request`, `capture_wav`, `capture_start_request`, `monitor_capacity`; `LiveState.{monitor_drops, paused}`; `TakeBuilder.captured`; `input::mix_frame` shared with `mix_floats` (plan R-01, R-02; review fixes).
- [X] T002 [US1] Tests in `tests/capture.rs`: `capture_equals_offline` (seven rates), `capture_channel_and_mix`, `capture_commands_reproduce`, `capture_part_chunk`, `capture_wav_roundtrip`, `capture_sidecar_reproduces`, `capture_session_swap`, `capture_default_panel`, `capture_rate_refused`.
- [X] T003 [US2] `capture.rs` (core): `rate_refusal`, `folder`, `next_path`, `GapCounter`, `MonitorState` and `monitor_fill`, `SilenceWatch`; tests `monitor_never_touches_the_take`, `monitor_latency_bounded`, `gaps_counted`, `digital_silence_noticed`, `capture_folder_names`, `capture_incomplete`; `tests/capture_alloc.rs` `capture_allocates_nothing`.
- [X] T004 `Session::accept_capture` (the hand-over).
- [X] T005 The window: `crates/workbench/src/capture.rs` (the input unit, the device's loss), the Capture section, start and stop, the monitor's output unit, the poll (drain, silence, loss, the length limit), Save capture… (plan R-04).
- [X] T006 `playback_check --capture` (FR-007; run from the owner's terminal: it needs the microphone permission).
- [X] T007 The gate; reach; wording.
- [ ] T008 Post-implementation reviews (coach and hardware), applied before the PR (records.md).
