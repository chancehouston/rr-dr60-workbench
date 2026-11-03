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
- **Moving the engine pin:** only to a merged engine commit or tag, in its own commit, with a
  CHANGELOG line.
- Conventional Commits. Never commit audio.

## Current status

- 2026-11-02: Bootstrapped (constitution 1.0.0, the plan of record, the read-only control list,
  CI on macOS). Engine pin `a06318c` (0.92.0). Next: feature 001, the offline render session.
