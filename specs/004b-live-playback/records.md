# Records: live playback

Recorded on 2026-11-02, with the engine at `a06318c`.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 121 green, 3 ignored |
| reach | 41/70 (unchanged: 004b calls no new Pipeline method) |
| `playback_check` (the silent device checks) | live playback at 48 kHz: the player produced 24 960 samples in 0.5 s (about 24 000), device not lost |

## Figures

- **`live_play_equals_offline`**: at seven rates, with irregular callback sizes (64 … 4500 frames, one above the 4096-frame part), a live take with a five-entry list equals `play_to_take` in samples, log, meter rows, frames and place. That is 13 007 samples at 6 kHz up to 209 055 at 96 kHz.
- **`live_commands_reproduce`**: at seven rates, nine commands are sent while the item plays: speed level, route twice, volume, battery, the speaker stage off and on, stage 10 off, and the speed mode. All are accepted and logged. The take differs from the list alone, and the offline twin with the take's changes reproduces it bit for bit.
- **`live_refusal_reported`**: the speaker stage, off at construction, is refused in the engine's words (`StageNotConfigured`) and not kept.
- **`live_take_replays`** and **`live_stopped_early`**: the playback sidecar replays a live take, and the prefix of a take stopped early (at 6 and 48 kHz).
- **`memory_stays_usable_while_live`**: an erase during a live playback acts on the device, and the take is unaffected.
- **`live_play_allocates_nothing`**: no allocation or free in the callbacks at 6 and 48 kHz, with commands, a refusal and a 20-entry list.
- **`live_ties_reproduce`**: at 6 and 48 kHz, a volume list entry sits on every 10 ms boundary, and a volume command is sent at every callback. The commands tie with list entries and still reproduce. Every command lands on a call boundary at the position it was scheduled.
- **`command_after_finish_refused`**, **`incomplete_take_not_exported`** and **`takes_leave_the_device_alike`**: see the review below.

## Post-implementation review (T008, B1 – B10), applied

The verdict was "sound with fixes". The reviewer's stress probe found no break in the central claim: 4 rates × 5 seeds, with random commands tied to list entries, the 64-pending limit, and stops with a change pending.

- **B1**:
  - The capture queues now hold about 10 s of UI stall.
  - A take whose capture overflowed carries `incomplete`, and `export_take` refuses it.
- **B2**: refusals are shown as they arrive, whichever tab is open. The drain leaves the replies to `take_replies`. A refused widget returns to the value in force.
- **B3**: a command after the playback finished is refused (`NotPlaying`) and not kept.
- **B4**: a finished render stops only the A/B (`stop_ab`). Starting another stream or opening a file stops everything.
- **B5**: **restatement of 004.** Play to take now runs on a clone too, so a change that lands inside a take no longer carries into the device. Both modes leave the device the same. 004's `playback_start_values_recorded` is restated: the values set on the device are in force at the next start.
- **B6**:
  - The scheduling order is position order by construction; it is asserted (`debug_assert`), and the stable sort is kept as a guard.
  - New: the tie test and the call-boundary assertions.
  - The early-stop test now compares the meter rows.
  - The allocation test now covers a 70-command burst, including `ScheduleFull`.
- **B7**: a prefix replay stops at the prefix, so its frames and place match the live take.
- **B8**: every refused list entry gets a reply. The twin of a live take is `play_to_take` with the take's own changes.
- **B9**: the live widgets follow every landed change, from the list or a command.
- **B10**: Play to take is disabled during a live playback. Record stays enabled, now covered by the 10 s queues.
