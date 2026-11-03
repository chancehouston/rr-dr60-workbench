# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Feature 002, taps, scopes and meters:
  - the meter timeline at 10 ms calls (the render's block is now rate ÷ 100, and the output is unchanged);
  - the reach rules, pinned by behavior;
  - waveform strips on the engine's map;
  - Welch spectra at 5 Hz bins and their ratio;
  - the spectrogram;
  - the tap browser;
  - the stage panels;
  - CSV exports with sidecars.
- Feature 001, the offline render session:
  - opening WAV, AIFF and FLAC files at the engine's seven rates, with the published conversion rules and a channel or the labelled mix;
  - a panel generated from the engine's control manifest and checked by the engine;
  - renders on a fresh engine with a tail, and the readings (latency, splices or muted regions, not emitted, paused at end, peak);
  - A/B playback through CoreAudio on the engine's one-pass map;
  - the 32-bit float export with a versioned sidecar that reproduces it;
  - the reach, wording and source-scan tests.
- Bootstrap: the workspace (`workbench_core`, `rr_dr60_workbench`), the engine pinned at rr-dr60-emu
  `a06318c` (0.92.0), a read-only window listing the control manifest by stage, Spec-Kit with the
  constitution 1.0.0 (1.0.2 with 001: the boundary conversions named; the reach count 70), the plan of record, community files and CI.
