# Contributing

Thanks for your interest. The workbench is a companion tool for
[rr-dr60-emu](https://github.com/chancehouston/rr-dr60-emu); engine behavior is changed there, not here.

- **Issues:** bugs in the workbench, or ideas for views and readings. If the workbench shows an engine
  gap (a missing read, a surprising figure), please open the issue in the engine repo.
- **Pull requests:** features go through Spec-Kit (`specs/NNN-name/`) and the principles in
  `.specify/memory/constitution.md`. Small fixes are welcome directly. Use Conventional Commits, and
  run `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test` before opening one.
- **Wording:** neutral and technical. Describe what the signal chain does. Use "modeled on", not
  "accurate", and make no claims about paranormal phenomena either way.
- **Audio:** never commit recordings. Tests generate their own stimuli.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).
