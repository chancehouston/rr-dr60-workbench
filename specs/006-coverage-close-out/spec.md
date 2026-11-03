# Feature Specification: Coverage close-out

**Feature Branch**: `006-coverage-close-out`

**Created**: 2026-11-03

**Status**: Under the owner's autopilot rule (2026-11-02). The draft reviews (coach 1 – 10, hardware B1 – B7) are applied, and their questions take their defaults.

**Input**: Feature 006 of docs/workbench-plan.md: "The `Pipeline` reach test at 100 % (constitution II): every remaining method. A guided tour of the stages." It builds on 001 – 007. At its start 51 of 70 methods are reached. The 19 left are:
- the five live getters and five plain setters (`route`, `volume`, `battery_volts`, `playback_speed`, `playback_speed_mode`, and their setters);
- the slot's reads (`component`, `component_parameter_value`);
- `reset`;
- the call forms `process_with_events`, `process_in_place`, `process_in_place_with_events`, `process_in_place_with_events_and_changes` and `play`;
- `record_latency_pre_encoder_samples`.

## Clarifications

### Session 2026-11-03 (autopilot, the draft reviews)

- **The reset render** (coach 1, hardware B1). The engine's `reset` keeps every value a schedule landed (the live values, the record-side values, the bypass switches, the slot's store). So the reset render's oracle is a fresh engine built from the first engine's state after its run: `Pipeline::settings()`, the live getters and the slot, with the same schedule. The audio compares bit for bit. The log compares after dropping the tag-0 entries at position 0 (the fresh engine's plain setters log them; the reset engine's start values do not).
- **Equal, per form** (coach 2, hardware B7). For each 10 ms call: the output bits, `BlockInfo` (produced, events, paused, changes) and `Pipeline::meters`. For each form: the event lists for the forms with events, and the log for the forms with changes. Every form runs the render's block, tail and just-in-time schedule (`Due`), against `render.request`. The in-place forms refill the buffer from the input each call and keep `buf[..produced]`.
- **The tests assert agreement** (coach 3). The window reports a disagreement as an engine finding, and a red test becomes an engine issue.
- **A refusal** (coach 4, hardware B2). With a named route tap of the other route, every form refuses: the returning forms with `TapNotOnRoute`. An in-place call is classified as refused when neither the input position nor the stream position advances. A pause advances the input clock.
- **The playback check** (coach 5, hardware B6). Each take runs on a clone of one device snapshot, so the device's values are unchanged afterwards. The getters are read before `start_playback`, compared as f32. Each take equals 004's play to take.
- **The readings' oracles** (hardware B3). `component` and `component_parameter_value` compare with the values set on the device. `record_latency_pre_encoder_samples` equals `record_latency_samples` with the codec off, and that figure less the coder's 180 device samples at the host rate (±1) with it on, at seven rates.
- **"Compare without this stage"** (coach 7, hardware B4, B5):
  - **The switches.** The stage's switches are the control manifest's switch entries with that stage number (stage 4 has two). Stages 8, 9 and 15 have none; stage 9 points to stage 7's.
  - **What it refuses and replaces.** It refuses a stale render. It replaces B, saying so, and leaves the panel's switches off, saying so.
  - **The schedule.** It removes the request's own scheduled entries of those switches, saying so: they would be refused with the stage off.
  - **No effect.** When A equals B bit for bit, it reports "no effect at this tap and route", from the engine's own result.
  - **Alignment.** A gate-off comparison in drop mode does not line up sample for sample; the tour says so.
- **The tour's words** (coach 8). Its words are the manifest's: status, path and confidence by the manifest's own option words, and each switch's description from the control manifest. The stage-0 controls (the slot, the output clamp, the seed, the tap, the rate…) are on a "no stage" page. A source scan finds no stage-text literals in the tour's module.
- **The reach test** (coach 6). Each of the 19 new methods is called as `Pipeline::name(`. The scan ignores comments, and the de-duplicated method list must equal the engine's method set.
- **Runtime** (coach 10): the checks run on a background thread with progress and cancel.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **The entry-point check, one-pass.** **Check the call forms** (Session tab) renders the session's request through each of the engine's one-pass call forms: `process_with_events_and_changes` (the render's), `process`, `process_with_events`, `process_in_place`, `process_in_place_with_events` and `process_in_place_with_events_and_changes`. It does so in 001's 10 ms calls on a fresh engine with the same schedule. It also renders through a **reused engine after `reset`**: one render, `reset`, the same render again. It reports for each whether the output equals the render's bit for bit. Where the form reports them, it also compares the gate reports and the change log. A difference is a finding about the engine, shown as such. | **Six forms and a reset, against the render** |
| D2 | **The entry-point check, playback.** **Check the call forms** for a device item plays it to a take through `play` and through `play_with_changes`. It sets the playback values once by `set_control_value` (004's path) and once by the plain setters (`set_route`, `set_volume`, `set_battery_volts`, `set_playback_speed`, `set_playback_speed_mode`), and reads them back by the plain getters. It reports whether the four takes are bit-identical and whether each getter reads the value set. | **Two forms × two setter paths** |
| D3 | **The readings.** The device's readings gain:<br>• the record path's latency to the encoder (`record_latency_pre_encoder_samples`);<br>• the live values by the plain getters;<br>• the slot's component and parameters by `component` and `component_parameter_value`, beside the by-index readings, which they must equal.<br>A disagreement is shown as a finding. | **Every reading by its own call** |
| D4 | **The guided tour.** A **Tour** tab steps through the stage manifest's 15 stages in order (Previous, Next, a stage list). For each stage it shows:<br>• the manifest's own fields (002's panel);<br>• the stage's controls (the control manifest's entries with that stage), with the panel's values;<br>• for a stage with an on/off switch among its controls, **Compare without this stage**. That keeps the current render as B, switches the stage's switches off in the panel and renders. The Compare tab (005b) then shows A, without the stage, against B, with it.<br>A stage that is not modeled says so, with the manifest's sentence. Every word is the engine's or the manifest's; the workbench writes no stage text. | **The manifest, stage by stage, with a comparison** |
| D5 | **The reach test.** It asserts 70/70: every public `Pipeline` method called (constitution II). Its list of methods by feature is complete. | **70/70 asserted** |
| D6 | **Not in 006**: the follow-ups recorded by 005 – 007 (they stay in their records). | **Deferred** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Check every entry point (Priority: P1)

**Independent Test**: core checks on synthetic renders and items at seven rates.

**Acceptance Scenarios**:

1. **Given** a request with a schedule, in drop and mute modes, at seven rates, **When** the one-pass forms are checked, **Then** every form agrees with the render per call (output, `BlockInfo`, meters), the forms with reports give the same gate reports, and the forms with changes the same log. The reset render equals its fresh-from-state oracle (`one_pass_forms_agree`).
2. **Given** a device item and playback values, **When** the playback forms are checked, **Then** the four takes are bit-identical and each getter reads its value (`playback_forms_agree`).
3. **Given** a device with a band filter in the slot and set parameters, **When** its readings are read, **Then** `component` and `component_parameter_value` equal the values set. The pre-encoder latency follows the codec rule at seven rates (`readings_by_their_calls`).
4. **Given** a named route tap of the other route, **When** checked, **Then** the returning forms give `TapNotOnRoute`, and the in-place forms are classified refused, with neither clock advancing. A drop-mode pause is not classified refused (`in_place_refusal_reported`).

### User Story 2 - The guided tour (Priority: P2)

**Independent Test**: the tour's pages and the comparison's request, built in the core.

**Acceptance Scenarios**:

1. **Given** the tour, **When** stepped through, **Then** the 15 stage pages and the "no stage" page partition all 36 controls, and each page shows the manifest's fields in its own words (`tour_follows_the_manifest`).
2. **Given** a stage with switches and a current render, **When** "Compare without this stage" is asked, **Then** the request has exactly that stage's switches off and its scheduled entries of them removed. The parameter diff lists exactly those controls and removed entries (`compare_without_stage`).
3. **Given** a stage past the tap, **When** compared, **Then** "no effect" is reported (`compare_without_stage_no_effect`). **Given** a stale render, it is refused (`compare_without_stage_refused`).

### Edge Cases

- **No render yet**: "Compare without this stage" asks for a render first.
- **A stage with no switch** (or not modeled): no comparison is offered.
- **A playback check on an empty memory**: refused in words.

## Requirements *(mandatory)*

- **FR-001**: The one-pass check MUST follow D1 (`one_pass_forms_agree`, `in_place_refusal_reported`).
- **FR-002**: The playback check MUST follow D2 (`playback_forms_agree`).
- **FR-003**: The readings MUST follow D3 (`readings_by_their_calls`).
- **FR-004**: The tour MUST follow D4 (`tour_follows_the_manifest`, `compare_without_stage`).
- **FR-005**: The reach test MUST assert 70/70 (D5), with the 19 new methods mapped:
  - `process_with_events`, `process_in_place`, `process_in_place_with_events`, `process_in_place_with_events_and_changes` and `reset` to FR-001;
  - `play` and the ten plain getters and setters to FR-002;
  - `component`, `component_parameter_value` and `record_latency_pre_encoder_samples` to FR-003.
- **FR-006**: Every new string MUST pass the wording check.

## Success Criteria *(mandatory)*

- **SC-001**: The reach test prints and asserts 70/70.
- **SC-002**: At seven rates, every call form agrees with the render (asserted).
- **SC-003**: The owner's desk checklist: run both checks; step through the tour and compare without one stage.

## Assumptions

- The engine is pinned at `a06318c` (0.92.0); 70 public `Pipeline` methods.
