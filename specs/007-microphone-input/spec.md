# Feature Specification: The Mac's microphone as the input

**Feature Branch**: `007-microphone-input`

**Created**: 2026-11-03

**Status**: Under the owner's autopilot rule (2026-11-02). The spec-and-plan reviews (coach 1 – 12, hardware Q1 – Q10) are applied, and their questions take their defaults.

**Input**: Feature 007 of docs/workbench-plan.md: "An input device as the source, the engine run in the input callback so the input clock drives it (no drift gaps), the take keeping the captured input; acoustic feedback and capture digests handled." It builds on 003 (the live core, its commands and its take) and on 003's deferral, which records the hardware review's design: run the engine in the input callback.

## Clarifications

### Session 2026-11-03 (autopilot, the spec-and-plan reviews)

- **The hand-over** (coach 1 – 4, hardware Q2): the take's request is the capture file's.
  - It is channel 1 (the file is mono; the device's channel is shown on the screen).
  - Its raw digest is the capture WAV's bytes, and its file name is the WAV's.
  - On Stop, the session opens the capture (`Session::accept_capture`), sets the panel to the take's start values and the tail off, and accepts the take as 003's live take. No sidecar restore is involved. So the take is not stale, and its sidecar, reopened with the capture, re-renders it.
  - The swap replaces the open file and drops its render, B and any restored range; the screen says so.
- **The mix** is 001's exactly: one `input::mix_frame`, used by `mix_floats` too (hardware Q3: `Iterator::sum` keeps an all-−0.0 frame at −0.0).
- **Capture mode never touches the file FIFO**, and its meter rows' tail flag is off (hardware Q1).
- **Gaps** (coach 6, hardware Q6): "no drift gaps" holds between the take and its capture. The device can still lose input (a missed deadline, an overload). Each callback's sample time is checked, and gaps are counted and shown; the take still reproduces.
- **Device changes** (coach 5): a lost device, a changed default input or a changed rate stops the capture as Stop does, keeps the take, and says why (polled each UI frame).
- **The monitor** (coach 7, hardware Q8): a small queue, about 100 ms plus two device buffers. It is primed to 20 ms, and primed again after an underrun. Above 100 ms it drops whole 10 ms blocks, counted. Underruns count only while the gate is not paused.
- **Stop** (coach 8, hardware Q10): the input unit stops first. The staged part (under 10 ms) is not in the capture or the take. The tail is off, so the engine's last latency of output (1923 samples at 48 kHz with the coder on) is not in the take; the screen gives the figure. Every other stream's start stops a capture first, keeping the take.
- **Length** (coach 9): the capture stops by itself before the WAV export's limit.
- **The capture folder** (coach 11): `~/Library/Application Support/rr-dr60-workbench/captures`, the first unused `capture-<n>.wav`.
- **Digital silence** (coach 12) is evaluated once, after a second of capture. A later non-zero sample clears it, and it never returns.
- **Live commands** (restated): the core takes 003's commands during a capture (`capture_commands_reproduce`). The window applies the schedule list. Sending commands from the window during a capture is deferred, because 003's live section is tied to its own run.
- **The binding** (hardware Q7): coreaudio-rs's input proc resizes its buffer when the device changes its callback size. That is the binding's allocation, not the workbench's, and it is rare. `capture_allocates_nothing` covers the workbench's body.
- **Bluetooth** (hardware Q9): a headset's microphone runs at 16 or 24 kHz, and 24 kHz is refused. The refusal and the monitor's hover text say so.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **The source.** **Capture** opens the system's default input device at its nominal rate. That rate must be one of the engine's host rates; otherwise capture is refused in words, naming the rate and Audio MIDI Setup. The channel is one of the device's channels or their mix: the mean in f64 in channel order, rounded to f32, the same rule as 001's float mix. This is a boundary conversion, stated on the screen. The captured samples are the device's f32 samples, as CoreAudio delivers them. | **The default input, its rate, a channel or the mix** |
| D2 | **The engine in the input callback.** The device's clock drives the engine: each input callback appends the converted samples to a staging buffer and runs the one-pass engine on every whole 10 ms chunk, through 003's chunk routine, with no budget and no skips. The panel's values are applied at the start. 003's schedule list (positions in the input or output clock from the start) and its live commands apply as in 003 D2 and D3. The engine's output, gate reports, change log and meter rows go to the take's queues, and the converted input goes to a capture queue. | **The input clock drives the engine** |
| D3 | **The values at the start.** When the session's panel is at the device's rate, its values are used. Otherwise a default panel at the device's rate is used, and the screen says so. | **The panel when its rate matches** |
| D4 | **Monitoring and acoustic feedback.** The engine's output can be heard through the default output device, through a monitor queue read by 001's output unit. **Monitoring is off by default** ("use headphones: the microphone hears the speakers"). It is never part of the take. An empty monitor queue plays zeros, and a full one drops samples; both are counted and shown. Drift between the two devices therefore costs only monitor samples, never take samples. | **Off by default, the take untouched** |
| D5 | **The take.** **Stop** ends the capture. The samples of an unfinished 10 ms chunk are not fed, and the screen says how many. The take is a live take (003's `Render`):<br>• its request's input is the captured input, from 0, its length the samples fed;<br>• the tail is off;<br>• its changes are the log's scheduled entries.<br>The captured input is written as a 32-bit float WAV to the workbench's capture folder, and the session opens it as its input. The take's sidecar values are restored, and the take becomes the session's render. So 001's A/B, export and sidecar, 002's views and 005b's comparisons all work on it. The input's SHA-256 (001's digest of the converted samples) is the capture digest, and the sidecar records it with the capture's file name. **Save capture…** copies the capture WAV. | **A live take over the captured input** |
| D6 | **Reproduction.** A finished take equals the offline render of its request, the captured input with its changes, bit for bit: output, gate reports, log and meter rows (003 D5). The exported take's sidecar reproduces it from the capture WAV. | **Bit for bit, as 003** |
| D7 | **Real-time safety.** Everything is allocated before the stream starts: the engine, the staging and chunk buffers, and the queues, sized for about 10 s of UI stall (as 004b). The input callback allocates nothing, locks nothing and does no I/O. A full take or capture queue marks the take **incomplete**: it does not reproduce and is not exported. | **As 003 D8, 004b B1** |
| D8 | **Digital silence.** When the first second of capture is all zeros, the screen says the input is digital silence and points to the microphone permission (System Settings → Privacy & Security → Microphone, for the app the workbench runs from). | **Shown, not blocking** |
| D9 | **One audio stream at a time.** A capture excludes 003's live run, 004b's live playback and 001's A/B. The monitor uses the output unit. | **One at a time** |
| D10 | **Not in 007**: input devices other than the default, the input device's own rate changes during a capture (the capture stops and says so), and recording the capture into the device's memory (the device records files, 004). | **Deferred** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Capture through the engine (Priority: P1)

A user captures from the Mac's microphone through the engine, then stops; the take and the captured input are in the session.

**Independent Test**: the core fed synthetic callbacks of irregular sizes at seven rates, without a device.

**Acceptance Scenarios**:

1. **Given** a capture core fed interleaved callbacks of irregular sizes (1 … 4096 frames, two channels), **When** it is stopped, **Then** the take equals the offline render of its request bit for bit (output, reports, log, meter rows), at seven rates (`capture_equals_offline`).
2. **Given** channel 2, or the mix, **When** captured, **Then** the captured input is that channel, or 001's float mix, bit for bit (`capture_channel_and_mix`).
3. **Given** commands sent during the capture and a schedule list, **When** it is stopped, **Then** the take reproduces with the take's own changes (`capture_commands_reproduce`).
4. **Given** a stop with a part chunk staged, **When** stopped, **Then** the part is reported and not fed (`capture_part_chunk`).
5. **Given** the capture written as a float WAV, **When** it is opened as an input, **Then** its samples and digest equal the take's input (`capture_wav_roundtrip`); **and** the take's sidecar restores and re-renders it bit for bit (`capture_sidecar_reproduces`).

### User Story 2 - Monitor safely (Priority: P2)

**Acceptance Scenarios**:

1. **Given** monitoring on with the monitor queue drained slower or faster than captured, **When** stopped, **Then** the take is unchanged, and the drops and underruns are counted (`monitor_never_touches_the_take`).
2. **Given** the input callback, **When** run with monitoring and commands, **Then** it allocates nothing (`capture_allocates_nothing`).
3. **Given** a full take queue, **When** stopped, **Then** the take is incomplete and not exported (`capture_incomplete`).
4. **Given** a second of zeros, **Then** the digital-silence notice is raised; one non-zero sample clears it (`digital_silence_noticed`).

### Edge Cases

- **A device rate the engine does not take** (22.05 kHz): refused in words.
- **A device with one channel**: channel 1, no mix.
- **The panel at another rate**: a default panel at the device's rate, noted.
- **Stop before the first whole chunk**: no take, said so; nothing opened (`capture_part_chunk`).
- **The device rate refused** (`capture_rate_refused`); **the panel at another rate** (`capture_default_panel`); **a one-channel device** (`capture_channel_and_mix`); **the hand-over** (`capture_session_swap`); **gaps** (`gaps_counted`); **the monitor's bound** (`monitor_latency_bounded`); **the folder's names** (`capture_folder_names`).

## Requirements *(mandatory)*

- **FR-001**: Capture MUST follow D1 – D3 (`capture_equals_offline`, `capture_channel_and_mix`, `capture_commands_reproduce`).
- **FR-002**: Monitoring MUST follow D4 (`monitor_never_touches_the_take`).
- **FR-003**: The take and its files MUST follow D5 and D6 (`capture_part_chunk`, `capture_wav_roundtrip`, `capture_sidecar_reproduces`).
- **FR-004**: The callback MUST be real-time safe (D7; `capture_allocates_nothing`, `capture_incomplete`).
- **FR-005**: Digital silence MUST be noticed (D8; `digital_silence_noticed`).
- **FR-006**: Every new string MUST pass the wording check; the reach test prints its ratio.
- **FR-007**: The device check (`playback_check`) MUST gain a capture case, run only when asked (it needs the microphone permission).

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, captures fed in irregular callbacks reproduce offline bit for bit.
- **SC-002**: The input callback allocates nothing.
- **SC-003**: The owner's desk checklist: capture a few seconds of speech with monitoring off, stop, listen A/B, export the take and its capture, reopen the capture with the sidecar and render: the same digest; then capture with headphones and monitoring on.

## Assumptions

- The engine is pinned at `a06318c` (0.92.0).
- A CLI-launched workbench gets the microphone permission of the terminal it runs from (macOS TCC).
