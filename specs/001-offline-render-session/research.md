# Research: the offline render session

This is Phase 0 of the plan. Each item gives the decision, the rationale and the alternatives.
Engine claims were read from the pinned checkout (`~/Code/DR60Recorder` at `a06318c`; `main` is
now `71f25f1`, with docs changes only), and library claims from the crates' sources, on 2026-11-02.
The plan reviews (coach 1 – 19, hardware B1 – B11) are applied. **"Verified"** marks a claim that
was read or run.

## R-01 Decoding: symphonia 0.6 (WAV, AIFF, FLAC)

- **Decision**: decode with `symphonia` 0.6, with only its `wav`, `aiff`, `flac` and `pcm`
  features, so MP3 and AAC are "unsupported format" by construction. Two kinds of file are refused
  by codec id, because `pcm` would otherwise decode them to S16: A-law and µ-law WAV.
- **Routing by the codec's `bits_per_sample`, not by the buffer type (verified, B2)**: the FLAC
  decoder delivers an `AudioBuffer<i32>` with every sample shifted left by 32 − b. A 16- or 24-bit
  FLAC therefore arrives as S32. For an S32 buffer with b < 32, the session checks that the low
  32 − b bits are zero, shifts right arithmetically, and then applies the 16- or 24-bit rule (R-02).
  WAV and AIFF arrive at their own width, and AIFF's big-endian 24-bit samples are read by
  symphonia's `read_be_i24`. Refused: b = 8, 20-bit FLAC, f64, and **WAVE_FORMAT_EXTENSIBLE with
  fewer valid bits than the container** (B8: whether symphonia delivers the raw container codes is
  unverified, so 001 refuses such files; plan P13). AIFF-C `sowt` (little-endian) is accepted if
  symphonia opens it, and the tests cover it.
- **Rationale**: one decoder for three containers that keeps integers as integers. MPL-2.0 is a
  file-level copyleft, which is fine as a dependency of an MIT tool, and the README notes it.
- **Fixtures**: there is no FLAC or AIFF writer among the dependencies. The test support module
  hand-writes AIFF (and AIFF-C `sowt`) files, and writes FLAC with verbatim subframes and a
  correct STREAMINFO and CRC. The MP3 and A-law fixtures are a few raw header bytes. Every
  fixture is generated at test time, and none is committed.

## R-02 The input conversion (FR-002)

- **Decision**: b is the decoded width after R-01's routing.
  - **16 bits**: `x as f32 / 32768.0`, which is exact.
  - **24 bits**: the engine's own `rr_dr60::input::samples_from_i24`, called rather than copied.
    It computes `(s as f32) * 2⁻²³` (`input.rs:11-15`), exact on every 24-bit code.
  - **32-bit integers**: `(x as f64 / 2³¹) as f32`, rounded once to nearest even and labelled "32-bit
    rounding". 2³¹ − 1 becomes exactly 1.0, which the label's test checks.
  - **Float**: unchanged.
  - **Mix of integer files**: summed exactly in `i64`, divided in `f64`, cast to `f32`. This equals
    one correct rounding for fewer than 2²⁷ channels (B9): a quotient that is not a midpoint lies at
    least 2⁻²⁵/n from any f32 midpoint, which is far more than f64's 2⁻⁵³.
  - **Mix of float files**: summed in `f64` in channel order, divided in `f64`, rounded to `f32`.
    This is deterministic, and is described that way rather than as "one rounding" (dated
    restatement of FR-002, spec Clarifications).
  - **Conversion rule**: `conversion 1` in the sidecar.
- **Tests**: every rule on the edge codes (−2^(b−1), 2^(b−1) − 1, 0, ±1 LSB) and on a 2¹⁶-step
  sweep. At 24 bits, every container (WAV, AIFF, AIFF-C `sowt`, FLAC) must equal `samples_from_i24`.

## R-03 Writing the export: hound, and the pair (verified, B3; coach 6)

- **Format**: `hound` 3.5.1 writes **WAVE_FORMAT_EXTENSIBLE** (tag 0xFFFE with the IEEE-float
  subformat GUID and a 68-byte header) for any width above 16 bits (`write.rs:234`). The f32 bits
  pass through unchanged (`write.rs:117-118`). The byte-level test asserts tag 0xFFFE and the
  IEEE-float GUID. An export is refused when its data exceeds `u32::MAX − 60` bytes (the RIFF size
  is 60 + data).
- **The pair**: write `stem.wav.partial` and `stem.sidecar.txt.partial`, then flush and close both.
  Remove any old `stem.sidecar.txt`, rename the WAV, then rename the sidecar. If the second rename
  fails, remove the new WAV. File operations go through an `ExportFs` trait, so the tests inject a
  failure at every step (`export_failure_leaves_no_pair`).

## R-04 Playback: CoreAudio's default output unit (spike verified; B7 and coach 11 applied)

- **Decision**: on macOS, playback uses `coreaudio-rs` 0.14's `DefaultOutput` unit.
  - The client format on the unit's input scope is **2-channel non-interleaved f32 at the take's
    rate**. `fill` writes the same samples to both channels: an exact copy, so the mono-to-stereo
    behavior is not left to the converter.
  - `kAudioUnitProperty_SampleRateConverterComplexity` is set to **Mastering**, and the screen
    names it.
  - The session reads the device's nominal rate (for FR-010's note) and listens for
    `kAudioHardwarePropertyDefaultOutputDevice` and `kAudioDevicePropertyDeviceIsAlive`. A
    change of device or a lost device stops playback with a message, as the spec's edge case
    requires.
  - On other platforms playback is unavailable in 001.
- **Verified by a spike (2026-11-02)**: the unit accepts 6, 8, 16, 44.1 and 96 kHz on the input
  scope and pulls each take's own rate (3066 / 3994 / 7988 / 22 016 / 47 926 frames in 0.5 s).
- **On screen**: "Playback is converted by macOS to the device's rate (Mastering quality). The
  conversion can raise peaks above the sample peak, and the device clips at ±1.0." The desk
  checklist recommends headphones, because the Mac's built-in speakers add their own processing.
- **Alternatives**: `cpal` opens only the device's own rates (macOS output devices offer no 6, 8 or
  16 kHz), and feature 003 must account for this (noted in the plan of record); `rodio`
  resamples in its own code.

## R-05 The real-time contract of the callback (FR-011)

- **Decision**: the takes are `Arc<[f32]>`, each built once: the input at decode and the render
  when it finishes (coach 7). The callback holds clones of the two `Arc`s and an `Arc<Shared>`
  of atomics:
  - `cursor` (bit 63 the take, bits 0 – 62 the position);
  - `request` (`u64::MAX` means none; swapped in at the top of a buffer);
  - `volume_bits`;
  - `playing`;
  - `overloads`.
- **The callback's work**: `fill` copies the samples times the monitor volume, writes zeros past a
  take's end, and allocates, locks and drops nothing. A new render or file stops playback first,
  and the old `Arc`s are dropped on the UI thread after `stop` (P3). Overloads come from a listener
  on `kAudioDeviceProcessorOverload`.
- **Test**: `fill` is a plain function in `workbench_core::monitor`. Its allocation test is its own
  test binary with a counting global allocator, run single-threaded: zero allocations over 10⁵
  calls, plus the request swap, the clamp and the zeros.

## R-06 The A/B position map (verified, B4)

- **The map**: `rr_dr60::timeline::onepass_output_to_input(o, L, events, drop)` gives
  i = o + removed(o) − L, clamped at 0 (`timeline.rs:218-239`), and is non-decreasing. In mute mode
  it is o − L. At the front taps L = 0, so it is the identity. Before the gate the reports are empty.
  With the coder bypassed, L excludes its delay.
- **Render → original**: the engine's map.
- **Original → render**: the smallest o whose **unclamped** `o + removed(o)` is ≥ i + L, found by
  binary search over that strictly increasing key. Using the unclamped key fixes the start: every
  o ≤ L clamps to 0, and the clamped key would map input 0 to output 0 instead of L. A position
  inside a removed pause lands on the splice. An input position after the last output (a final
  pause that never resumes) clamps to the render's end.
- **"Same seconds"**: o = i. Both directions are clamped to the target's end.
- **Tests**: hand-built reports with exact values, input 0 mapping to L, the final-pause case, and,
  at every rate, `map(inverse(i)) ≥ i` and `inverse(map(o)) ≤ o`.

## R-07 Gate reached, the tail and the readings (B1, B5, B6; coach 1, 2)

- **Gate reached**: the engine's predicate is quoted in the workbench with its citation, because
  the engine does not publish it (`paths.rs:204-206` at `a06318c`):
  `vas.enabled && (tap ∈ {AfterVas, AfterDecoder} || tap is after stage 10)`.
  - `max_events` is **not** the predicate. It is non-zero at `AfterRecord` and with the gate
    switched off.
  - A behavior test over every tap × `vas.enabled` pins the predicate: a drop-mode burst plus 3 s of
    silence must come out shorter exactly when the predicate holds. If the engine's behavior
    moves, this test fails.
  - An **engine issue** asks for a published `Pipeline::gate_runs()` (P11).
- **The tail (D7)**: T = `latency_samples()` + `drain_samples()`, labelled "the engine's latency
  plus its playback drain".
  - `drain_samples()` is the player's drain (`transport.rs:1342`): stage 10's 121 device samples,
    the conversion and N_OUT. It is not a one-pass figure.
  - It covers the codec's frame completion: the delay releases kept sample n at n + 180, after
    the frame is emitted (`pipeline.rs:319-322`).
  - It does not bound the ringing of the record band-pass or of a high-Q inserted filter.
  - In drop mode a pause in the tail freezes everything after the gate (`pipeline.rs:440-446`).
    Every input sample still comes out, because the minimum hang (50 ms = 300 device samples,
    `validate.rs:212`) exceeds the delay after the gate.
  - **T is 0 at the front taps**, where L = 0 and nothing after the gate runs (P12).
  - **Tests**:
    - `tail_emits_the_input`, the test that does not rest on T itself: at seven rates, hang 50 ms,
      coder on, drop mode, an input ending on a tone, assert produced ≥ input + L.
    - `tail_is_sufficient`: extending the tail by a further second leaves the render's prefix
      bit-identical, with the extra output below −100 dBFS at the default settings, hiss off.
    - A reported figure: the energy after T with a high-Q band filter.
  - An **engine issue** asks for a one-pass drain figure (P11).
- **The readings**:
  - The splices or muted regions are Σ `BlockInfo.events`, shown only when the gate is reached.
  - Paused at end is the last call's `paused`.
  - **Not emitted** = input + tail − output, labelled "removed pauses, dropped tail and samples held
    at a final pause". It is 0 in mute mode, with the gate bypassed and at the front taps, which a
    test asserts.
  - The sample peak is labelled "workbench analysis".
- **Wording**: the screen says "splices" or "muted regions", never "gate events", because the
  denylist has the stem `event` (P5; spec Clarifications).

## R-08 Classifying and checking the controls (FR-006; coach 4, B10)

- **Class**: a control is a settings field when
  `control_map::settings_value(&Settings::new(rate), i)` returns `Ok`. It returns `NotASetting` for
  the live and component controls (`control_map.rs:418, 440`), and those are written on an engine
  with `Pipeline::set_control_value` (live and component, `control_map.rs:606`).
- **The check engine**: one `Pipeline::new(Settings::new(rate))` is kept. The checks run in this
  order:
  1. `Settings::validate` on the defaults plus the one value;
  2. `validate` on the whole settings;
  3. `reconfigure(settings)` on the check engine, which also refuses rate plans and memory
     (`pipeline.rs:536-539`);
  4. every live and component value applied through `set_control_value`;
  5. a zero-length `process(&[], &mut [])` for the tap against the route.
  `process` checks the lengths, then `refuse_if_active`, then `tap_on_route`, and an error returns
  before `run_block` (`pipeline.rs:641-651`).
- **Seam**: `Panel::from_entries(entries: &[ControlEntry], rate)` builds the panel model from any
  entry list. `Panel::new(rate)` is `from_entries(controls(), rate)`. SC-003's "a control added to
  the engine appears" is tested by feeding a copied and extended entry list, because a
  `#[non_exhaustive]` struct can be copied and mutated outside its crate (engine spec 014's lesson).

## R-09 The render (FR-007) and the oracle (B11 kept)

- **Decision**: a render thread builds `Pipeline::new(settings)` and asserts
  `recording_counter() == 0`. It applies the slot, its parameters 0 – 3, the route, the volume and
  the battery voltage, in that order. It then calls `process_with_events` in blocks of 4096 over
  the input followed by T zeros, appending `output[..produced]` and the reports. Cancel is checked
  per block, and progress is the input consumed. The oracle repeats the calls with one whole-input
  call and at block size 1.
- **Speed**: the engine records 1.23 µs per device sample at 48 kHz with the coder on. The test
  profile builds `rr_dr60` at opt-level 3 (a package override), because the block-size-1 oracle
  runs at 7 rates × 3 stimuli × 2 modes. SC-004 is quoted from a release run.

## R-10 The sidecar (coach 13)

- **Format**: the grammar is in [contracts/sidecar-v1.md](contracts/sidecar-v1.md).
  - Every value is written as the engine reads it back. **−0.0 becomes 0.0 on every path**, because
    `set_settings_value` stores it as 0.0 (`control_map.rs:324`) and so does the live path.
  - Choices are written as decimals.
  - The seed and the host rate appear only as control lines.
  - The slot is restored before its parameters.
  - Channels are numbered from 1 in the file and from 0 in the model.
- **f32 round-trip (verified)**: Rust's `{}` prints the shortest round-trip decimal, and the bits
  field rules.

## R-11 Digests

SHA-256 (`sha2` 0.11) over the raw file's bytes, over the converted input's f32 little-endian bytes
and over the render's f32 little-endian bytes. hound keeps the bits, so the digests are exact.

## R-12 The engine commit at run time

`workbench_core/build.rs` reads `../../Cargo.lock`, takes the `rr_dr60` package's
`git+…?rev=…#<sha>` and exports `RR_DR60_COMMIT`. It emits
`cargo:rerun-if-changed=../../Cargo.lock`, and a missing source fails the build.

## R-13 Wording (FR-016; coach 8)

- **Strings**: every string the workbench writes lives in `workbench_core::text`.
- **Test**: it reads the engine's `DENYLIST_STEMS`, `DENYLIST_PHRASES` and the suffix set from the
  pinned checkout's `crates/rr_dr60_harness/src/manifest_checks.rs` (located with
  `cargo metadata`), and adds a **workbench supplementary list**: *accurate*, *reveal*, *uncover*,
  *hidden voice*, and *measured* except in "not measured". These are the engine's per-test lists
  (`us2_cross_check.rs:1226`) plus constitution VI's word. If parsing the engine source fails, the
  test fails.
- **Why not a dependency**: a dev-dependency on `rr_dr60_harness` would switch on the engine's
  `__test-hooks` feature in our test build.
- **Engine issue**: the engine should publish one list from its core (P6).

## R-14 The reach test (FR-017; coach 3)

- **What it does**: the test runs `cargo metadata`, scans **every** `.rs` file of the pinned
  `rr_dr60` for `pub fn` items inside `impl Pipeline` blocks, and excludes `#[doc(hidden)]` items
  and names starting with `__`. That leaves **74 at 0.92.0**: 81 minus the seven hidden test hooks,
  which can never be reached.
- **Counting calls**: it scans `crates/*/src` for `.name(` and `Pipeline::name(`, then prints
  `called / total` and the uncalled list.
- **What it asserts**: the spec's Engine reach list, restated with `process` and `reconfigure` (the
  check engine uses both), so 15 methods. The ratio itself is not asserted before feature 006.
  The expected print at 001 is **15/74**.

## R-15 Long files and memory

The bound is three f32 copies: the input, the previous render and, while rendering, the new
render, each an `Arc<[f32]>` built once, plus `footprint_bytes()`. Decoding streams into the
selected channel only, so a channel change decodes the file again. The session shows the sizes.
