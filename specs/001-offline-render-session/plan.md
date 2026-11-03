# Implementation Plan: The offline render session

**Branch**: `001-offline-render-session` | **Date**: 2026-11-02 | **Spec**: [spec.md](spec.md) (signed off 2026-11-02)

**Input**: Feature specification from `specs/001-offline-render-session/spec.md`

## Decisions for sign-off

Read this section first. The rationale and alternatives for each row are in [research.md](research.md).

| # | Decision | Recommended |
|---|---|---|
| P1 | **Decoder.** `symphonia` 0.6, with only the WAV, AIFF and FLAC features, so a lossy file is "unsupported format" by construction. Its license is MPL-2.0, file-level copyleft, which is fine as a dependency of an MIT tool, and the README notes it (R-01). | **symphonia, three formats only** |
| P2 | **Playback on macOS only, through CoreAudio's default output unit**, with the take's rate on the unit's input scope. The operating system then does the only rate conversion, which the spec allows. A spike verified 6 – 96 kHz on the development Mac. On other platforms playback is unavailable in 001; render and export still work (R-04). | **CoreAudio, macOS only in 001** |
| P3 | **The playback callback** reads two takes in memory through atomics only. A new render or file **stops playback** before the takes are swapped, so the callback never drops memory. Dropouts are counted from the device's overload notifications (R-05). | **Stop on a new render** |
| P4 | **The A/B map** is the engine's `onepass_output_to_input`, inverted by binary search. A position inside a removed pause lands on the splice (R-06). | **Invert the engine's map** |
| P5 | **Wording restatement.** The engine's denylist contains the stem "event", so the screen says **"splices"** (drop mode) and **"muted regions"** (mute mode), never "gate events". FR-009 itself is unchanged (R-07). | **Splices / muted regions** |
| P6 | **The wording check** reads the engine's denylist from the pinned checkout's harness source. A dev-dependency on the harness would switch on the engine's test hooks in our test build. The fix belongs in the engine: **an engine issue** asking the core to publish the list (constitution I) (R-13). | **Read the source; file an engine issue** |
| P7 | **The engine commit** in the sidecar is read from `Cargo.lock` at build time, so it is the code that was built (R-12). | **From the lock file** |
| P8 | **The sidecar** is a strict line-based text format, version 1. Continuous values are written as their decimal and their bits, and the bits rule. It is lenient only for unknown or missing controls (D9) ([contracts/sidecar-v1.md](contracts/sidecar-v1.md)). | **As contracted** |
| P9 | **The reach test** prints `called/total` (13 of 81 expected), fails if a method on the spec's list is not called, and does not fail on the ratio until feature 006 (R-14). | **Print; assert the listed methods** |

## Summary

A session opens one WAV, AIFF or FLAC file at an engine rate and converts the chosen channel by
the published rules. It edits every manifest control through a panel the engine classifies and
checks, and renders the input plus a tail on a fresh engine in a background thread. The original
and the render play through CoreAudio, with an A/B switch on the engine's own position map. The
export is the render, unaltered, plus a strict versioned sidecar that reproduces it bit for bit.
All logic is in `workbench_core` and tested without a window. The egui binary is a thin view.

## Technical Context

**Language/Version**: Rust stable, edition 2024 (local toolchain 1.99).

**Primary Dependencies**:
- `rr_dr60` (git, pinned `a06318c`)
- `eframe`/`egui` 0.37 (binary only)
- `symphonia` 0.6 (`wav`, `aiff`, `flac`)
- `hound` 3.5
- `sha2` 0.11
- `coreaudio-rs` 0.14 (macOS, binary only)
- `rfd` 0.17 (file dialogs, binary only)

**Storage**: Files only: the input, the exported WAV and the sidecar.

**Testing**: `cargo test`, with generated stimuli (tone, noise, a voiced burst with pauses) written to temporary files. There are no committed audio files.

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
| I. The engine is the source of truth | The workbench does no DSP. The device's rate conversion and the monitor volume are boundary conversions on playback only, and labelled. The panel comes from `controls()`. The engine classifies and checks every control (R-08). The denylist gap goes to the engine as an issue (P6). | Pass |
| II. Everything reachable | The reach test lands now (P9). The spec's list of 13 methods and every `Tap` are brought into use. | Pass |
| III. Faithful and reproducible | Exports are f32 IEEE, unaltered. The sidecar records the commit, seed, counter, tail, block, conversion and every value, and three digests. Each render runs on a fresh engine. No clock or OS randomness reaches the engine. | Pass |
| IV. Real-time audio safety | The callback is a pure function over atomics, with an allocation-counting test. Takes are dropped only after a stop (P3). Overloads are counted. | Pass |
| V. Tested without the window | Everything except the egui view and the CoreAudio binding is in `workbench_core` and covered by `cargo test`. There is a desk checklist (quickstart). | Pass |
| VI. Neutral communication | All strings are in `text`, checked against the engine's denylist. Boundary conversions and the workbench's own figures are labelled. "Splices" replaces "events" (P5). | Pass |

The post-design re-check found no violations and no complexity to track.

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

None.
