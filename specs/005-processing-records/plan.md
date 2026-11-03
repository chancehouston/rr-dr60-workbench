# Implementation Plan: Processing records

**Branch**: `005-processing-records` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)

## Summary

The device keeps a run per recorded item (D1). From it the workbench writes the engine's processing record, plus a small record-input file and, when the input is exact on the 24-bit grid, the fed span as `.i24` (D2). It reads records with the device engine's reader and diffs their settings against the panel (D3). It re-renders a record by a mirror of the engine's recipe (Swift's `ReRender`), and compares the digests (D4).

## Technical context

Rust 2024, egui/eframe 0.37 (as 001 – 004b). The engine is pinned at `a06318c` (0.92.0, form version 2). No new dependencies. New core module `crates/workbench_core/src/records.rs`; changes in `device.rs` and the window's Device tab.

## Constitution check

- **I (engine the source of truth)**: the record's bytes are the engine's writer's, its reading is the engine's reader's, the digest comparison is the engine's digest. The recipe is mirrored from the engine's own (the harness and Swift) because no public re-render exists; FR-007's engine issue names the gap. The settings diff compares engine readings: the pure `control_map::settings_value` for the settings fields, and `control_value` on a small check engine for the slot (R-06). It copies no rules.
- **II (everything reachable)**: nine methods brought into use (FR-005); the reach test prints the ratio.
- **III (faithful and reproducible)**: the record is written byte for byte; the `.i24` span is written only when it is exact (R-03).
- **IV (real-time safety)**: nothing here runs in a callback.

## Research

- **R-01 The run.** `RecordOutcome` gains `stop: StopFields` and `counter: u64` (B1). `StopFields::from(&StopOutcome)` after `stop_recording_with_changes`; `StopFields::from(&RecordInfo)` at a self-stop (the engine's writer fills the padding start, `transport.rs:1040`). The event buffers are sized when used: a call's by `max_record_events(n)` after the just-in-time schedule, the stop's by `max_record_events(rate)` just before the stop (Q3: a hang change scheduled late raises the bound). A self-stopping call over its buffer erases the item it kept (Q2); a call's `info.events` above its buffer is `Error::BufferTooSmall` and abandons the recording, never truncates. `max_record_events` is always enough, so this is a defensive check with no test (P5). The device keeps `runs: BTreeMap<u64, Run>` keyed by the counter at the start; `Run` holds the outcome's events, log, stop and a `RunSource` the caller passes into `Device::record` (the file name, the converted input's SHA-256, the channel, the conversion version, the start).
- **R-02 Writing.** `Device::processing_record(n)`: for an item with a run, `RunData { events, changes, stop }` → `processing_record_len` → a buffer → `processing_record_write`. For an imported item (`recording_counter_of` is `None`), an empty run (`RunData` with no rows and `StopFields::default()`): the engine ignores the run for imported records (`run_changes`, `transport.rs:983`); the plan's test pins that the write succeeds. A recorded item whose run the device does not hold (cannot happen in-session) is refused (`NoSuchRecording`). Runs are stored for `Kept` and `SelfStopped` only, never for `DiscardedEmpty` or an abandoned recording; erase does not prune them (a counter never returns), and Reset device drops them with the engine (P5).
- **R-03 The feed point.** `feed_point_i24(x: &[f32]) -> bool`: every sample `s` satisfies `samples_from_i24([i24_from_sample(s)]) == s` bit for bit. The fed span is `input[start .. start + stop_input_position]` (clamped to the input). When true, `.i24` holds `i24_from_sample` of each, little-endian `i32`. The check compares bits (−0.0 fails, plan review Q5). An integer 16- or 24-bit file's conversion (x ÷ 2¹⁵ or x ÷ 2²³) is on the grid; a channel mix may or may not be, and the per-sample check decides.
- **R-04 The record-input file.** [contracts/record-input-v1.md](contracts/record-input-v1.md). Written and parsed by the same key-value line form as the sidecars; unknown keys refused; the record file's SHA-256 is checked at a re-render (a record-input file belongs to one record).
- **R-05 Reading.** `Device::read_record(bytes) -> Result<RecordReadout, Error>` is `processing_record_read`. The change rows come from `record::record_changes`, the unknown names from `record::record_unknown_setting`. `Device::check_record(n, &mut readout)` is `recording_digest_matches`. The flags are named by a table of the eight `record::flags` constants, and an unknown bit shows as hex (P9). The splice rows (up to 500 shown) come from `record::record_event`, with each row's host input position, and the stage mask is shown beside `STAGE_MODEL_DIFFERS` (P9).
- **R-06 The settings diff** (restated by plan review Q1). An imported record carries no settings: `settings_diff` returns `None` and the window shows "no settings in an imported record". Otherwise the 26 settings fields are read by the pure `control_map::settings_value(&readout.settings, i)`, with no engine and so no reservation of the record's memory and no refusal of another library's settings. The slot's controls (31 – 35) are read by `control_value` on a small check engine (`Settings::new(rate)`, capacity 1 s) with the record's slot restored. That is skipped when the record names an unknown component. They are compared with the panel's readings, −0.0 equal to 0.0. The five live values (`device::PLAY_VALUES`) are not in a record and are listed as such.
- **R-07 The re-render.** `records::Recipe { settings, counter, remaining_at_start, component, store, changes, stop }` from a read-out and its change rows; `records::re_render(recipe, fed: &[f32], progress, cancel) -> Result<Outcome, ReRenderError>` mirrors Swift's `reRender`:
  1. `Pipeline::new_at_counter(settings, counter)`;
  2. the fill by import when `memory_status().remaining_samples > remaining_at_start`: `filler(encoding, missing)` (Swift's `fill`: `Settings::new(6000)` with `record_stage_enabled`, `adc_clamp_enabled`, `agc.enabled`, `vas.enabled`, `mic.capsule_enabled`, `mic.amp_enabled` false, the codec as the record's, `capacity_s = missing / 6000 + 1`; `start_recording`, `Pipeline::record` of `missing` zeros, `stop_recording`, kept with length `missing`, export); `import`; then remaining must equal `remaining_at_start`, else `ReRenderError::Fill`;
  3. the slot (`records::restore_slot`): `components::parameter_at` is crate-private, so every store index is offered to `set_component_parameter(BeforeLimiter, i, store[i])` until the engine answers `NoSuchComponent` (its own end of the parameter list; the harness restores the same leading indices), then `set_component(BeforeLimiter, component)` (plan review P1);
  4. the schedule while `pending_changes(Record) < 64`, before the start and after every call;
  5. `start_recording`; `Pipeline::record` in 10 ms calls (`block_for`) up to `stop.stop_input_position` (clamped to `fed`); progress per call; cancel between calls (a cancelled re-render reports `Cancelled`, nothing else);
  6. no stop call after a self-stop; otherwise `stop_recording`;
  7. the digest of the kept item (`StopResult::Kept { number }`, or `count` after a self-stop) against the record's; a stop that keeps nothing is `NothingKept` (Q7).
  `Outcome { digest, expected, reproduces }`. Deltas (US1 AS3) mutate the `Recipe` in the tests; "no fill" is a test-only flag `Recipe::fill` (true in the window).
- **R-07b Preparing a re-render** (plan review P3). `records::prepare(record: &[u8], record_input: &str, input: &Input) -> Result<(Recipe, RecordReadout, Range<usize>), ReRenderError>` holds every refusal, one named variant each:
  - a record-input file that does not parse;
  - a record whose SHA-256 is not the record-input file's (`NotThisRecord`);
  - an imported origin (`Imported`);
  - an unknown component (`UnknownComponent`);
  - a session input whose SHA-256 or channel is not the named one (`OtherInput`).
  The span is `start .. start + fed`, clamped to the input.
- **R-08 The window.** The Device tab gains, per item, **Save record…** (a save dialog for `<stem>.record.txt`; the two companion files beside). A **Processing record** section: **Open record…**, the read-out as a grid (identity, stored form, run, stop, flags by name, unknown names, slot), the settings diff, **Check against item** with a number, and **Re-render** (uses the session's input; the record-input file is found beside the record by name; refusals in words). The re-render runs on a thread with progress and cancel (001's `Job` pattern, its own job slot).
- **R-09 Reach.** `Pipeline::record(` (the re-render's feed and the filler), `Pipeline::encoding(` (the filler's codec) and the record methods by their names; `set_component(`/`set_component_parameter(` are unique names. Expected 50/70.
- **R-10 Test cost.** Sixteen cells × seven rates of record-then-re-render at `capacity_s` 2 with inputs of about 1.5 s (the self-stop cells 3 s). Under the coverage profile the harness's three rates (as 004).

## Project structure

```text
crates/workbench_core/src/records.rs     # new: Run, RunSource, writing, the feed point, the record-input file, the diff, the recipe
crates/workbench_core/src/device.rs      # RecordOutcome.stop/counter, runs by counter, processing_record, read_record, check_record
crates/workbench_core/tests/records.rs   # US1, US2
crates/workbench/src/main.rs             # the Device tab's record section
```

## Testing

The deltas of US1 AS3 (plan review P2):

| Delta | Cell | Applied to | Premise |
|---|---|---|---|
| seed +1 | codec off, no changes, drop, stop call | Recipe | — |
| counter +1 | the same | Recipe | — |
| knee to −30 dBFS | the same | Recipe | the −20 dBFS voice crosses it |
| threshold −3 dB | the same | Recipe | the voice has pauses |
| hiss entry moved ⌈rate ÷ 6000⌉ | codec off, changes | Recipe | the hiss's 20 ms ramp |
| knee entry moved 0.1 s | codec on, changes | Recipe | — |
| stop 120 device samples earlier | stop call, codec off and on | Recipe | — |
| no fill | self-stop, after one prior item | Recipe (`fill` false) | `missing > 0` asserted |
| twins: volume, route, speed, battery | any | the recording device, before the recording | still reproduces |
| twin: output clamp toggled | any | Recipe settings | still reproduces |

The self-stop stimulus keeps the gate open (voice, mute mode or no pauses), so 3 s of input fills 2 s of memory.

Further tests (P4, P8, and the post-implementation reviews): `re_render_refills_schedule` (at least 70 entries), `re_render_from_offset` (also from the decoded `.i24` span), `re_render_unknown_component` (a read-out with the flag set), `re_render_cancel`, `flags_label_expected` (`records::expected_note`), `flag_names_every_bit`, `slot_restored` (the band filter, seven rates, the two slot deltas, the diff's slot rows), `live_value_twins_reproduce`, `late_hang_change_keeps_recording` (the bound rises on a check engine; the device's over-buffer paths are guards no 10 ms call reaches), `filler_has_the_length`, `record_input_roundtrip_and_refusals`, `negative_zero_equals_zero` (through `settings_diff`), `companion_paths_beside_the_record`, and `wording`'s `record_view_debug_names_pass`. `feed_point_rule` also re-renders from the decoded `.i24`, fed from 0, to the item's digest. `tests/wording.rs` covers the new strings (FR-006). The engine issue is #76 (FR-007).

US1 and US2 as named in the spec, in `tests/records.rs`. `record_written_as_engine`'s oracle is a direct engine driven by the test (the same calls, its own buffers), not `Device`. `re_render_reproduces` against the item's `recording_digest`. The deltas as US1 AS3 lists them, applied to the `Recipe`.
