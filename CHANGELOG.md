# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Feature 004b, live playback: a clone of the device engine plays an item in the output callback; the route, volume, battery, speed level and mode and the stage switches 10 – 13 change at once; the take keeps every change in scheduling order, uses 004's playback sidecar, and replays as a prefix when stopped early.

### Changed

- Play to take runs on a clone of the device, so a change that lands inside a take no longer carries into the device (spec 004b review B5).

### Added (004)

- Feature 004, the record and play paths (offline):
  - a persistent device engine, built from the panel, synced through `reconfigure` and emptied by Reset device;
  - recording into the memory, with record-side changes in the input clock and the engine's refusals and self-stop;
  - the memory view: items, status, erase, erase all, and export and import of the stored form;
  - play to a take, with the five playback values and playback-clock changes, showing the change log and the place reached;
  - the playback sidecar (`rr-dr60-workbench playback 1`), and a replay that checks the stored form's, the produced count's and the output's digests.
- Feature 003, the live engine:
  - the live core in the output callback (10 ms chunks through a FIFO, a bounded skip in drop mode);
  - live controls with the engine's reasons for the disabled ones;
  - a schedule list fed just in time;
  - the timeline and its CSV;
  - sidecar version 2 (`start`, `fed`, change lines), which reproduces a live take bit for bit.
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
