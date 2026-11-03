# Feature Specification: Live playback

**Feature Branch**: `004b-live-playback`

**Created**: 2026-11-02

**Status**: In force under the owner's autopilot rule (2026-11-02). 004b was split from 004, whose draft review (B1 – B11) covered live playback before the split. Its own review is the post-implementation review (T008).

**Input**: Feature 004b of docs/workbench-plan.md: "The device engine moved into the output callback for a live playback with live controls, with the memory calls disabled meanwhile (they are not real-time safe)." It builds on 003 (the live core) and 004 (the device and the playback take).

## Clarifications

### Session 2026-11-02 (autopilot)

- **A clone, not a move.** The engine is `Clone`. A live playback runs on a clone of the device engine, made on the UI thread before the stream starts. That has three consequences:
  - the device and its memory stay on the UI thread;
  - nothing has to come back from the audio thread;
  - the memory calls stay **enabled** during a live playback, because they touch the device, not the clone.

  This restates the plan row's "memory calls disabled". The clone copies the store reserved at construction (1.8 MB with the speech coder on, 86 MB with it bypassed).

### Session 2026-11-02 (post-implementation review: dated restatements)

- **D5**: a refused list entry or command is not kept, so the twin of a live take is `play_to_take` with **the take's own changes** (B8). A command after the playback finished is refused (`NotPlaying`, B3).
- **D7**: **Play to take** is disabled during a live playback (B10). Record and the memory calls stay enabled, and the capture queues hold about 10 s of UI stall (B1). An overflowed take is marked incomplete and is not exported.
- **004 D4**: Play to take also runs on a clone (B5), so a change landed inside a take does not carry into the device.

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **The live player.** **Play live** on item N works as follows:<br>• **Setup**: the panel's five playback values are set on the device (004 D4), then the device's settings, the values, the item's digest and the clone are taken, all before `start_playback`.<br>• **Running**: the clone plays item N inside the output callback, in **whole 10 ms calls** (`play_with_changes`, 004's block) into a pre-allocated FIFO, as in 003 D1.<br>• **Rate**: the device's rate is the file's rate, and the output device's conversion applies.<br>Because the player's output does not depend on how its calls are split, the live output, change log and meter rows equal 004's offline take for the same values and changes. | **The clone in the callback, 10 ms calls, a FIFO** |
| D2 | **Live controls.** While live, the playback controls take effect at once:<br>• the plain live values: route, volume, battery, speed level and speed mode;<br>• the stage switches 10 – 13 (engine spec 021).<br>Each one goes through a fixed-capacity lock-free queue, and the callback schedules it on `Stream::Playback` at the stream's current position, tagged with the next sequence number. The engine refuses a stage that does not run in this configuration (`StageNotConfigured`), and a full queue refuses a command; both are shown in the engine's words, never dropped silently. | **The playback controls, at once** |
| D3 | **Scheduled changes.** The schedule list's playback-clock entries (004's `play_request`) are fed just in time by 003's routine, `Due`, on `Stream::Playback`, in the same 10 ms calls. | **003's list, on the playback stream** |
| D4 | **The take.** The take collects every sample the player produced, the drain included, plus the change log and the meter rows. It also keeps **every change scheduled, in the order the engine received it**, list entries and live commands alike, with its position, value and tag. A refused change is not kept. When the playback finishes and the FIFO is empty, the run ends by itself; Stop ends it earlier, and then the take holds what was produced. Either way, the take becomes the Device tab's playback take: 004's `PlayTake`, with the same playback sidecar and replay. | **A playback take, in scheduling order** |
| D5 | **Reproduction.** A finished live take equals `play_to_take` on the device with the take's start values and its changes, in output, change log and meter rows, bit for bit. It also replays from its playback sidecar and the exported stored form (004 D5). A take stopped early is marked **stopped early**, and its sidecar records the produced count, so a replay compares the prefix. | **Bit for bit, as 004** |
| D6 | **Real-time safety.** The clone, the FIFO, the buffers and the queues are allocated before the stream starts. The callback does not allocate, lock or do I/O. An overflow of a capture queue marks the take **incomplete**. | **As 003 D8** |
| D7 | **One output stream.** A live playback, 003's live run and 001's A/B playback exclude one another. There is no seek. | **One stream** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Play an item live and change it while it plays (Priority: P1)

**Acceptance Scenarios**:

1. **Given** an item and a schedule list, **When** it is played live with irregular callback sizes, **Then** the take equals `play_to_take` with the same values and list (`live_play_equals_offline`, seven rates).
2. **Given** a live playback, **When** route, volume, battery, speed and stage commands are sent between callbacks, **Then** each lands at the position the callback scheduled it, the take keeps it, and the offline twin with the take's changes reproduces the take bit for bit (`live_commands_reproduce`, seven rates).
3. **Given** a stage switch for a stage that is not running, **When** it is sent, **Then** it is refused in the engine's words and not kept (`live_refusal_reported`).
4. **Given** a live take, **When** its playback sidecar is replayed with the exported stored form, **Then** it reproduces (`live_take_replays`).
5. **Given** a live playback, **When** the callbacks run, **Then** they allocate nothing (`live_play_allocates_nothing`).

### Edge Cases

- **Stop before the end**: the take holds the samples produced so far. It is marked stopped early, and a replay compares the prefix.
- **A command after the playback finished**: refused by the engine (not playing), and reported.
- **An erase during a live playback**: it acts on the device, not the clone. The take names its item by number and digest at the start.

## Requirements *(mandatory)*

- **FR-001**: A live playback MUST follow D1 – D4 on a clone of the device engine.
- **FR-002**: A finished live take MUST reproduce offline and from its sidecar (D5).
- **FR-003**: The callback MUST be real-time safe (D6).
- **FR-004**: Every new string MUST pass the wording check.

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, live takes with live commands and a list reproduce bit for bit offline.
- **SC-002**: The live callback allocates nothing.
- **SC-003**: The owner's desk checklist: play an item live, change the speed, the route and the speaker stage while it plays, stop, export and replay the take.

## Assumptions

- The engine is pinned at `a06318c`.
