# Records: the record and play paths

Recorded on 2026-11-02, with the engine at `a06318c`.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 108 green, 3 ignored |
| reach | **41/70** (SC-003: at least 40) |
| launch | the window runs with an empty log |

## Figures

- **`recording_partition_twin`**: at seven rates, the device's item (recorded in 10 ms calls) has a stored form byte-identical to a direct engine's, fed in one call and in irregular calls (1, 997, 64, 4801).
- **`playback_take_reproduces`**: at seven rates, a playback take with ten seeded changes (speed level, route, volume, the speaker stage's switch, speed mode), all ten landed (asserted), differs from the same take without changes, equals an independent direct engine (every change scheduled before `start_playback`, 4096-sample calls) in output and log, and replays bit for bit through its playback sidecar and the exported stored form on a fresh device. That is 9 230 samples at 6 kHz up to 228 255 at 96 kHz, each with 101 frames.
- **`playback_partition_twin`**: the device's 10 ms playback equals a direct engine's playback in one large call.

## Restatements

- **001 D4**: rows not on the one-pass (the memory, the playback speed and its mode) are now **settable**, still labelled, because the device uses them. A one-pass render ignores them, and a sidecar restore applies them. The one-pass sidecar carries the speed mode and the memory controls but not the speed level; the playback sidecar records the level.
- **The spec's second oracle** (a recording's playback equal to the one-pass under engine spec 008's recipe) is the engine's own, proved in its harness with the gate off or open at the end. The workbench's oracles are the direct-engine twins above, which hold in drop mode too.

## Post-implementation review (T009, B1 – B10), applied

- **B1**: every error path in a recording abandons it (`abandon_recording`), and a playback error stops the playback. A refused record-side change leaves no item and an idle engine (`failed_recording_leaves_memory_unchanged`). The five values are restored if one setter fails.
- **B2**: `replay` returns a `ReplayReport` that checks the stored form's SHA-256, the produced count and the output's SHA-256 against the sidecar. A changed stored form is noticed.
- **B3**: the oracle for `playback_take_reproduces` is a direct engine, independent of `Device` (above).
- **B4**: Export take uses `export_take`. It refuses when the item number now names another recording (by digest, after an erase renumbered the items). A **Replay a playback sidecar…** button reads the stored form named in the sidecar, from the same folder, and shows each check.
- **B5**: `read_playback` is strict. Unknown keys, a missing setting, a missing value, a repeated key and an unknown control are refused (`clocks_and_guards`).
- **B6**: a change in the wrong clock is refused before the start (`check_clocks`).
- **B7**: `SelfStopped` carries the item's length.
- **B8**: `record_changes_twin`: record-side changes against a direct engine that schedules them before one call; the stored forms are byte-identical. The outcome's `fed` is the stop's `stop_input_position` (the stop's own silence excluded, engine spec 020 D9).
- **B9**: the record and play calls take the schedule list through `record_request` and `play_request`. Readings are for the item last clicked. Listen is labelled "take only".
- **B10**: the round-trip test's comments state that the speed mode round-trips and the level does not.
