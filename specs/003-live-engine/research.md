# Research: the live engine

The engine is pinned at `a06318c`. The reviews' engine checks are cited from their reports (coach and hardware, 2026-11-02).

## R-01 One routine for offline and live

**Decision.** `render.rs` gains a shared step, `run_chunk(p, chunk_in, due, cursor) -> ChunkOut`, used by both the offline render and the live core. It does three things:

1. It schedules every listed change due before the chunk's end in its own clock. The input clock uses `input_position(OnePass) + n`, the output clock `stream_position(OnePass) + n`, an upper bound because produced ≤ fed. Changes are scheduled in list order.
2. It calls `process_with_events_and_changes` on one 10 ms chunk.
3. It returns the produced samples, the reports, the change entries and the meter row.

**Why.** The engine's output does not depend on the partition. Positioned changes land on their sample at any block size (engine spec 015). One routine on the same chunks therefore makes the live take and its re-render identical by construction, and the tests prove it.

## R-02 The request for a range and its changes

`RenderRequest` gains two fields:

- `start: u64` (S);
- `changes: Vec<Scheduled { clock, position, control, value: f64, tag }>`.

A live take's request holds `input = input[S .. S + fed]`, zero-padded past the file's end, with `tail = false`. The tail it fed is already part of `fed`. A re-render of that request makes the same calls. `Render` gains `log: Vec<ChangeEntry>` and `live: Option<LiveInfo { start, fed, skips: Vec<(u64, u64)>, incomplete: bool, tail_switch: bool }>`. 001's render now also collects the log: its tag-0 entries come from the start's plain setters, and are identical in a re-render.

## R-03 The live core (D1, D4, D8)

`LiveCore` holds:

- the `Pipeline`;
- the input `Arc<[f32]>`, with `pos` and `end` (input length + tail when the tail switch is on);
- the FIFO (a ring of 4096 + 2 chunks);
- the chunk buffers;
- the events buffer (`max_events(chunk)` at the hang's 50 ms minimum);
- the changes buffer (`MAX_CHANGES_PER_CALL`);
- the due list and its cursor;
- the queues (rtrb, MIT/Apache): commands, replies, captured samples, entries, events, rows and skips;
- the counters.

`fill(out)`, per callback:

1. Drain the commands. Each is scheduled at the current position in its clock with the next tag, and its `Result` (`Error` is `Copy`) goes to the reply queue.
2. While the FIFO holds less than `out.len()`, the budget (4 × `out.len()` input samples) is not spent and `pos < end`: run one chunk, push its output to the FIFO and the capture queues, and assert produced ≤ fed.
3. Copy from the FIFO. The remainder is zeros: a **skip**, recorded at take position `produced_total` with its length, or the end of the run (`finished`).

A full capture queue or an overflowing report buffer sets `incomplete`.

## R-04 Disabled controls (D2)

When the run starts, a check engine built like the live one tries each live-timing control. It schedules the control's current value at position 0 on `OnePass`, records the error kind, then calls `clear_schedule`. The engine's refusals map to the reasons: `ControlNotOnStream` gives "not on the one-pass", `StageNotConfigured` gives "stage not running in this configuration", `TapNotOnRoute` gives "tap not on this route". A band-filter parameter with the slot empty is "slot empty", read from the panel's slot value. An idle control is "idle: stop to change".

## R-05 Sidecar version 2 (D5)

The magic line is `rr-dr60-workbench sidecar 2`. Version 2 adds the keys `live`, `start` and `fed`, and change lines of the form `change <in|out> <position> <identifier> <decimal> 0x<16 hex f64 bits> <tag≥1>`. A reader accepts versions 1 and 2, and refuses a change line with tag 0 or with decimal and bits that disagree. Restoring builds the request from the start values, the range and the changes. The contract is in [contracts/sidecar-v2.md](contracts/sidecar-v2.md).

## R-06 CoreAudio

`playback.rs` generalizes `Player::start` to take a fill closure `FnMut(&mut [f32], &mut [f32]) + Send + 'static`. 001's `monitor::fill` becomes one such closure, and the live core another, which writes its mono buffer to both channels. Stop drops the unit on the UI thread, which drops the closure and the core. The device-loss and overload handling is 001's.

## R-07 Timing (SC-004)

Release build, 48 kHz, coder on, drop mode at its budget: the worst `fill` of 64 and 512 frames, measured over 60 s of simulated callbacks, against 25 % of the buffer's duration. The engine's records predict about 11 – 15 % at the worst 64-sample block.
