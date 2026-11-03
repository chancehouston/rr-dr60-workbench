# Research: the offline render session

Phase 0 of the plan. Each item gives the decision, the rationale and the alternatives. Engine
claims are read from the pinned checkout (`~/Code/DR60Recorder` at `a06318c`, now `71f25f1` on
`main` with only docs changed). Every claim in the "verified" items was run or read on 2026-11-02.

## R-01 Decoding: symphonia 0.6 (WAV, AIFF, FLAC)

- **Decision**: decode with `symphonia` 0.6, using only the `wav`, `aiff` and `flac` features and
  the PCM codec. The decoder's buffer keeps the file's own sample type (`S16`, `S24`, `S32`, `F32`;
  `U8`/`S8`/`F64` are refused by D1). The session reads the container's bits per sample and
  refuses 20-bit FLAC by that field.
- **Rationale**: one decoder for the three containers. It keeps the integer samples as integers, so
  FR-002's conversion runs on the exact values. MPL-2.0 is file-level copyleft and fine as a
  dependency of an MIT tool (the license is recorded in the README's dependency note).
- **Alternatives**: `hound` + `claxon` + an AIFF crate (three decoders and three sets of quirks);
  symphonia's default features (they would also open MP3 and AAC, which D1 refuses: off at the
  feature level, so a lossy file is "unsupported format" by construction).

## R-02 The input conversion (FR-002)

- **Decision**: 16-bit: `x as f32 / 32768.0` (exact). 24-bit: the engine's
  `rr_dr60::input::samples_from_i24` itself, called, not copied. 32-bit integer:
  `(x as f64 / 2147483648.0) as f32`, one round to nearest even, labelled. Float: unchanged. Mix:
  integers summed in `i64` and floats in `f64`, in channel order, divided by the channel count in
  `f64`, rounded once to `f32`. The conversion rule carries a version (`conversion 1`) in the
  sidecar.
- **Rationale**: the spec's rule, x ÷ 2^(b−1); the engine's published feed point for 24 bits (spec
  020). A container wider than the valid bits (WAVE_FORMAT_EXTENSIBLE) is scaled by the container
  width, because the decoder delivers container-width samples.
- **Test**: each rule against hand values at the ends (−2^(b−1), 2^(b−1) − 1, 0, ±1 LSB), and 24
  bits against `samples_from_i24` on every value of a 2^16-step sweep.

## R-03 Writing the export: hound, IEEE float

- **Decision**: `hound` 3.5 writes `WAVE_FORMAT_IEEE_FLOAT` (format 3), mono, 32-bit, at the input
  rate. The pair is written as `name.wav.partial` and `name.sidecar.txt.partial`, both flushed and
  closed, then renamed (the sidecar last). A failure removes both partial files. An export whose
  data would exceed `u32::MAX − 44` bytes is refused before writing.
- **Rationale**: hound writes exactly the f32 bytes. The rename order means a reader never sees a
  sidecar without its WAV. The read-back test uses hound too, plus a byte-level check of the
  format tag.

## R-04 Playback: CoreAudio's default output unit (verified)

- **Decision**: on macOS, playback uses `coreaudio-rs` 0.14's `DefaultOutput` audio unit, with the
  take's rate set on the unit's input scope as mono non-interleaved f32. CoreAudio's own converter
  takes it to the device. On other platforms, playback is unavailable in 001 with a message, and
  rendering and export still work.
- **Verified** with a spike on 2026-11-02 (the development Mac): `set_stream_format` succeeds at 6,
  8, 16, 44.1 and 96 kHz, and the unit pulls 3066 / 3994 / 7988 / 22 016 / 47 926 frames in 0.5 s,
  which is the take's own rate each time.
- **Rationale**: the spec allows only the device's own rate conversion (D6, FR-010), and this is
  the operating system's. `cpal` opens the device at a configuration the device supports, and
  macOS output devices do not offer 6, 8 or 16 kHz, so cpal would need a resampler of the
  workbench's own. `rodio` resamples in its own code.
- **Alternatives**: AVAudioEngine through objc2 (more binding work for the same converter);
  AudioQueue (deprecated paths).

## R-05 The real-time contract of the callback (FR-011)

- **Decision**: before the unit starts, the callback owns an `Arc` of both takes (`Box<[f32]>`
  each) and an `Arc<Shared>` of atomics: `cursor` (`AtomicU64`: bit 63 the take, bits 0–62 the
  position), `request` (`AtomicU64`, `u64::MAX` = none; the UI stores a mapped cursor and the
  callback swaps it in at the top of the buffer), `volume_bits` (`AtomicU32`, f32 bits),
  `playing` (`AtomicBool`) and `overloads` (`AtomicU64`). The callback copies samples times the
  monitor volume and writes zeros past a take's end. It allocates nothing, locks nothing and drops
  nothing (the takes are dropped on the UI thread after `stop`). A new render or a new file stops
  playback first.
- **Dropouts**: a property listener on the default device's `kAudioDeviceProcessorOverload` counts
  overloads into `overloads`, and the session shows the count. The source is in memory, so it
  cannot underrun.
- **Test**: the callback body is a plain function over slices and the shared atomics in
  `workbench_core::monitor`. It is tested without a device under a counting global allocator
  (zero allocations over 10⁵ calls) and for the request swap, the clamp and the zeros.

## R-06 The A/B position map (D6, FR-010)

- **Decision**: render → original: `rr_dr60::timeline::onepass_output_to_input(o, latency,
  events, drop)` with the render's own reports. Original → render: the smallest render position o
  whose mapped input is ≥ i, found by binary search over the map, which is monotone
  non-decreasing. A position inside a removed pause lands on the splice. "Same seconds" is
  o = i. Both are clamped to the target's end. The map is in `workbench_core::ab`.
- **Rationale**: the engine publishes the map (spec 020). Inverting it by search keeps the engine's
  rule as the one definition.
- **Test**: for renders in drop and mute mode at every rate, `map(inverse(i)) ≥ i` and
  `inverse(map(o)) ≤ o` for every position, and exact values on a hand-built report list.

## R-07 The tail and the readings (D7, FR-009)

- **Decision**: T = `latency_samples()` + `drain_samples()` of the fresh engine, in host samples.
  The readings are the engine's `BlockInfo` fields summed per call (`events`, `produced`), the last
  call's `paused`, and `max_events(block)` to size the report buffer. "Gate not reached" holds
  when the tap is one of the taps before the gate (`AfterMicCapsule`, `AfterMicAmp`, `AfterInsert`,
  `AfterAgc`). The workbench reads that from the engine's `max_events` being 0 for a non-zero
  block, not from a list. The engine documents 0 only at the after-AGC tap (`pipeline.rs`
  `max_events`). A test over every tap checks that `max_events(4096) == 0` holds exactly at the taps
  before the gate. If a front tap reads non-zero, that is an engine issue (constitution I) and the
  session shows the count it reads. The peak is `max |x|` over the render in `f32`, labelled "sample peak
  (workbench analysis)".
- **Restatement of the spec's label (wording)**: the engine's wording denylist contains the stem
  `event`, so the screen says **"splices"** in drop mode and **"muted regions"** in mute mode,
  never "gate events". FR-009's content is unchanged. The spec's sentence keeps its term for the
  requirement only.

## R-08 Classifying and checking the controls (FR-006)

- **Decision**: a control's class comes from the engine. If
  `control_map::settings_value(&Settings::new(rate), i)` is `Ok`, the control is a settings field,
  written with `set_settings_value`. Otherwise (`NotASetting`), it is written on an engine with
  `Pipeline::set_control_value`, which handles the plain live controls and the component slot and
  parameters (spec 024's `set_component_control`). One kept **check engine**
  (`Pipeline::new(Settings::new(rate))`) checks each live or component value: set, read back with
  `control_value`, then restore. Settings are checked by `Settings::validate` on the defaults plus
  the one value, then on the whole. The tap against the route is checked by a zero-length
  `process(&[], &mut [])` on the check engine with the session's route, tap and settings, because
  the engine raises `TapNotOnRoute` before the length matters and changes no state.
- **Rationale**: no list of classes in the workbench. The zero-length call is the engine's own
  refusal path (`pipeline.rs` `process`: `refuse_if_active`, then `tap_on_route`).

## R-09 The render (FR-007) and the oracle

- **Decision**: a render thread builds `Pipeline::new(settings)` (`recording_counter()` asserted
  0), applies the non-settings values in the spec's order (the slot, its parameters 0 – 3, the
  route, the volume, the battery voltage), then calls `process_with_events` in blocks of 4096 over
  the input followed by T zeros, appending `output[..produced]`. Cancel is an `AtomicBool` checked
  per block, and progress is an `AtomicU64` of input samples consumed. The block size is recorded
  in the sidecar. The oracle helper in the tests repeats the calls with one whole-input call and
  at block size 1.
- **Speed**: the engine's records give 1.23 µs per device sample at 48 kHz with the codec on, about
  0.5 s per minute of audio in a release build. SC-004's 10 s is quoted from a release run on the
  development Mac.

## R-10 The sidecar format

- **Decision**: a line-based UTF-8 text file. The first line is `rr-dr60-workbench sidecar 1`,
  followed by `key value` lines. Control lines are `control <identifier> <value>`, where a
  continuous value is the shortest round-trip decimal and its bits (`0.25 0x3e800000`; the bits
  rule, and a decimal that disagrees with them is malformed), a switch is `on`/`off`, an integer
  is decimal and a choice is its engine value. Duplicate keys are malformed. An unknown key
  outside the control lines makes the file malformed. An unknown identifier on a control line is
  skipped and listed. The grammar is in `contracts/sidecar-v1.md`.
- **Rationale**: readable in an issue, exact for f32, strict where the version must change, and
  lenient only where the engine grows (D9).
- **Alternatives**: TOML or JSON (a float formatter's choice of digits would decide the
  round-trip); the engine's processing record (record path only, D8).

## R-11 Digests

- **Decision**: SHA-256 (`sha2` 0.11) over the raw file's bytes, over the converted input's f32
  little-endian bytes, and over the render's f32 little-endian bytes, written as lowercase hex.
- **Rationale**: the spec asks for cryptographic strength. These are the same bytes that the tests
  compare.

## R-12 The engine commit at run time

- **Decision**: `workbench_core`'s `build.rs` reads `Cargo.lock` from the workspace root, finds the
  `rr_dr60` package's `source = "git+…?rev=…#<sha>"` and exports the 40-character sha as
  `RR_DR60_COMMIT`. If it is missing, the build fails.
- **Rationale**: the pin is in the lock file, so the commit in the sidecar is the code that was
  built.

## R-13 Wording (FR-016)

- **Decision**: every string the workbench writes lives in `workbench_core::text`, as constants
  and format functions. A test reads the engine harness's `DENYLIST_STEMS`, `DENYLIST_PHRASES` and
  suffix set from the pinned checkout's `crates/rr_dr60_harness/src/manifest_checks.rs` (located
  with `cargo metadata`), builds the same matcher and checks every string. The engine's own strings
  are not re-checked: the engine's tests check them.
- **Rationale**: a dev-dependency on `rr_dr60_harness` would turn on the engine's `__test-hooks`
  feature in the test build through feature unification. The renders under test would then come
  from a different build of the engine.
- **Engine gap (constitution I)**: the denylist is not published by the engine's core. An engine
  issue proposes `rr_dr60::wording::{STEMS, PHRASES, SUFFIXES}`. Until then, the source read is the
  stated workaround, and a parse failure fails the test.

## R-14 The reach test (FR-017)

- **Decision**: a test runs `cargo metadata`, reads the pinned `rr_dr60`'s `pipeline.rs`,
  `transport.rs` and `control_map.rs`, and collects the names of `pub fn` items inside
  `impl Pipeline` blocks. It then scans `crates/*/src` for `.name(` calls and prints `called / total`
  and the list of uncalled methods. The test asserts that the spec's "Engine reach" list is called
  and fails if any listed method is not. It does not fail on the ratio before feature 006.
- **Expected** at 001: 13 of 81.

## R-15 Long files and memory

- **Decision**: the session holds the selected channel (`Box<[f32]>`), the current render and,
  while rendering, the new one, which is the three-copy bound. Decoding streams packet by packet
  into the selected channel only, so another channel means decoding again. The session shows the
  three sizes plus `footprint_bytes()`.
