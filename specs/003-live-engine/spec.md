# Feature Specification: The live engine

**Feature Branch**: `003-live-engine`

**Created**: 2026-11-02

**Status**: Draft. Under the owner's autopilot rule (2026-11-02), the decisions take effect at their recommendations once the reviews are applied.

**Input**: Feature 003 of docs/workbench-plan.md: "Real-time playback through the engine on the output device. Live controls and scheduled changes (`schedule_change`) from the panel. The change log and voice-activation events on a timeline. Optionally, the Mac's microphone as the input." It builds on features 001 (the session) and 002 (the views).

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **Live mode.** The engine runs inside the output callback. The callback feeds the session's input (the file, from a chosen position) to a fresh one-pass engine, built like 001's render with the panel's values, and plays the engine's output. The rate is the file's, and the device's own conversion applies as in 001. The live run is the same calls as a render made of the callback's blocks, so the engine's output equals 001's render of the same input, values and changes, sample for sample. | **The engine in the callback, the file's rate** |
| D2 | **Live controls.** While live, a control whose timing is **live** and whose paths include the one-pass (route, volume, battery voltage, the record-side values of engine spec 017, the playback-side switches of spec 021, the band filter's parameters) takes effect at once: the callback schedules it at the stream's current position in that control's own clock (`changes::clock_of`). The engine logs every change it applies. An **idle** control is disabled while live, with the reason "idle: stop to change". Changes reach the callback through a fixed-capacity lock-free queue, and a change that finds the queue full is refused on screen, never dropped silently. | **Live values at once; idle ones disabled** |
| D3 | **Scheduled changes.** Before or during a live run, the user can add changes at a time: control, value and time in seconds of the input (the input clock for record-side values, the output clock otherwise). They are scheduled at the run's start, or at once when added during the run. The engine's limit (64 pending per stream) is shown, and a change past it is refused with the engine's words. | **A schedule list, with the engine's limits** |
| D4 | **Drop mode live.** In drop mode the engine emits less output than input while the gate is paused. Each callback feeds input until the device buffer is filled or **a budget of 4 × the buffer** has been fed. If the budget runs out, the rest of the buffer is silence, labelled "gate paused: skipping". Live, the user hears the removed pauses go by fast, and the work per callback stays bounded (constitution IV). | **Bounded skip, labelled** |
| D5 | **The live take and its reproduction.** The live output is captured to memory through a lock-free queue as a take, along with the change log and the gate's reports. Stopping makes it a render of the session: shown, played and exported like 001's. The sidecar becomes **version 2**: version 1's fields, plus one `change <position> <control id> <value> <tag>` line per change the engine applied, plus `live yes`. A reader accepts versions 1 and 2. Re-rendering a version-2 sidecar offline, with the input and the changes scheduled at their logged positions, reproduces the live take bit for bit. Silence inserted by D4's budget is not engine output: it is not captured, and it is listed with its positions. | **The take with its log; sidecar v2 reproduces it** |
| D6 | **The timeline.** The live take shows, on the 002 waveform's time axis, every change-log entry (position, control, before, after, tag) and every gate report (splice or muted region). The list is exportable as CSV with the sidecar. | **Changes and gate reports on the timeline** |
| D7 | **Live meters.** The meter read-out after every callback's last engine call is sent to the UI, and the 002 field table shows it as live bars, reached rule included. The full timeline is in the take after the stop. | **Live bars; the timeline after the stop** |
| D8 | **The Mac's microphone (P3).** An input device can replace the file. The engine then runs at the input device's rate, which must be one of the engine's rates (44.1 or 48 kHz on Macs), or the source is refused with the supported list. The input arrives through a lock-free queue from an input callback, and the take keeps the captured input, so it reproduces like a file. An overrun or underrun is counted and shown, and the take records the positions where input was missing. | **Microphone input at the device's rate** |
| D9 | **Real-time safety.** The callback allocates nothing, locks nothing and does no I/O. The engine, its buffers (sized for the largest device buffer, 4096 frames, split if larger) and the queues are created before the stream starts. A test drives the callback body without a device under the counting allocator. Device overloads are counted, as in 001. | **Pre-allocated; tested without a device** |
| D10 | **Positions.** A live run starts from the session's current position in the input (default 0) on a fresh engine. Stop ends it, and the take covers from the start position to the stop. Seeking during a live run is not offered; stop and start again. | **Start at a position, no seek** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Hear the engine live and turn its knobs (Priority: P1)

The user starts a live run on a file and changes the volume, the route, the gate's threshold and the band filter while listening. Each change is heard at once and logged where it landed.

**Independent Test**: Drive the live core with simulated callbacks of varying sizes and a stream of changes. Its output equals an offline render with the same changes scheduled at the logged positions. The callback allocates nothing.

**Acceptance Scenarios**:

1. **Given** a session with a file, **When** the user starts live, **Then** the engine's output plays, and with no changes it equals 001's render of the same input from the start position, sample for sample (`live_equals_render`).
2. **Given** a live run, **When** the user changes the volume, **Then** the change lands at the next sample in the output clock and the log shows it with its before and after values (`live_change_logged`).
3. **Given** a live run, **When** the user changes a record-side value (the gate threshold), **Then** it lands in the input clock, logged (`live_record_side_change`).
4. **Given** a live run, **When** the user tries an idle control (the speech coder switch), **Then** it is disabled with "idle: stop to change" (`idle_disabled_live`).
5. **Given** a stopped live run, **When** its sidecar is re-rendered offline, **Then** the output equals the live take bit for bit (`live_take_reproduces`).
6. **Given** callbacks of 1 to 4096 frames, **When** the live core runs, **Then** no allocation or deallocation happens in the callback body (`live_callback_allocates_nothing`).

### User Story 2 - Schedule changes and read the timeline (Priority: P2)

The user schedules "at 2.0 s set the volume to 0.5" and "at 3.5 s switch the route", runs live, and reads the timeline afterwards.

**Acceptance Scenarios**:

1. **Given** a schedule of changes at times, **When** a live run passes them, **Then** each lands exactly at its position and appears on the timeline (`schedule_lands_exactly`).
2. **Given** more changes than the engine's limit, **When** they are scheduled, **Then** the excess is refused in the engine's words (`schedule_limit_refused`).
3. **Given** a live take in drop mode, **When** the timeline is shown, **Then** every gate report is on it, and the budget's skipped silence is listed with its positions (`timeline_lists_reports_and_skips`).
4. **Given** a timeline, **When** it is exported, **Then** the CSV lists every entry, with the version-2 sidecar beside it (`timeline_csv_roundtrip`).

### User Story 3 - Record from the Mac's microphone (Priority: P3)

The user picks the microphone as the source and hears the engine live on their voice. The take keeps the captured input.

**Acceptance Scenarios**:

1. **Given** an input device at 48 kHz, **When** it is chosen, **Then** the engine runs at 48 kHz and the take's input is the captured samples (`mic_source_feeds_engine`, through a simulated input queue).
2. **Given** an input device at a rate the engine lacks, **When** it is chosen, **Then** it is refused with the supported list (`mic_rate_refused`).
3. **Given** a missing input block (an underrun), **When** the run continues, **Then** it is counted, the gap's position is recorded, and the take still reproduces (`mic_underrun_recorded`).

### Edge Cases

- **The queue to the callback is full**: the change is refused on screen, never dropped silently.
- **The input ends during a live run**: the tail is fed as in 001 (latency plus drain), and the run then stops by itself.
- **The device changes or is lost**: the live run stops with 001's message, and the take up to that point is kept.
- **A device buffer above 4096 frames**: the callback splits it into calls of at most 4096.
- **A gate that stays paused**: each callback is bounded by D4's budget, and the skip label shows.
- **A very long live run**: the take grows on the UI thread from the queue. If the UI falls behind and the queue fills, the overflow is counted and the take is marked incomplete (not reproducible), never silently cut.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The live core MUST run a fresh one-pass engine, built like 001's render, inside a callback body that is a plain function of its state, testable without a device (D1, D9).
- **FR-002**: With no changes, the live output MUST equal 001's render of the same input from the start position at any callback sizes (`live_equals_render`).
- **FR-003**: Live controls MUST be scheduled at the stream's current position in the control's clock, and idle controls MUST be disabled while live (D2).
- **FR-004**: Scheduled changes MUST land at their positions, and refusals MUST come in the engine's words (D3).
- **FR-005**: Drop mode MUST be bounded by D4's budget, with the skipped silence labelled, listed and not captured.
- **FR-006**: The take MUST capture the engine's output, the change log, the gate's reports and the meters' timeline. Stopping MUST turn it into the session's render (D5, D7).
- **FR-007**: The sidecar MUST be version 2 with the change lines and the live flag. Version 1 MUST still be read, and a version-2 restore plus offline render MUST reproduce the take bit for bit (D5).
- **FR-008**: The timeline MUST show and export the change log and the gate's reports (D6).
- **FR-009**: The microphone source MUST run at the input device's rate if the engine supports it, and keep the captured input with its gaps (D8).
- **FR-010**: The callback body MUST allocate nothing (a counting-allocator test), and the queue overflows MUST be counted and shown (D9).
- **FR-011**: The feature MUST bring into use `schedule_change`, `process_with_events_and_changes`, `stream_position`, `input_position`, `pending_changes` and `clear_schedule`, and the reach test MUST print the ratio (constitution II).
- **FR-012**: Every new string MUST pass the wording check (constitution VI).

### Key Entities

- **LiveCore**: the engine, the input source, the queues, the pre-allocated buffers and the counters.
- **Command**: a control change ("now" or at a position) sent to the callback.
- **Take**: the captured output, input (for the microphone), change log, reports, meters, skip list and counters, which becomes a `Render`.
- **Sidecar v2**: v1, plus `live yes` and the `change` lines.

## Success Criteria *(mandatory)*

- **SC-001**: A live run's output equals the offline re-render of its sidecar bit for bit, at seven rates, with random callback sizes from 1 to 4096 and a random stream of changes (simulated).
- **SC-002**: The callback body allocates nothing over 10⁵ simulated callbacks with changes.
- **SC-003**: A live change is heard within one device buffer (the desk checklist) and lands on the logged sample (test).
- **SC-004**: The live callback at 48 kHz uses less than 25 % of real time on the development Mac (release build, simulated: the engine's work for a buffer, measured per callback).
- **SC-005**: The owner's desk checklist: go live on a file, turn the volume, route, threshold and band filter, schedule two changes, stop, read the timeline, export, reopen the sidecar and reproduce, then go live on the microphone.

## Assumptions

- The engine is still pinned at `a06318c`. Its schedule holds 64 changes per stream, and its one-pass logs every change (`MAX_CHANGES_PER_CALL` 158).
- Alternatives considered:
  - **D1**: running the engine on a worker thread with a ring to the callback (more latency and more moving parts, for no benefit, since the engine is real-time safe).
  - **D4**: unbounded feeding (a long pause in one callback would overload it), or forcing mute mode live (it would hide drop mode's character).
  - **D5**: a separate schedule file (a second artifact to keep beside the sidecar).
  - **D8**: resampling the microphone to a file rate (signal processing of the workbench's own).
