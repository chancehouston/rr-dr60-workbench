---
description: "Tasks for feature 001, the offline render session"
---

# Tasks: The offline render session

**Input**: [spec.md](spec.md) (signed off), [plan.md](plan.md) (signed off, P1 – P13), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: **Test-first** (constitution V; the owner's cadence). In every story the test tasks come first and must **fail** before their implementation tasks begin. Each test task names the row of the plan's **Traceability** table it covers, as `[TR: …]`.

**Paths**: `core/` = `crates/workbench_core/`, `app/` = `crates/workbench/`. Integration tests live in `core/tests/`, and shared test helpers in `core/tests/support/mod.rs`.

## Phase 1: Setup

- [X] T001 Add the dependencies to `Cargo.toml` (workspace) and the two crate manifests:
  - `core`: `symphonia` 0.6 with `default-features = false` and features `wav`, `aiff`, `flac`, `pcm`; `hound` 3.5; `sha2` 0.11.
  - `app`: `rfd` 0.17; `coreaudio-rs` 0.14 under `[target.'cfg(target_os = "macos")'.dependencies]`.
  - Workspace: `[profile.test.package.rr_dr60] opt-level = 3` (R-09).
- [X] T002 [P] Add `core/build.rs`. It reads `../../Cargo.lock`, takes the 40-character sha of the `rr_dr60` package's `git+…#<sha>` source, emits `cargo:rustc-env=RR_DR60_COMMIT=<sha>` and `cargo:rerun-if-changed=../../Cargo.lock`, and panics if the source is missing (R-12). Expose `pub fn engine_commit() -> &'static str` in `core/src/lib.rs`.
- [X] T003 [P] Create the module skeletons named in [contracts/core-api.md](contracts/core-api.md): `core/src/{input,panel,render,ab,export,sidecar,monitor,text}.rs`. Each has a module doc and no logic, and `lib.rs` declares them. Move the bootstrap's `kind_label` and `timing_label` into `text.rs`.

## Phase 2: Foundational (blocks every story)

- [X] T004 Write the test support in `core/tests/support/mod.rs`:
  - deterministic stimuli with no OS randomness: a tone that ends with no trailing silence; noise from a fixed xorshift seed; a voiced burst with pauses (a 150 Hz pulse train through a two-pole resonance, 0.4 s on and 0.8 s off);
  - writers to a temporary directory: WAV via hound (i16, i24, i32, f32, 8-bit, A-law header, EXTENSIBLE 20-in-24); a hand-rolled AIFF writer (16, 24 big-endian) and AIFF-C `sowt`; a hand-rolled FLAC writer (STREAMINFO, verbatim subframes, CRC-8 and CRC-16; 16 and 24 bits, and 20 bits for the refusal); an MP3 stub of raw frame-header bytes (R-01);
  - `oracle(settings, engine_values, input_with_tail, block)`, which repeats R-09's calls on a fresh `Pipeline`.
- [X] T005 Add `core/src/text.rs` (FR-016). Every workbench string lives here as constants and format functions. On screen use "splices" or "muted regions", never "gate events" (P5).
- [X] T006 Add `Panel::from_entries(entries, rate)` and `Panel::new(rate)` = `from_entries(rr_dr60::controls(), rate)` in `core/src/panel.rs`. Build one `ControlRow` per entry in manifest order:
  - `class`: `Setting` if `control_map::settings_value(&Settings::new(rate), i)` is `Ok`, else `EngineControl` (R-08);
  - `value` at the engine's default reading;
  - `status`: `NotOnOnePass` when `entry.paths` excludes the one-pass, `ReadOnly` for `host_rate_hz`, else `Accepted`.

  Also add `snapshot()` → `(Settings, Vec<(u32, ControlReading)>)`, ordered so that the slot comes before its parameters, and the check engine `Pipeline::new(Settings::new(rate))`. No editing yet.

**Checkpoint**: the support helpers compile, and `Panel::new(48000)` yields 36 rows.

## Phase 3: User Story 1 – Render a file and hear the difference (P1) 🎯 MVP

**Goal**: open a file, render it with the defaults on a fresh engine with the tail, show the readings, and A/B on the engine's map.
**Independent test**: `cargo test -p workbench_core --test input --test render_oracle --test tail --test readings --test ab --test monitor --test monitor_alloc`.

### Tests (write first; they must fail)

- [X] T007 [P] [US1] `core/tests/input.rs` [TR: FR-001, D1, D3, US1 AS4; FR-002; FR-003, US1 AS3]:
  - `refuses_formats_rates_and_widths`: MP3, A-law, 8-bit, 20-bit FLAC, EXTENSIBLE 20-in-24, f64 and 22.05 kHz are refused, each with an error naming what is supported.
  - `conversion_rules`:
    - edge codes and 2¹⁶-step sweeps per width;
    - 24 bits equal to `rr_dr60::input::samples_from_i24` in WAV, AIFF, AIFF-C `sowt` and FLAC;
    - FLAC 16 and 24 bits routed by `bits_per_sample`;
    - 2³¹ − 1 → 1.0;
    - the mix: exact for integers, the float rule as restated.
  - `channels_and_mix`: the default is channel 1, every channel is offered, and the mix carries its label.
  - `digests`: SHA-256 of the raw bytes and of the converted f32 LE samples.
  - `counts_non_finite`.
- [X] T008 [P] [US1] `core/tests/render_oracle.rs` [TR: FR-007, SC-001, US1 AS1]: for seven rates × {tone, noise, voiced burst} × {drop, mute}, the render equals the whole-call and block-size-1 oracles sample for sample. Assert `recording_counter() == 0`.
- [X] T009 [P] [US1] `core/tests/tail.rs` [TR: D7, P12]:
  - `tail_emits_the_input`: seven rates, hang 50 ms, coder on, drop mode, an input ending on a tone; produced ≥ input + latency.
  - `tail_is_sufficient`: a further second of tail leaves the prefix bit-identical, and the extra output is below −100 dBFS with the defaults and the hiss off. Print the high-Q band-filter figure (reported only).
  - `tail_zero_at_front_taps`.
- [X] T010 [P] [US1] `core/tests/readings.rs` [TR: FR-009]:
  - every reading of FR-009 (as restated) on known stimuli;
  - `gate_predicate_pinned`: over every `Tap` × `vas.enabled`, a drop-mode burst plus 3 s of silence comes out shorter exactly when `gate_reached` is true (R-07);
  - not-emitted = 0 in mute mode, with the gate bypassed and at front taps;
  - the peak is `max |x|`.
- [X] T011 [P] [US1] `core/tests/ab.rs` [TR: FR-010, SC-005]:
  - hand-built report lists with exact values;
  - input 0 → latency;
  - the final-pause clamp;
  - at seven rates in drop and mute mode, for every position, `map(inverse(i)) ≥ i` and `inverse(map(o)) ≤ o`;
  - "same seconds" is the identity, clamped.
- [X] T012 [P] [US1] `core/tests/monitor.rs` [TR: FR-011]: `fill` writes identical left and right buffers equal to the take × volume, writes zeros past the end, swaps a request in at the top of a buffer, and switches the take bit.
- [X] T013 [P] [US1] `core/tests/monitor_alloc.rs` [TR: FR-011]: its own binary, with a counting `#[global_allocator]` and `--test-threads=1` documented in the file. Zero allocations and deallocations over 10⁵ `fill` calls with requests and volume changes.

### Implementation

- [X] T014 [US1] Implement `core/src/input.rs` (R-01, R-02, R-15, data-model "Input"):
  - `open(path, channel)` streams symphonia packets into the selected channel only, as `Arc<[f32]>` built once;
  - routes by `bits_per_sample`, and for S32 buffers with b < 32 checks the low bits are zero and shifts arithmetically right;
  - converts with `convert_i16`, `convert_i24` (calls `samples_from_i24`), `convert_i32`, and the mix;
  - refuses by codec id and width;
  - computes `raw_sha256`, `input_sha256` and the non-finite and subnormal counts.

  Make T007 pass.
- [X] T015 [US1] Implement `core/src/render.rs` (R-07, R-09, data-model "Render"):
  - `gate_reached(&Settings)` quotes `paths.rs:204-206` at `a06318c` in its doc comment and cites rr-dr60-emu#72.
  - `render(req, progress, cancel)` builds a fresh `Pipeline::new`, asserts the counter, applies the engine values in order, sets T = latency + drain (0 at the front taps; cites rr-dr60-emu#73), calls `process_with_events` in 4096 blocks with `max_events(4096)`-sized reports, appends `output[..produced]`, and collects the readings, the reports, the peak, the output digest and the footprint.
  - Cancel is checked per block, and progress is the input consumed.

  Make T008 – T010 pass.
- [X] T016 [US1] Implement `core/src/ab.rs` (R-06): `to_original` via `onepass_output_to_input`; `to_render` by binary search on the unclamped key `o + removed(o)` ≥ i + L; clamp to the target's end. Make T011 pass.
- [X] T017 [US1] Implement `core/src/monitor.rs` (R-05): `Shared` atomics, `Takes` (`Arc<[f32]>` clones) and `fill(takes, shared, left, right)`. Make T012 and T013 pass.
- [ ] T018 [US1] Implement `app/src/playback.rs` (macOS, R-04):
  - a `DefaultOutput` unit with a 2-channel non-interleaved f32 client format at the take's rate on the input scope;
  - `kAudioUnitProperty_SampleRateConverterComplexity` = Mastering;
  - a render callback that calls only `monitor::fill`;
  - listeners for the default-device change, `DeviceIsAlive` (stop and report) and `ProcessorOverload` (count);
  - the device's nominal rate read for the note;
  - `stop()` before the takes are dropped.

  On other platforms, a stub that reports `text::PLAYBACK_UNAVAILABLE`.
- [ ] T019 [US1] Build the US1 view in `app/src/main.rs`:
  - Open (rfd) and the channel picker;
  - Render, with progress and Cancel on a `std::thread` with the atomics;
  - the readings panel (FR-009 labels from `text`; "gate not reached");
  - the transport: play/stop, an A/B key and button, "engine map / same seconds", the monitor volume;
  - the device-rate and Mastering note, the peak warning above 1.0, the overload count.

  A new render or file stops playback first (P3).

**Checkpoint**: US1's tests pass, and desk steps 1 – 3, 9 – 12 and 14 work.

## Phase 4: User Story 2 – Change settings from the engine's own panel (P2)

**Goal**: every manifest control is editable through its own engine path and checked by the engine. The panel grows with the engine.
**Independent test**: `cargo test -p workbench_core --test panel`.

### Tests (write first; they must fail)

- [ ] T020 [P] [US2] `core/tests/panel.rs` [TR: FR-004, SC-003, US2 AS1; FR-005, US2 AS5; FR-006, US2 AS3, AS4; US2 AS2, AS6; US2 AS7]:
  - `presents_every_control`: every field of FR-004, for all 36 entries.
  - `grows_with_the_engine`: a copied entry list with one entry appended and one mutated appears with no other change.
  - `not_on_the_one_pass_and_read_only`.
  - `engine_checks_each_class`:
    - a sensitivity of 6 is refused in the engine's words, and Render is unavailable;
    - a volume out of range is refused through `set_control_value`;
    - the earphone tap with the speaker route is refused beside both controls via the zero-length `process`;
    - `reconfigure` is the whole check.
  - `sensitivity_5_matches_oracle` and `band_filter_matches_oracle`, each through `render` against the T004 oracle.
  - `stale_and_reset`.

### Implementation

- [ ] T021 [US2] Implement `Panel::set`, `reset_to_defaults`, `renderable` and the check order of R-08 in `core/src/panel.rs`:
  1. `validate` on the defaults plus the one value;
  2. `validate` on the whole settings;
  3. `reconfigure` on the check engine;
  4. the engine values through `set_control_value`, read back with `control_value`;
  5. the zero-length `process` for the tap and the route.

  Map each refusal to its row (using `setting_control` where the engine names a field), and to both rows for the tap and the route. Make T020 pass.
- [ ] T022 [US2] Implement staleness in `core/src/render.rs` or the session type: a render is stale when the panel snapshot, the channel, `input_sha256` or the tail choice differ from `render.request` (D5).
- [ ] T023 [US2] Build the panel view in `app/src/main.rs`:
  - stage groups in manifest order;
  - an editor per value type: a switch toggle, an integer field, a continuous drag or field showing the unit, a choice drop-down of the options;
  - the disabled reason;
  - sources and assumptions on hover;
  - the refusal text beside the row;
  - the stale marker and "reset to defaults".

**Checkpoint**: US2's tests pass, and desk steps 4 – 6 and 15 work.

## Phase 5: User Story 3 – Export a render and reproduce it later (P3)

**Goal**: the WAV + sidecar pair, written atomically, which reproduces the render bit for bit.
**Independent test**: `cargo test -p workbench_core --test export --test sidecar`.

### Tests (write first; they must fail)

- [ ] T024 [P] [US3] `core/tests/export.rs` [TR: FR-012, US3 AS1; SC-006 overwrite]:
  - `roundtrip`: read back with hound, bit-equal; the header is tag 0xFFFE with the IEEE-float GUID.
  - `failure_leaves_no_pair`: an injected `ExportFs` failure at each step (both creates, both writes, both renames) leaves neither a new pair nor a mixed pair.
  - `refuses_oversize`: data above `u32::MAX − 60` bytes, checked by arithmetic, not by writing.
  - `overwrite_replaces_the_pair`.
- [ ] T025 [P] [US3] `core/tests/sidecar.rs` [TR: FR-013; FR-014, SC-002, US3 AS2 – AS5]:
  - `values_roundtrip`: every value type, −0.0 → 0.0, and every range's ends.
  - `grammar`: every rule of [contracts/sidecar-v1.md](contracts/sidecar-v1.md); a duplicate key, an unknown key, a decimal disagreeing with its bits, or a bad first line → refused.
  - `reproduces`: at seven rates, export, then a fresh session (open, restore, render) gives equal samples and an equal `render.sha256`.
  - `differences_listed`: a different commit, an unknown id, a missing control, a refused value, a different input digest, a different `host_rate_hz`.
  - `refusals`: the session is unchanged.

### Implementation

- [ ] T026 [US3] Implement `core/src/sidecar.rs`: `write`, `read` and `restore` per the contract. Values are written as read back. The slot is restored before its parameters. `host_rate_hz` is never applied. Restoring gives a `RestoreReport`. Make T025's grammar and values tests pass.
- [ ] T027 [US3] Implement `core/src/export.rs`:
  - an `ExportFs` trait and a real implementation;
  - partial files, then removing the old sidecar, renaming the WAV, renaming the sidecar, with rollback on a failed second rename;
  - the size refusal (R-03).

  Make T024 and T025's `reproduces` pass.
- [ ] T028 [US3] Build the export and reopen view in `app/src/main.rs`: Export (rfd save, overwrite confirm) and Open sidecar, with the `RestoreReport` shown as a list.

**Checkpoint**: US3's tests pass, and desk steps 7, 8 and 13 work.

## Phase 6: Polish & cross-cutting

- [ ] T029 [P] `core/tests/reach.rs` [TR: FR-017] (R-14):
  - `cargo metadata` → the pinned `rr_dr60`;
  - scan every `.rs` file for `pub fn` in `impl Pipeline`, excluding `#[doc(hidden)]` items and `__` names;
  - scan `crates/*/src` for `.name(` and `Pipeline::name(`;
  - print `called/total` and the uncalled list (expect 15/74);
  - assert the spec's Engine reach list.
- [ ] T030 [P] `core/tests/wording.rs` [TR: FR-016] (R-13): parse `DENYLIST_STEMS`, `DENYLIST_PHRASES` and the suffix set from the pinned harness's `manifest_checks.rs` (fail on a parse failure), add the supplementary list (accurate, reveal, uncover, hidden voice, measured except "not measured"), and check every `text` string. Enumerate the strings with a `text::all()` list kept beside them, plus a source scan proving that `app/src` has no string literal outside `text`.
- [ ] T031 [P] `core/tests/source_scan.rs` [TR: FR-015]: no `SystemTime`, `rand`, `getrandom` or `thread_rng` in `crates/*/src`, and `Instant` only in the progress throttle.
- [ ] T032 [P] `core/tests/edge.rs` [TR: SC-006, edge cases]: `empty_file`, `metadata_and_many_channels` (a 10-channel WAV with LIST chunks), `non_finite_counted` (NaN, ±inf and subnormals counted and passed unchanged), `refusals_leave_session_unchanged` (format, rate, value, tap and route, sidecar, export size).
- [ ] T033 [P] `core/tests/render_speed.rs` [TR: SC-004]: `#[ignore]`, release only. One minute at 48 kHz renders in under 10 s, with the time printed.
- [ ] T034 [P] Add `core/examples/desk_files.rs`, which writes the quickstart's desk files into a given folder (the folder is ignored by `.gitignore`).
- [ ] T035 Update `README.md` (status, the dependency license note for symphonia's MPL-2.0, macOS-only playback), `CLAUDE.md` (status, the lessons learned) and `CHANGELOG.md` (001's entry).
- [ ] T036 Run the gate: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `cargo test -p workbench_core --test monitor_alloc -- --test-threads=1`, and the release `render_speed`. Record the figures in `specs/001-offline-render-session/records.md`.
- [ ] T037 Post-implementation reviews: `speckit-coach` and `hw-emulation-reviewer` on the branch. Apply the findings at their defaults, and record them in `records.md`.

## Dependencies

- Setup (T001 – T003) → Foundational (T004 – T006) → stories.
- US1 (T007 – T019) is the MVP. US2 needs only T006, and its oracle tests need T015.
- US3 needs T015 and T021, because a restore writes through the panel.
- Polish follows the stories; T029 – T034 can start as soon as their modules exist.

## Parallel examples

- After T006: the seven US1 test files T007 – T013 in parallel, then T014, T016 and T017 in parallel (separate modules), with T015 after T014.
- US2's T020 and US3's T024 and T025 can be written while US1 is implemented.
- Polish T029 – T034 in parallel.

## Implementation strategy

MVP first: Phases 1 – 3 give a usable render-and-listen tool with the defaults. Then US2 makes it a
tuning tool, and US3 makes its findings reproducible. Each checkpoint runs the story's tests and the
matching desk steps. The owner runs the whole desk checklist before merging.
