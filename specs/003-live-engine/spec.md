# Feature Specification: The live engine

**Feature Branch**: `003-live-engine`

**Created**: 2026-11-02

**Status**: The draft reviews (coach 1 – 15, hardware B1 – B10) are applied. Under the owner's autopilot rule (2026-11-02), the decisions take effect at their recommendations. `/speckit-clarify`: see Clarifications.

**Input**: Feature 003 of docs/workbench-plan.md: "Real-time playback through the engine on the output device. Live controls and scheduled changes (`schedule_change`) from the panel. The change log and voice-activation events on a timeline. Optionally, the Mac's microphone as the input." It builds on 001 (the session) and 002 (the views).

## Clarifications

### Session 2026-11-02 (autopilot)

- **The microphone is deferred** to a later feature (coach 12). It brings a second device clock, and drift between two devices makes gaps certain on long runs. The hardware review's design is recorded for it: run the engine in the input callback, so the input clock drives it and the take has no gaps.
- **About 10 ms of added latency is accepted**, so that the live meter rows equal 002's (coach 7).

### Session 2026-11-02 (post-implementation review: dated restatements)

- **D6, the timeline**: a list beside the waveform (changes, gate reports, skips), exported as CSV, rather than markers on 002's waveform. Input-clock changes cannot be placed on the output exactly in drop mode.
- **D3, ordering**: the list is sorted by position before it is tagged, so entries added out of order land where they belong.
- **D5, after the stop**: the panel takes the take's end values (its last applied value per control; a refused command never reaches them), and the take is not stale against them. The sidecar's `tail` is the switch as the run used it: on only if the input ended.
- **D5, re-renders of a take** stay ranged: a ranged request is written `live yes`, so a second generation keeps its range and changes. A sidecar with a change identifier this engine does not know restores its values, lists the identifier, and does not set the range.
- **The callback** never panics: an engine failure stops the run, with its words on screen and the take marked incomplete. A lost reply is counted and does not mark the take.
- **The live output** is scaled by the monitor volume on playback only. The take keeps the engine's samples.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **The live core.** The engine runs inside the output callback, at the file's rate (the device's own conversion applies as in 001), on a fresh one-pass engine built like 001's render with the panel's values at the start. Each callback calls the engine in **whole chunks of rate ÷ 100 input samples** (002's block) into a pre-allocated output FIFO. It feeds chunks until the FIFO holds the device buffer, or until D4's budget is spent, then copies the buffer out of the FIFO. The FIFO adds up to 10 ms of latency. Because the engine's output does not depend on how its input is split into calls, the live output, gate reports and change log equal an offline render of the same input range, values and changes. The meter rows are those of the same 10 ms calls. | **Engine in the callback, 10 ms chunks, a FIFO** |
| D2 | **Live controls.** While live, a control takes effect at once when its timing is **live** and the one-pass accepts it in this configuration. The callback schedules it at the stream's current position in that control's own clock (`changes::clock_of`; the input clock for record-side values, the output clock otherwise). Every such command is scheduled with a tag starting at 1: its sequence number. A control is disabled, with the engine's own reason, when:<br>• its timing is idle ("idle: stop to change");<br>• the one-pass does not run it (`playback_speed`, `playback_speed_mode`, stage 10's switch: "not on the one-pass");<br>• its stage is off at the start or beyond the tap ("stage not running in this configuration");<br>• its route does not match a named-route tap ("tap not on this route");<br>• it is a band-filter parameter with the slot empty ("slot empty").<br>The reasons come from a dry-run schedule on a check engine at the start configuration. Commands reach the callback through a fixed-capacity lock-free queue. Each command's result returns through a reply queue, and the UI shows the engine's error text. A command that finds the queue full is refused on screen, never dropped silently. | **Live values at once; the rest disabled with the engine's reason** |
| D3 | **Scheduled changes.** The user lists changes: control, value and time. The time is entered in the control's clock, labelled "input s from the start" or "take s (output)", and converted as position = round(t · rate) from the run's start. The list lives on the workbench side, sorted and stable. Before each engine call, the callback schedules only the entries due before that call's end. The engine allows 64 changes per clock to be pending at once, and an entry it refuses is reported with its words. | **A list fed to the engine just in time** |
| D4 | **Drop mode live.** A paused gate emits less output than input. Each callback feeds chunks until the FIFO holds the buffer or **4 × the buffer** of input has been fed. If the budget runs out, the rest of the buffer is silence labelled "gate paused: skipping". That silence is not engine output: it is not captured, and its device positions are listed. Each call is bounded: the core asserts `produced ≤ fed`. | **Bounded skip, labelled** |
| D5 | **The take and its reproduction.**<br>• **Range**: a live run starts at input position **S** (the session's chosen position, default 0) and covers the input it fed, [S, S + fed). `fed` counts the tail's silence when the input ended during the run, and the tail switch is "on" only then.<br>• **Take**: all the engine output produced, including output still in the FIFO at the stop, with the gate's reports, the change log and the meter rows. Stopping makes the take the session's render.<br>• **Sidecar version 2**: version 1's fields, with the value block taken from the request's snapshot **at the start** (never read back at the stop), plus `live yes`, `start S` and `fed n`. One `change <in\|out> <position> <identifier> <value decimal> <value 0x f64 bits> <tag>` line follows per entry of the engine's change log with a tag of at least 1, in log order (ties land in submission order). Tag-0 entries, the start's plain setters, are not written, and a reader refuses a change line with tag 0. A reader accepts versions 1 and 2.<br>• **Reproduction**: re-rendering a version-2 sidecar feeds input[S .. S + fed] to a fresh engine with the start values and schedules the change lines in file order. This reproduces the take's samples, gate reports and change log bit for bit. The meter rows also match, both being 10 ms calls. | **The take, its range, sidecar v2** |
| D6 | **The timeline.** The take shows, on 002's waveform time axis:<br>• every change-log entry (clock, position, control, before, after, tag);<br>• every gate report (splice or muted region);<br>• the budget's skips.<br>The list exports as CSV with the version-2 sidecar beside it. | **The log, the reports and the skips** |
| D7 | **Live meters.** After each callback, the latest 10 ms row is sent to the UI through a queue, and the 002 field table shows it as live bars, with the reach rule. The take's full timeline appears after the stop. | **Latest row as bars** |
| D8 | **Buffers and real-time safety.** The engine, the FIFO (4096 + 2 chunks), the events buffer (sized for the largest chunk at the hang range's minimum), the changes buffer (`MAX_CHANGES_PER_CALL`) and the queues are allocated before the stream starts. The callback allocates nothing, locks nothing and does no I/O. A buffer above 4096 frames is served in parts. An events or changes overflow, or a capture queue that is full, marks the take **incomplete (not reproducible)**, counted and shown. | **Pre-allocated; overflows mark the take** |
| D9 | **One output stream.** While live, 001's A/B playback is disabled. A live run starts and stops; it has no seek. | **One stream, no seek** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Hear the engine live and turn its knobs (Priority: P1)

The user starts a live run on a file and changes the volume, the route, the gate's threshold and the band filter's frequency while listening. Each change is heard at once and logged where it landed.

**Independent Test**: The live core is driven by simulated callbacks (sizes and commands from a fixed-seed generator, the seed printed). Its take equals the offline re-render of its version-2 sidecar. The callback body allocates nothing.

**Acceptance Scenarios**:

1. **Given** a session at start position S, **When** a live run plays to the input's end with no changes, **Then** the take equals 001's render of input[S..] as its own input, samples and reports (`live_equals_render`).
2. **Given** a live run, **When** the volume changes, **Then** the change lands on the first sample of the next engine call in the output clock, logged with its before and after values and tag (`live_change_logged`).
3. **Given** a live run, **When** the gate threshold changes, **Then** the change lands in the input clock and is logged (`live_record_side_change`).
4. **Given** a live run, **When** the panel is shown, **Then** idle controls and the controls the one-pass refuses are disabled with the engine's reason (`live_disabled_reasons`).
5. **Given** a stopped live run with changes, stopped by hand mid-file, **When** its sidecar is re-rendered offline, **Then** the samples, reports and change log equal the take's bit for bit (`live_take_reproduces`).
6. **Given** callbacks of 1 to 4096 frames, and above, **When** the core runs, **Then** no allocation or deallocation happens in the callback body (`live_callback_allocates_nothing`).

### User Story 2 - Schedule changes and read the timeline (Priority: P2)

**Acceptance Scenarios**:

1. **Given** a list of changes at times in their clocks, **When** a live run passes them, **Then** each lands at its position and is on the timeline (`schedule_lands_exactly`).
2. **Given** more than 64 changes due within one call in one clock, **When** they are fed, **Then** the excess is reported in the engine's words (`schedule_limit_refused`).
3. **Given** a drop-mode take with a long pause, **When** the timeline is shown, **Then** the gate reports and the budget's skips are listed (`timeline_lists_reports_and_skips`).
4. **Given** a timeline, **When** it is exported, **Then** the CSV lists every entry, with the version-2 sidecar beside it (`timeline_csv_roundtrip`).

### Edge Cases

- **A full command queue**: the command is refused on screen.
- **The input ends during a live run**: the tail (001's rule) is fed, then the run stops by itself, with the tail switch on in the sidecar.
- **A device change or loss** stops the run with 001's message, and the take is kept.
- **A device buffer above 4096 frames** is served in parts.
- **A gate that stays paused**: each callback is bounded by D4, and the skip label shows.
- **A long run whose capture queue fills**: the take is marked incomplete.
- **Stopping during a skip**: the take is the engine's output only, and the skip is listed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The live core MUST be a plain state with a callback body, testable without a device, calling the engine in 10 ms chunks through a FIFO (D1, D8).
- **FR-002**: With no changes, the take MUST equal 001's render of input[S..] at any callback sizes (`live_equals_render`).
- **FR-003**: Live commands MUST be scheduled at the stream's current position in their clock with tags from 1, and the disabled controls MUST show the engine's reason (D2).
- **FR-004**: The listed changes MUST be fed just in time and land at their positions, with refusals in the engine's words (D3).
- **FR-005**: Drop mode MUST be bounded by D4, with the skips labelled, listed and not captured.
- **FR-006**: The take MUST hold every sample the engine produced, the reports, the log and the rows, and become the session's render at the stop (D5).
- **FR-007**: Sidecar v2 MUST follow D5, a reader MUST accept versions 1 and 2, and a version-2 re-render MUST reproduce the take (`live_take_reproduces`).
- **FR-008**: The timeline MUST show and export D6's entries.
- **FR-009**: The callback MUST allocate nothing, and the overflows MUST mark the take incomplete (D8).
- **FR-010**: The feature MUST call `schedule_change`, `process_with_events_and_changes`, `stream_position`, `input_position`, `pending_changes`, `max_events` and `clear_schedule`, and the reach test MUST print the ratio (constitution II).
- **FR-011**: Every new string MUST pass the wording check (constitution VI).

### Key Entities

- **LiveCore**: the engine, the input range, the FIFO, the buffers, the queues, the due list and the counters.
- **Command**: a control index and value with its tag, "now" or at a position in a clock.
- **Take**: the output, the reports, the log, the rows, the skips, `start`, `fed` and the counters. It becomes a `Render` whose request holds input[S .. S + fed].

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, live takes with fixed-seed callback sizes (1 – 4096) and commands equal the re-renders of their sidecars in samples, reports and log (printed seeds).
- **SC-002**: The callback body allocates nothing over 10⁵ simulated callbacks with commands.
- **SC-003**: A command lands on the first sample of the next engine call after it is received (test). It is heard within one buffer plus the FIFO, plus the engine's latency for input-clock values (the desk checklist).
- **SC-004**: In a release build at 48 kHz with the coder on and drop mode at its budget, the worst callback of 64 and of 512 frames stays below 25 % of its real time, with the mean reported.
- **SC-005**: The owner's desk checklist: go live, turn the knobs, schedule two changes, stop, read the timeline, export, reopen the sidecar and reproduce.

## Assumptions

- The engine is pinned at `a06318c`. Its one-pass has two schedules of 64, one per clock (`pending_changes` sums them), logs every change it applies, and is real-time safe (`schedule_change` is a bounded insert).
- The live take's meter rows equal a re-render's because both use 10 ms calls.
- Alternatives considered:
  - **D1**: callback-sized engine calls. They give simpler latency, but the meter rows would differ from 002's.
  - **D2**: the plain setters. They log tag 0, which cannot be told apart from the start values, and a route set and set back across an empty call never lands.
  - **D5**: a separate schedule file.
  - The microphone, now deferred.
