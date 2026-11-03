# rr-dr60-workbench Constitution

The workbench is the public desktop companion tool of the rr-dr60-emu engine
(https://github.com/chancehouston/rr-dr60-emu): a power tool for exploring and tuning the
engine, written in Rust with egui. It is not a product, and it replaces nothing. The iOS app and its
test bench (`ic-recorder-app`) continue alongside it. The plan of record is `docs/workbench-plan.md`.

## Core Principles

### I. The Engine Is the Source of Truth

- The workbench MUST NOT implement signal processing of its own on the audio path: every sound it
  plays or exports comes out of the `rr_dr60` engine. Meters, scopes, spectra and difference
  views MAY analyse the engine's output for display, and they MUST NOT feed back into it. File
  decoding and the output device's rate conversion at the boundary do not count as signal
  processing of the workbench's own. Each spec that uses them MUST state them.
- Every control the workbench shows MUST come from the engine's control manifest
  (`rr_dr60::controls()`). Every stage panel MUST come from the stage manifest (`rr_dr60::manifest()`).
  Names, kinds, timings, ranges, defaults, options, units, sentences, sources and assumptions are
  read from the engine at run time, never copied into the workbench. A control added to the engine
  appears in the workbench without UI work, or a test fails.
- A gap the workbench finds in the engine (a missing read, a wrong figure, an unclear rule) MUST
  become an issue or spec in the engine repo. The workbench MUST NOT work around it in a way that
  hides it.

Rationale: the workbench exists to evaluate the engine. Anything it added to the sound, or copied
from the engine, would mean evaluating the workbench instead.

### II. Everything Reachable

- By the end of the plan of record, every public method of `rr_dr60::Pipeline` (81 at engine 0.92.0)
  and every `Tap` MUST be reachable from the window. Each feature MUST state which methods it brings
  into use. A test MUST compare the engine's public `Pipeline` methods with the workbench's calls
  and print the ratio.
- The workbench serves exploration and tuning: density, raw figures and every option are welcome.
  Polish is optional and correctness is not.
- A figure the workbench shows MUST be either the engine's own figure, or a documented analysis of
  the engine's output with its unit and method, so that a note or an issue can quote it.

Rationale: an engine feature that cannot be reached cannot be evaluated, and coverage that is not
measured drifts.

### III. Faithful and Reproducible

- What the workbench exports is the engine's output sample for sample: no normalization, dither,
  fade or trimming unless the export names it. The default export is 32-bit float at the host rate.
- Every render MUST be reproducible. The workbench keeps the engine version (pinned commit), the
  settings, the seed, the host rate and the control schedule with every render, and writes them
  beside every export. Where the engine's processing record applies (spec 020), the workbench uses it.
- The same input, settings, seed and schedule give bit-identical output on every run. The workbench
  MUST NOT use wall-clock time or OS randomness in anything it passes to the engine.

Rationale: a finding from the workbench is worth something only if someone else can reproduce it.

### IV. Real-Time Audio Safety

- The audio callback MUST be real-time safe: no allocation, locks, I/O or unbounded work. Buffers
  and engines MUST be created before audio starts. Controls MUST cross to the audio thread through
  lock-free queues or atomics, or through the engine's own schedule (`schedule_change`).
- The UI thread MUST NOT block the audio thread. Anything the audio thread drops or defers MUST be
  counted and shown.

Rationale: a glitch the workbench causes would be read as the engine's.

### V. Tested Without the Window

- The logic (sessions, rendering, scheduling, analysis, file formats, the manifest-driven control
  model) lives in a library crate that has no UI dependency, and its tests run under
  `cargo test` with no window and no audio device. The egui layer stays thin.
- Every feature MUST have tests that drive it through that library: the acceptance scenarios, the
  engine's refusals surfaced as named errors, and bit-exact comparisons with direct engine calls
  where the workbench claims to pass audio through unaltered.
- Every feature MUST carry a desk checklist that the owner runs on the Mac before merging.

Rationale: tests prove the wiring, and only the owner's ears and eyes prove the tool.

### VI. Neutral, Honest Communication

- Every string follows the engine's claims policy: neutral and technical, describing what the
  signal chain does. Use "modeled on", never "accurate", and never make a claim about paranormal
  phenomena in either direction. Readings say how they were taken.
- Additions that the original recorder does not have (emulator options, the workbench's own views)
  are labelled as such.

Rationale: the repository is public, and its screenshots and notes may be quoted.

## Platform Standards

- Rust stable, edition 2024. egui/eframe for the window. macOS first; other desktop platforms are
  welcome where they cost nothing.
- The engine is a git dependency pinned to a full commit or a tag of rr-dr60-emu. Moving the pin is
  its own commit, and the CHANGELOG records it.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings` and
  `cargo test --all-features` are clean before any merge. Use Conventional Commits.
- MIT license. Do not commit audio files: fixtures are generated by tests.

## Development Workflow

- Every feature is a Spec-Kit feature on its own `NNN-name` branch:
  1. `/speckit-specify`, with a Decisions-for-sign-off table.
  2. The draft reviews, applied at their defaults: `speckit-coach` always, and `hw-emulation-reviewer`
     when the feature shows readings or analysis.
  3. `/speckit-clarify`.
  4. The owner's spec sign-off.
  5. `/speckit-plan` and its reviews.
  6. The owner's plan sign-off.
  7. `/speckit-tasks`.
  8. Test-first implementation.
  9. The post-implementation reviews.
  10. The gate.
  11. A pull request.
  12. `/speckit-converge`.
  13. The owner's desk checklist on the Mac.
  14. The owner merges.
- Agents stop between the owner's sign-offs only on a failed check or a new decision.

## Governance

- This constitution supersedes other practices in this repository. The engine repo's constitution
  governs the engine.
- Amendments are made by pull request. Each MUST state its rationale and bump the version under
  semantic versioning: MAJOR for removed or redefined principles, MINOR for added or materially
  expanded principles or sections, and PATCH for clarifications.
- Every spec, plan and pull request MUST be checked for compliance (the plan's Constitution Check).

**Version**: 1.0.0 | **Ratified**: 2026-11-02 | **Last Amended**: 2026-11-02
