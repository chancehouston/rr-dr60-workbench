# rr-dr60-workbench

A desktop power tool for exploring and tuning [rr-dr60-emu](https://github.com/chancehouston/rr-dr60-emu),
an open-source emulator of the audio signal chain of the Panasonic RR-DR60 digital voice recorder.

The workbench is a **companion tool for the library, not a product**. It puts every control,
stage and reading of the engine within reach of a window, so you can hear, see and tune what each
stage does: band-limiting, hiss, limiting, voice-activated gating, the speech codec and the output
chain. Every sound it plays or exports is the engine's own output. The workbench does no signal
processing of its own. Everything is **modeled** on the device's documents and on recorded
assumptions; nothing has been checked against a real unit (see the engine's README).

## Status

**Feature 001, the offline render session:**

- **Open** a WAV, AIFF or FLAC file at one of the engine's rates (6, 8, 16, 44.1, 48, 88.2 or 96 kHz). Pick a channel or the labelled mix.
- **Edit** every one of the engine's controls in a panel the engine itself describes and checks.
- **Render** through a fresh one-pass engine, with a tail so that the whole input comes out. The session shows the latency, splices, samples not emitted and peak.
- **Play** the original and the render with an instant A/B switch at the same moment of content, through the engine's own position map. Playback is macOS only for now.
- **Export** the render unaltered, as 32-bit float WAV, with a sidecar that reproduces it bit for bit when it is reopened with the same input.

**Feature 002, taps, scopes and meters:**

- **Meters**: every engine meter over time at 10 ms resolution, with each field's unit and kind, and figures the configuration does not reach greyed out.
- **Waveforms**: the original and the render, aligned through the engine's map, with removed pauses and muted regions marked.
- **Spectra**: Welch spectra at 5 Hz bins, calibrated to a full-scale sine, and their ratio.
- **Spectrogram**: of either take.
- **Tap browser**: renders the input at every tap.
- **Stages**: the engine's stage manifest, verbatim.
- **CSV exports**: each with the render's sidecar beside it.

**Feature 003, the live engine:**

- **Live**: the engine runs inside the output callback on a file.
- **Live controls**: changes take effect at once; the controls the one-pass refuses are disabled with the engine's own reason.
- **Scheduled changes**: listed changes land on their exact sample.
- **The timeline**: the change log, the gate's reports and the skips.
- **Reproduction**: every live take reproduces bit for bit from its version-2 sidecar.

**Feature 004, the record and play paths:** a persistent device with its memory. Record the input with record-side changes; list, erase, export and import items; play an item to a take with the playback controls and scheduled changes. A playback sidecar replays the take bit for bit.

**Feature 004b, live playback:** play an item live, on a copy of the device in the output callback. The playback controls and the stage switches change at once.

**Feature 005, processing records:** save the engine's processing record of a recording, with a record-input file (and the fed span as 24-bit samples when exact). Open a record, with every field and flag and a settings diff against the panel. Check it against an item, and re-render it from its input to the same digest.

**Feature 005b, comparisons and presets:** keep a render as B. The Compare tab shows a parameter diff, a null test (latency-aligned or at sample 0) and a spectral difference B ÷ A. B is a third A/B take. Presets save and load the panel's values.

**Feature 007, the microphone:** capture from the Mac's default input with the engine in the input callback. The take is reopened over its captured input, and monitoring is off by default.

**Feature 006, the coverage close-out:** every public engine method reachable (70/70, asserted). The Session tab checks the call forms, and so does each device item. The Tour tab steps through the stages, with "compare without this stage".

The plan of record ([docs/workbench-plan.md](docs/workbench-plan.md)) is complete.

## Build and run

Requires Rust stable (edition 2024). The engine is fetched from GitHub at a pinned commit.

```sh
cargo run --release -p rr_dr60_workbench
```

The device check (silent playback and live checks; `--capture` adds one second from the default
input, which needs the microphone permission):

```sh
cargo run -p rr_dr60_workbench --bin playback_check
cargo run -p rr_dr60_workbench --bin playback_check -- --capture
```

macOS is the first target. Rendering and export are portable; playback uses CoreAudio and is
macOS only in this version. egui runs on Windows and Linux too, untested so far.

Desk files for trying it out: `cargo run -p workbench_core --example desk_files -- out/desk`.

Dependencies: the decoder, [symphonia](https://github.com/pdeljanov/Symphonia), is MPL-2.0
(file-level copyleft, used unmodified as a library); everything else is MIT or Apache-2.0.

## Development

Spec-Kit drives the features (`.specify/`, `specs/`); the principles are in
[.specify/memory/constitution.md](.specify/memory/constitution.md). Before a merge:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

See [CONTRIBUTING.md](CONTRIBUTING.md). License: [MIT](LICENSE).
