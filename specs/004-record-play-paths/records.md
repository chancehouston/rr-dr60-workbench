# Records: the record and play paths

Recorded on 2026-11-02, with the engine at `a06318c`.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 105 green, 3 ignored |
| reach | **41/70** (SC-003: at least 40) |
| launch | the window runs with an empty log |

## Figures

- **`recording_partition_twin`**: at seven rates, the device's item (recorded in 10 ms calls) has a stored form byte-identical to a direct engine's, fed in one call and in irregular calls (1, 997, 64, 4801).
- **`playback_take_reproduces`**: at seven rates, a playback take with ten seeded changes (speed level, route, volume, the speaker stage's switch, speed mode), all ten landed, replays bit for bit, output and log, through its playback sidecar and the exported stored form on a fresh device. That is 9 230 samples at 6 kHz up to 228 255 at 96 kHz, each with 101 frames.
- **`playback_partition_twin`**: the device's 10 ms playback equals a direct engine's playback in one large call.

## Restatements

- **001 D4**: rows not on the one-pass (the memory, the playback speed and its mode) are now **settable**, still labelled, because the device uses them. A one-pass render ignores them, and a sidecar restore applies them. The one-pass sidecar does not carry the playback speed; the playback sidecar records it.
- **The spec's second oracle** (a recording's playback equal to the one-pass under engine spec 008's recipe) is the engine's own, proved in its harness with the gate off or open at the end. The workbench's oracles are the direct-engine twins above, which hold in drop mode too.
