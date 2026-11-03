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

Bootstrap. The window lists the engine's control manifest grouped by stage. Features arrive in the
order of [docs/workbench-plan.md](docs/workbench-plan.md): offline render sessions, taps and scopes,
the live engine, the record and play paths, processing records and comparisons.

## Build and run

Requires Rust stable (edition 2024). The engine is fetched from GitHub at a pinned commit.

```sh
cargo run --release -p rr_dr60_workbench
```

macOS is the first target. egui runs on Windows and Linux too, untested so far.

## Development

Spec-Kit drives the features (`.specify/`, `specs/`); the principles are in
[.specify/memory/constitution.md](.specify/memory/constitution.md). Before a merge:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

See [CONTRIBUTING.md](CONTRIBUTING.md). License: [MIT](LICENSE).
