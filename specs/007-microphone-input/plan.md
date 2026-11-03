# Implementation Plan: The Mac's microphone as the input

**Branch**: `007-microphone-input` | **Date**: 2026-11-03 | **Spec**: [spec.md](spec.md)

## Summary

An input-driven mode of 003's live core: the input callback converts the device's frames to the engine's mono input, stages them, runs whole 10 ms chunks through the same chunk routine, and sends the take's queues, a capture queue and an optional monitor queue. Stop builds 003's live take over the captured input. The window writes the capture as a float WAV, opens it as the session's input, restores the take's sidecar, and accepts the take.

## Technical context

Rust 2024; coreaudio-rs 0.14 (`macos_helpers::{get_default_device_id, audio_unit_from_device_id}`, `set_input_callback` with interleaved f32), rtrb, as before. The engine is pinned at `a06318c`. No new dependencies.

## Constitution check

- **I**: the engine is the only signal processing. The channel pick or mix is a boundary conversion, stated (D1). The monitor's zeros and drops are playback only, never in the take or an export.
- **II**: no new `Pipeline` methods expected; the reach test prints its ratio.
- **III**: the take reproduces from the capture WAV and its sidecar (D6). The capture's digest is 001's.
- **IV**: the callback is real-time safe (D7), and the allocation test covers it.

## Research

- **R-01 The core** (`crates/workbench_core/src/live.rs`):
  - `LiveCore` gains a mode: `File` (003, unchanged), or `Capture { staged, channels, channel, captured: Producer<f32>, monitor: Option<Producer<f32>>, monitor_drops }`.
  - `LiveCore::new_capture(req, list, channels, channel)` takes a request at the device's rate with an empty input, start 0 and tail off. The end is unbounded.
  - `push(&mut self, interleaved: &[f32])` is the input callback's body. Per frame it picks the channel or computes `mix_frame` (f64 sum in channel order ÷ n, rounded to f32, equal to `input::mix_floats` per frame). It stages the samples and runs `run_staged` per whole chunk. `run_staged` is `chunk` with the input taken from the staging buffer, so the File mode's `chunk` copies its file slice into the same buffer.
  - Output goes to the samples queue and to the monitor queue when present (a full queue counts a drop). The fed input goes to the capture queue; a full one marks incomplete. A meter row's `tail` is false.
  - Commands are taken at the top of each `push`, as in `fill_part`.
  - `staged()` gives the unfed part.
- **R-02 The take**:
  - `TakeBuilder` gains `captured: Vec<f32>`, drained from `LiveQueues.captured` (None in File mode).
  - `TakeBuilder::capture_request(start_req)` sets the input to the captured samples, with their SHA-256 (`input_sha256`). The raw digest is the capture WAV's bytes (`raw_sha256`, so the session accepts the take once it opens the file). It also sets `frames`, channel 1, tail false and start 0. The window sets `file_name` to the WAV's name.
  - Then 003's `finish`.
- **R-03 The files**:
  - `capture::wav(captured, rate)`: `export::wav_bytes` (32-bit float).
  - Opening it with `input::open_bytes` gives the same samples bit for bit (f32 WAV decoding is the identity), so the same `input_sha256`. This is pinned by `capture_wav_roundtrip`.
  - The capture folder is `~/Library/Application Support/rr-dr60-workbench/captures`, and the file the first unused `capture-<n>.wav` (`capture::{folder, next_path}`). **Save capture…** copies it.
- **R-04 The window** (`crates/workbench/src/capture.rs`, main.rs):
  - `capture.rs`: the input unit on the default input device at its nominal rate (`playback::mac::nominal_rate`), interleaved f32 with the device's channel count, and an input callback calling `LiveCore::push`.
  - The monitor is 001's output unit with `Playback::start_live` and a fill that pops the monitor queue, zeros on empty, counted.
  - A **Capture** section in the Session tab: the channel, the monitor switch with the headphones warning, Start and Stop, the counters (fed, staged, monitor drops and underruns, incomplete) and the digital-silence notice.
  - On stop: write the capture, then `session.accept_capture(path, take)`. That opens the file, sets the panel to the take's start values and the tail off, and accepts the take as 003's live take. There is no sidecar restore.
- **R-05 Digital silence**: the UI counts the captured samples it drains. When the first `rate` are all zeros, the notice is shown, and it clears on a non-zero sample.
- **R-06 The device check**: `playback_check --capture` captures 1 s from the default input with monitoring off. It prints the rate, the channels, the fed count, whether the input is digital silence, and the offline-reproduction result. It is not run by default.

## Testing

`tests/capture.rs` holds the spec's names; `tests/capture_alloc.rs` holds the allocation test (one binary per global allocator, as `live_alloc`). The input is synthetic interleaved stereo: voice on channel 1, a tone on channel 2. The callbacks are of irregular sizes (1, 7, 64, 441, 512, 4096, …). The oracle is `render::render` of the take's request.
