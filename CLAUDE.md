# CLAUDE.md — rr-dr60-workbench

Guidance for Claude Code working in this repository. Keep it current.

- **What this is:** the public desktop companion tool (Rust, egui/eframe) for exploring and tuning
  the rr-dr60-emu engine. It is not a product and it replaces nothing: the iOS app and its bench
  (`~/Code/ic-recorder-app`) continue alongside it. The engine is a git dependency pinned to a full
  commit. Its sources, registers and records are in the sibling checkout `~/Code/DR60Recorder`.
  Engine changes go through Spec-Kit in the engine repo, never here.
- **Principles:** `.specify/memory/constitution.md` (it wins conflicts). In short: the engine is the
  source of truth (no DSP of our own on the audio path; every control and stage panel generated
  from the engine's manifests at run time); everything reachable (every `Pipeline` method and `Tap`
  by the end of the plan, measured by a test); faithful and reproducible exports (the engine's
  samples unaltered, with a settings, seed and engine sidecar); a real-time-safe audio callback; the
  logic in `workbench_core` with no UI dependency, tested without a window.
- **Claims policy:** the engine's. Neutral and technical; "modeled on", never "accurate"; no claim
  about paranormal phenomena in either direction. The repo is public: never put the owner's private
  views in it.
- **Layout:** `crates/workbench_core` (logic, tests), `crates/workbench` (the egui binary
  `rr_dr60_workbench`, thin). egui 0.37's app trait is `fn ui(&mut self, ui: &mut egui::Ui, …)`.
  Panels take a `Ui` (`egui::Panel::top(..).show(ui, ..)`, `CentralPanel::default().show(ui, ..)`).
  The engine's enums are `#[non_exhaustive]`, so matches need a wildcard arm.
- **Before declaring work done:** `cargo fmt --all -- --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`.
- **Process:** Spec-Kit; the plan of record is `docs/workbench-plan.md`. Each feature goes on its
  own `NNN-name` branch through the constitution's Development Workflow. The owner signs off at the
  spec, the plan and the merge, and runs each feature's desk checklist on the Mac. Reviews:
  `speckit-coach` always, and `hw-emulation-reviewer` when a feature shows readings
  (`.claude/agents/`).
- **Autopilot (owner, 2026-11-02):** the owner approves the agent's decisions for this workbench and lets the agent merge its own workbench PRs once CI is green and the post-implementation reviews are applied, so work keeps moving. Desk checklists are run by the owner afterwards; findings become follow-up fixes. Engine-repo changes still go through the engine's own sign-offs.
- **Moving the engine pin:** only to a merged engine commit or tag, in its own commit, with a
  CHANGELOG line.
- Conventional Commits. Never commit audio.

## Current status

- 2026-11-02: Bootstrapped (the plan of record, CI on macOS). Engine pin `a06318c` (0.92.0).
- 2026-11-02: **Feature 001 (the offline render session) is merged** (PR #2, merge commit 8afbced, CI green; merged by the agent under the autopilot rule, the owner's desk checklist pending): 54 tests after the post-implementation reviews and convergence (T038, streaming decode). Before that: 45 tests, reach 12/70, one minute at 48 kHz renders in 0.54 s, constitution 1.0.2. The figures are in `specs/001-offline-render-session/records.md`. Engine issues #72 – #74 were filed. The core `Session` holds the rules of opening a file, attaching a render and restoring a sidecar. Next: feature 002 (taps, scopes and meters) on `002-taps-scopes-meters`.
- 2026-11-02: **Feature 002 (taps, scopes and meters) is merged** (PR #3, merge commit aed08c2, CI green; under the autopilot rule, the owner's desk checklist pending): 81 tests, reach 15/70, figures in `specs/002-taps-scopes-meters/records.md`, engine issue #75 (the meter table). Next: feature 003 (the live engine) on `003-live-engine`.
- 2026-11-02: **Feature 003 is implemented** on its branch: 91 tests, reach 20/70, live takes reproduce bit for bit; the microphone deferred to 007. Figures in `specs/003-live-engine/records.md`.

## Lessons (003)

- One chunk routine shared by the offline render and the live core makes reproduction true by construction: the engine's output does not depend on the partition, and it logs a change's actual landing position.
- A just-in-time schedule needs a cursor per clock: the input-clock and output-clock subsequences of the log are each in landing order, and they interleave.
- Size the live queues for the worst callback, not the average: at 6 kHz a 6000-frame callback runs up to 400 ten-millisecond chunks under the 4× budget.
- The tap-and-route refusal shows only for the *other* route: a dry run must try every option, not the current value.
- rustfmt reflows long lines, so a script's exact-text patch silently misses. Assert each anchor, or use the Edit tool after reading the formatted text.

## Lessons (002)

- Bins of 5 Hz (N = rate ÷ 5) put 1 kHz on bin 200 at every rate. A power-of-two size reads −0.63 dB from scalloping.
- `Meters` is `#[non_exhaustive]`: read its field names from `Debug`. `rest_meters()` carries the battery factor, and `Meters::default()` carries 0.0.
- A reach rule is pinned both ways: an unreached field must stay at rest in every row, and a reached one must move, with the one measured exception stated (the earphone output clamp).
- The reach scan counts a name the workbench also defines (`meters`, `settings`) only as `Pipeline::name(`. Call such engine methods in that form.
- `egui::Id::new` is deprecated in egui 0.37, and `link_axis` needs an `Id`. Drop the link rather than reach for a deprecated call.

## Lessons (001)

- symphonia 0.6: route samples by the codec's `bits_per_sample`. FLAC delivers 16 and 24 bits shifted into `i32`, and WAV and AIFF at their own width. A FLAC fixture with fixed blocking needs STREAMINFO min block = max block.
- hound writes WAVE_FORMAT_EXTENSIBLE for f32 (tag 0xFFFE, a 68-byte header). The data limit is `u32::MAX − 60`.
- The engine's speech coder's idle output is quiet, not zero (about 1e-3 at its start). A tail bound with the coder on is the coder's own output on silence, not −100 dBFS.
- Outputs before the latency clamp to input 0 in the engine's one-pass map, so round-trip properties hold only for unclamped outputs.
- A reach scan by name confuses the workbench's own methods (`export`, `play`, `settings`) with the engine's. A name the workbench defines counts only as `Pipeline::name(`.
- CoreAudio's default output unit accepts 6 – 96 kHz on its input scope and converts. `--bin playback_check` checks it silently.
- egui 0.37: `App::ui(&mut self, ui, frame)`, `egui::Panel::{top,left}(..).show(ui, ..)`. Defer `self` calls out of `ui.horizontal` closures, because of borrows.
