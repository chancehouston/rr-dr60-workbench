# Records: the Mac's microphone as the input

Recorded on 2026-11-03, with the engine at `a06318c`.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 185 green, 3 ignored |
| `tests/capture.rs` | 15 green; `tests/capture_alloc.rs` 1 green |
| reach | 51/70 (unchanged) |
| `playback_check --capture` (the owner's terminal, 2026-11-03) | MacBook Pro Microphone, 48 000 Hz, 1 channel: 48 000 fed in 1 s, gaps 0, digital silence false, incomplete false, reproduces offline true |

## Figures

- **`capture_equals_offline`**: at seven rates, interleaved stereo in callbacks of 1 … 4096 frames. The take equals the offline render of its request bit for bit: output, gate reports, log and meter rows. The captured input is whole 10 ms chunks; the staged part is the rest.
- **`capture_commands_reproduce`**: three commands and a two-entry schedule list (input and output clock), at 6 and 48 kHz. Every entry landed, and the take reproduces.
- **`capture_session_swap`**: the window's hand-over (`Session::accept_capture`). B is dropped; the take is not stale; its sidecar, reopened with the capture file, re-renders it bit for bit.
- **`capture_wav_roundtrip`**: the float WAV keeps ±0, subnormals, ±∞ and a signalling NaN's payload bit for bit. The capture digest is the input's.
- **`monitor_never_touches_the_take`** and **`monitor_latency_bounded`**:
  - a slow and a fast monitor leave the take bit-identical;
  - priming, block drops rounded down, underruns counted only with the gate open;
  - a 16 kHz capture with 4096-frame output pulls gives no drops and no underruns after priming.
- **`capture_allocates_nothing`**: the callback body and the monitor's fill, with commands, at 6 and 48 kHz, over 6 s of 4096-frame callbacks.
- **`capture_incomplete`**: a take never drained past about 10 s is incomplete, and its export is refused.

## Reviews, applied

- **Spec and plan** (coach 1 – 12, hardware Q1 – Q10): the spec's first Clarifications session.
- **Post-implementation** (coach 1 – 11, hardware B1 – B9): the spec's second session.
  - **Coach**:
    - **1**: a capture is finished before any session change, and other streams are disabled while it runs.
    - **2**: the staged part is reported.
    - **3**: an incomplete take's export is refused.
    - **4**: the schedule list is tested.
    - **5**: the spec and plan are restated.
    - **6**: the device check sleeps 1.5 s.
    - **7**: the channel fallback is said.
    - **8**: the swap message names the range.
    - **9**: the monitor test compares the takes fully.
    - **10**: the gap counter, as hardware B4.
    - **11**: `capture_wav` returns its error.
  - **Hardware**:
    - **B1, B2**: the monitor's bounds and its round-down drops.
    - **B3**: the binding's resize, a known exception, follow-up.
    - **B4**: the time stamp's validity and timeline resets.
    - **B5**: the nominal rate compared like with like.
    - **B6**: the Bluetooth hint corrected.
    - **B7**: two drop counters, and the monitor's device loss.
    - **B8**: the mix's hover text.
    - **B9**: as coach 11.
- **Follow-ups**:
  - live commands from the window during a capture;
  - a raw input callback with a preallocated buffer (B3);
  - input devices other than the default.
