# Records: the live engine

Recorded on 2026-11-02 on the development Mac, with the engine at `a06318c`.

## The gate

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 91 green, 3 ignored (the release timings) |
| reach | **20/70** |
| live callback allocations (`live_alloc`, up to 10⁵ callbacks of 1 – 5000 frames with commands) | 0 allocations, 0 frees |
| SC-004, release, 48 kHz, coder on, drop mode at its budget | worst **296 µs = 22.2 %** of a 64-frame callback (mean 1.37 %); worst 252 µs = 2.4 % of 512 frames (mean 1.06 %) |
| Silent device check (`playback_check`) | the live core produced 24 000 samples in 0.5 s at 48 kHz; 0 overloads |

The 64-frame figure is tight by design. Each engine call is a whole 10 ms chunk, so that the meter rows equal 002's (coach 7), and a 64-frame callback that needs a new chunk does a chunk's work. At macOS's usual 512 frames the worst case is 2.4 %.

## Figures

- **`live_equals_render`**: at seven rates, a run starting a quarter second into the file with seeded callback sizes (1 … 6000 frames) equals the offline render of the same range in samples, gate reports, change log and meter rows.
- **`live_take_reproduces`**: at seven rates, a take with 12 seeded commands (volume, gate threshold, route, band-filter frequency) and a scheduled change, stopped by hand at callback 30, re-renders bit for bit from its version-2 sidecar, with 9 – 11 changes landed. The sidecar's change lines carry the exact values and bits.
- **`schedule_limit_refused`**: 70 changes due at one position. The engine accepts 64 and refuses 6, in its own words.
- **Reasons**: the speech coder is "idle", the playback speed "not on the one-pass", a band-filter parameter with the slot empty "slot empty", the route at the after-speaker tap "tap not on this route", and the speaker switch when the stage is off at the start "stage not running".

## Restatements during implementation

- **The queue sizes** are a second's stall plus a worst-case callback. The first sizing (rows 128) overflowed at 6 kHz, where one 6000-frame callback runs up to 400 chunks under the budget. The reproduction tests now assert a complete take.
- **A listed change the engine refuses** is reported and skipped while live, not a stop. A re-render treats a refusal as a hard error (plan review B3).
