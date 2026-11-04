# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed (#27)

- The engine pin moves to `7420389` (rr-dr60-emu 0.95.0, the merge of engine spec 028, the one-pass output map; engine issue #82).
- **The A/B map and the splice marks follow the engine's one-pass output map.** A gate report is now heard at its position plus the coder's delay **plus the playback half's latency** (the engine's `one_pass_splice_output_position`; the earlier rule left out the playback half's latency: L_post outputs per splice too early in drop mode, 40 at 8 kHz and 241 at 48 kHz, exact at 6 kHz). `render::output_to_input` equals the engine's `one_pass_input_at_output` centre on every output (`tests/ab.rs::the_map_is_the_engines`); each render carries `splice_shift_samples`. The comparison's report alignment and its prefix (`compare::prefix_end`: B's reports map to A's as o + H_B − H_A − s, a paused end placed H before the take's end) use the same shift.
- **The timeline CSV is form 2**: a gate report's row adds `heard`, `input_centre`, `input_first` and `input_last` (the engine's alignment where the report is heard, from one engine per export), and the comment line says the report positions are on the clock before the speech coder. Reach 77/77.

### Changed

- The engine pin moves to `ce081b8` (rr-dr60-emu, the fix of engine issue #83: the meters' reach follows the requested route before it lands), and `render::meter_reach_for` no longer processes a silent sample.

### Changed (010)

- The engine pin moves to `ad1ba8d` (rr-dr60-emu 0.94.0, the merge of engine spec 027, the public re-render).
- **The re-render is the engine's session** (`rr_dr60::rerender`): the workbench's copy of the recipe (the filler, the slot restore, the schedule refill, the stop rule) is gone. The result line shows the engine's verdict (Reproduces, Differs, Rebuilt) and the rows that landed. An input that ends before the record's stop is now refused, with the samples missing, instead of reported as a difference.
- The device records with the plain `record` call when no record-side change is scheduled or pending (the same stored form).

### Added (010)

- The memory view's **Fill the memory** (leave N s; `fill_to_remaining`) and **Reset device at counter** (`new_at_counter`), both labelled additions of the emulator, and the memory's encoding on the status line.

### Changed (008)

- The engine pin moves to `3890bfe` (rr-dr60-emu 0.93.0, the merge of engine spec 026, the host reads).
- The engine's reads replace the workbench's mirrors: whether the gate runs (`Settings::one_pass_vas_runs`), the reports' latency, the wording lists (`rr_dr60::wording`), the meter table with reach and lag (`rr_dr60::meters`, `Pipeline::meter_reached` / `meter_lag_samples`), and the stage labels.
- **The tail is the engine's one-pass drain** (`Pipeline::one_pass_drain_samples`). It was the player's drain, and 0 at the front taps, so renders are longer (7910 host samples of tail at 48 kHz by default) and the front taps get a tail.
- **The A/B map, the marks, the meters' placement and the spectrum's spans use the gate reports' clock.** With the coder on in drop mode, a splice was counted the coder's delay early; it now lands where it reaches the output (the 005b follow-up).
- The readings show the speech coder's delay and frame.

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
