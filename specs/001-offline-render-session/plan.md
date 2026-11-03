# Implementation Plan: The offline render session

**Branch**: `001-offline-render-session` | **Date**: 2026-11-02 | **Spec**: [spec.md](spec.md) (signed off 2026-11-02) | **Plan signed off by the owner 2026-11-02** (P1 – P13 as recommended)

**Input**: Feature specification from `specs/001-offline-render-session/spec.md`

## Decisions for sign-off

Read this section first. The rationale and alternatives for each row are in [research.md](research.md).

| # | Decision | Recommended |
|---|---|---|
| P1 | **Decoder.** `symphonia` 0.6, with only the WAV, AIFF, FLAC and PCM features (A-law and µ-law WAV refused by codec id), routed by the codec's bit depth (FLAC delivers 16 and 24 bits shifted into 32), so a lossy file is "unsupported format" by construction. Its license is MPL-2.0, file-level copyleft, which is fine as a dependency of an MIT tool, and the README notes it (R-01). | **symphonia, three formats only** |
| P2 | **Playback on macOS only, through CoreAudio's default output unit**, with the take's rate on the unit's input scope as **two identical channels** and the converter at **Mastering** quality. A changed or lost output device stops playback with a message. The operating system then does the only rate conversion, which the spec allows. A spike verified 6 – 96 kHz on the development Mac. On other platforms playback is unavailable in 001; render and export still work (R-04). | **CoreAudio, macOS only in 001** |
| P3 | **The playback callback** reads two takes in memory through atomics only. A new render or file **stops playback** before the takes are swapped, so the callback never drops memory. Dropouts are counted from the device's overload notifications (R-05). | **Stop on a new render** |
| P4 | **The A/B map** is the engine's `onepass_output_to_input`, inverted by binary search on its unclamped key, so input 0 maps to the latency and not to 0. A position inside a removed pause lands on the splice (R-06). | **Invert the engine's map** |
| P5 | **Wording restatement.** The engine's denylist contains the stem "event", so the screen says **"splices"** (drop mode) and **"muted regions"** (mute mode), never "gate events". FR-009 itself is unchanged (R-07). | **Splices / muted regions** |
| P6 | **The wording check** reads the engine's denylist from the pinned checkout's harness source. A dev-dependency on the harness would switch on the engine's test hooks in our test build. The fix belongs in the engine: **an engine issue** asking the core to publish the list (constitution I) (R-13). | **Read the source; file an engine issue** |
| P7 | **The engine commit** in the sidecar is read from `Cargo.lock` at build time, so it is the code that was built (R-12). | **From the lock file** |
| P8 | **The sidecar** is a strict line-based text format, version 1. Continuous values are written as their decimal and their bits, and the bits rule. It is lenient only for unknown or missing controls (D9) ([contracts/sidecar-v1.md](contracts/sidecar-v1.md)). | **As contracted** |
| P9 | **The reach test** prints `called/total`: **12/70** expected. The total excludes the engine's seven hidden test hooks, which no caller can reach, so 100 % at feature 006 is possible. The test fails if a method on the spec's list is not called, and does not assert the ratio until feature 006 (R-14). Constitution II's "81" becomes **74** by a PATCH amendment (**70** by 1.0.2 at implementation: eleven hooks, not seven). | **74; PATCH the constitution** |
| P10 | **Constitution I, honestly.** The channel mix and the monitor volume are arithmetic on the audio path that principle I does not name, and so are the stereo copy and the macOS conversion on playback. Recommended: a **PATCH amendment** that names the mix (input) and the monitor volume and stereo copy (playback only, never exported) as boundary conversions, each labelled on screen, plus a Complexity Tracking row until the amendment merges. | **PATCH amendment + Complexity row** |
| P11 | **Two engine gaps, filed as engine issues** (constitution I): (a) whether the gate runs at a configuration is not published, so the workbench quotes the engine's rule with its citation and pins it with a behavior test; (b) the one-pass has no drain figure of its own, so T borrows the player's drain, labelled, and a non-circular test proves every input sample comes out. Neither blocks 001. | **File both; quote and pin meanwhile** |
| P12 | **The tail is 0 at the taps before the gate**, where the engine's latency is 0 and nothing after the gate runs. Everywhere else it is latency plus drain (R-07). | **0 at the front taps** |
| P13 | **WAV files with fewer valid bits than their container** (for example 20 bits in 24) are refused in 001, like 20-bit FLAC: whether the decoder hands over the raw codes is unverified (R-01). | **Refuse** |

## Summary

A session opens one WAV, AIFF or FLAC file at an engine rate and converts the chosen channel by
the published rules. It edits every manifest control through a panel the engine classifies and
checks, and renders the input plus a tail on a fresh engine in a background thread. The original
and the render play through CoreAudio, with an A/B switch on the engine's own position map. The
export is the render, unaltered, plus a strict versioned sidecar that reproduces it bit for bit.
All logic is in `workbench_core` and tested without a window. The egui binary is a thin view.

## Technical Context

**Language/Version**: Rust stable, edition 2024 (local toolchain 1.99).

**Primary Dependencies** (test fixtures for AIFF and FLAC are written by hand-rolled writers in the test support module: no fixture dependency, no committed audio):
- `rr_dr60` (git, pinned `a06318c`)
- `eframe`/`egui` 0.37 (binary only)
- `symphonia` 0.6 (`wav`, `aiff`, `flac`, `pcm`)
- `hound` 3.5
- `sha2` 0.11
- `coreaudio-rs` 0.14 (macOS, binary only)
- `rfd` 0.17 (file dialogs, binary only)

**Storage**: Files only: the input, the exported WAV and the sidecar.

**Testing**: `cargo test` with `rr_dr60` built at opt-level 3 in the test profile (the block-size-1 oracle), the allocation test as its own single-threaded binary, and generated stimuli (tone, noise, a voiced burst with pauses) written to temporary files. There are no committed audio files.

**Target Platform**: macOS 26 on Apple silicon (the development Mac). Render and export are portable, and playback is macOS only (P2).

**Project Type**: Desktop application. The logic library is separate from a thin UI binary.

**Performance Goals**: one minute at 48 kHz renders in under 10 s in a release build (SC-004; expected about 0.5 s, R-09). Progress updates at ≥ 2 Hz, and a cancel takes effect within 1 s.

**Constraints**:
- Bit-exact with the engine (FR-007).
- No signal processing of the workbench's own (constitution I).
- A real-time-safe callback (FR-011).
- Memory within three f32 copies plus the engine's footprint (R-15).

**Scale/Scope**: one session per window. Files up to 60 minutes at 96 kHz.

## Constitution Check

| Principle | How the plan meets it | Status |
|---|---|---|
| I. The engine is the source of truth | No DSP on the render or the export. The mix (input), and the monitor volume, stereo copy and macOS conversion (playback only), are boundary arithmetic, labelled on screen; the mix and the monitor volume are not yet named by principle I (P10, Complexity Tracking). The panel comes from `controls()`. The engine classifies and checks every control (R-08). The denylist gap goes to the engine as an issue (P6). | Pass with P10 |
| II. Everything reachable | The reach test lands now (P9): 12/70 expected. The spec's list (15 methods) and every `Tap` are brought into use. | Pass |
| III. Faithful and reproducible | Exports are f32 IEEE, unaltered. The sidecar records the commit, seed, counter, tail, block, conversion and every value, and three digests. Each render runs on a fresh engine. No clock or OS randomness reaches the engine. | Pass |
| IV. Real-time audio safety | The callback is a pure function over atomics, with an allocation-counting test. Takes are dropped only after a stop (P3). Overloads are counted. | Pass |
| V. Tested without the window | Everything except the egui view and the CoreAudio binding is in `workbench_core` and covered by `cargo test`. There is a desk checklist (quickstart). | Pass |
| VI. Neutral communication | All strings are in `text`, checked against the engine's denylist. Boundary conversions and the workbench's own figures are labelled. "Splices" replaces "events" (P5). | Pass |

The post-design re-check found one item to track (principle I, P10). Two engine gaps go to the engine (P11).

## Project Structure

### Documentation (this feature)

```text
specs/001-offline-render-session/
├── spec.md, plan.md, research.md, data-model.md, quickstart.md
├── contracts/sidecar-v1.md, contracts/core-api.md
├── checklists/requirements.md
└── tasks.md            # /speckit-tasks
```

### Source Code (repository root)

```text
crates/workbench_core/
├── build.rs            # R-12: RR_DR60_COMMIT from Cargo.lock
├── src/lib.rs
├── src/input.rs        # decode, convert, mix, digests
├── src/panel.rs        # ControlRow, classes, checks (R-08)
├── src/render.rs       # fresh engine, tail, blocks, readings
├── src/ab.rs           # the engine's map and its inverse
├── src/export.rs       # the WAV + sidecar pair, atomic
├── src/sidecar.rs      # write, read, restore
├── src/monitor.rs      # the callback body and its atomics
├── src/text.rs         # every workbench string
└── tests/              # input, panel, render_oracle, ab, export_roundtrip, sidecar, reach, wording, monitor_alloc, render_speed
crates/workbench/
├── src/main.rs         # egui view: file bar, panel, transport, readings
└── src/playback.rs     # CoreAudio unit around monitor::fill (macOS)
```

**Structure Decision**: the two-crate workspace from the bootstrap. Everything testable is in
`workbench_core`. The binary holds only egui, file dialogs and the CoreAudio binding.

## Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| The channel mix and the monitor volume are arithmetic on the audio path, which constitution I 1.0.0 does not name (P10) | The spec's D2 and D6, both signed off | Dropping them would remove features the owner signed. Constitution 1.0.1 (on this branch) names them, so this row closes when the branch merges. |

## Traceability

Each requirement maps to a test in `crates/workbench_core/tests/` (T) or a desk-checklist step in
[quickstart.md](quickstart.md) (D).

| Requirement | Verified by |
|---|---|
| FR-001, D1, D3, US1 AS4 | T `input::refuses_formats_rates_and_widths` (MP3, A-law, 8-bit, 20-bit FLAC, 20-in-24 WAV, f64, 22.05 kHz) |
| FR-002 | T `input::conversion_rules` (edge codes, sweeps, every container at 24 bits against `samples_from_i24`, 2³¹ − 1 → 1.0, mix rounding) |
| FR-003, US1 AS3 | T `input::channels_and_mix` |
| FR-004, SC-003, US2 AS1 | T `panel::presents_every_control`, `panel::grows_with_the_engine` (extended entry list) |
| FR-005, US2 AS5 | T `panel::not_on_the_one_pass_and_read_only` |
| FR-006, US2 AS3, AS4 | T `panel::engine_checks_each_class` (refusals in the engine's words; the tap against the route) |
| FR-007, SC-001, US1 AS1, US2 AS2, AS6 | T `render_oracle` (seven rates × three stimuli × drop/mute; whole-call and block-size-1 oracles; the slot) |
| D7, P12 | T `tail_emits_the_input`, `tail_is_sufficient`, `tail_zero_at_front_taps` |
| FR-008, US1 AS5 | T `render::progress_and_cancel` (progress at ≥ 2 Hz on a 60 s input; cancel within 1 s; the previous render kept) |
| FR-009 | T `render::readings` (each reading; the gate predicate over every tap × enabled; not-emitted 0 cases) |
| FR-010, SC-005, US1 AS2 | T `ab::map_and_inverse`; D steps 3, 9 |
| FR-011 | T `monitor_alloc` (own binary), `monitor::fill_rules`; D step 11 |
| FR-012, US3 AS1 | T `export::roundtrip` (tag 0xFFFE + IEEE-float GUID), `export::failure_leaves_no_pair`, `export::refuses_oversize` |
| FR-013 | T `sidecar::values_roundtrip` (−0.0, every range's ends, every type) |
| FR-014, SC-002, US3 AS2 – AS5 | T `sidecar::reproduces` (seven rates), `sidecar::differences_listed`, `sidecar::refusals` |
| FR-015 | T `source_scan::no_clock_or_os_randomness` (`SystemTime`, `Instant` in engine inputs, `rand`, `getrandom`) |
| FR-016 | T `wording` (every `text` string against the engine's denylist + the supplementary list) |
| FR-017 | T `reach` (prints 12/70; asserts the listed methods) |
| US2 AS7 | T `panel::stale_and_reset` |
| SC-004 | T `render_speed` (release, ignored; quoted) |
| SC-006, edge cases | T `refusals_leave_session_unchanged`, `edge::empty_file`, `edge::metadata_and_many_channels`, `edge::non_finite_counted`, `export::overwrite_replaces_the_pair` |
| SC-007 | D the whole checklist |
