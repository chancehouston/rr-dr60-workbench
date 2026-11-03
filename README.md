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

Next, in the order of [docs/workbench-plan.md](docs/workbench-plan.md): taps and scopes, the live engine, the record and play paths, processing records and comparisons.

## Build and run

Requires Rust stable (edition 2024). The engine is fetched from GitHub at a pinned commit.

```sh
cargo run --release -p rr_dr60_workbench
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
