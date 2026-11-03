# Implementation Plan: Coverage close-out

**Branch**: `006-coverage-close-out` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)

## Summary

Three core additions and their window parts:
- `checks.rs` runs the session's request through the six one-pass call forms, and does the reset check (D1).
- `Device::playback_check` runs the playback forms × setter paths on clones, and `Device::call_readings` gives the readings by their own calls (D2, D3).
- `tour.rs` holds the tour's pages, the stage switches and the "without this stage" request (D4).

The reach test asserts 70/70, with a complete list (D5).

## Technical context

Rust 2024, egui 0.37. The engine is pinned at `a06318c`. No new dependencies.

## Constitution check

- **I**: the checks only compare the engine's own outputs; a disagreement is reported as an engine finding. The tour's words are the manifests'. The stage-status words are not published, so the tour shows the engine's variant names, and engine issue #79 asks for the words.
- **II**: 70/70 asserted.
- **III, IV**: no export or audio-thread change.

## Research

- **R-01 The forms** (`checks::run_on`):
  - It runs the render's own loop (block, tail, `Due::feed` before each call) with the form's call.
  - The in-place forms copy the chunk into the buffer and keep `buf[..produced]`.
  - A refused in-place call is classified by the two one-pass clocks not advancing (hardware B2).
  - The per-call record is `BlockInfo`'s four counts and `Pipeline::meters`.
  - `agrees` compares the output bits, the calls, and the events and log where the form reports them.
- **R-02 The reset check** (`checks::reset_check`):
  - Run, read `settings()` and every non-setting control (`control_value`: the live values and the slot), `reset`, run again.
  - The oracle is `Pipeline::new(settings)` with those values, run with the same schedule.
  - The comparison is bit for bit, with the logs compared without their tag-0 position-0 entries (hardware B1).
- **R-03 The playback check** (`Device::playback_check`):
  - Four clones of the device, each set by `set_control_value` or the plain setters (`set_plain`), the getters read, `start_playback`, then `play_with_changes` or `play` in 10 ms calls with `Due::feed_on`.
  - The four outputs are compared bit for bit, and against `play_to_take` on a further clone (hardware B6, coach 5).
- **R-04 The readings** (`Device::call_readings`): `record_latency_pre_encoder_samples`, the five getters, `component`, and `component_parameter_value` until the engine refuses an index.
- **R-05 The tour** (`tour.rs`):
  - The pages are the manifest's stage numbers, then 0. A page's controls are the control manifest's entries with that stage, and its switches the switch-valued entries.
  - `without_stage` refuses a stale render, a stage with no switch, or one whose switches are all off. Otherwise it turns the switches off in the request's settings and moves their scheduled entries to `removed`.
  - `no_effect` is bit identity.
- **R-06 The window**:
  - "Check the call forms" (Session tab, on its own thread).
  - "check forms" per device item.
  - The readings' new lines.
  - The Tour tab: Previous, Next, the page list, the manifest's fields, the controls with the panel's values and descriptions, and "Compare without this stage". That keeps B, switches the panel's switches off, renders and opens Compare, with the notes D4 lists.

## Testing

`tests/checks.rs`:
- `one_pass_forms_agree` (seven rates, drop and mute, a two-entry schedule);
- `reset_keeps_the_landed_values`;
- `in_place_refusal_reported`;
- `playback_forms_agree`;
- `readings_by_their_calls` (seven rates, codec off and on);
- `tour_follows_the_manifest`;
- `compare_without_stage`;
- `compare_without_stage_no_effect_and_refused`.

`tests/reach.rs` asserts 70/70 and the complete list.
