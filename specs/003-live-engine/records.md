# Records: the live engine

Recorded on 2026-11-02 on the development Mac, with the engine at `a06318c`.

## The gate

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 96 green, 3 ignored (the release timings), after the post-implementation review |
| reach | **20/70** |
| live callback allocations (`live_alloc`: 6 and 48 kHz, callbacks of 1 – 6000 frames, a 70-entry list in both clocks, commands in both clocks, pauses) | 0 allocations, 0 frees; changes landed 73 / 90, skips 81 / 280 |
| SC-004, release, 48 kHz, coder on, drop mode at its budget | worst **296 µs = 22.2 %** of a 64-frame callback (mean 1.37 %); worst 252 µs = 2.4 % of 512 frames (mean 1.06 %) |
| Silent device check (`playback_check`) | the live core produced 24 000 samples in 0.5 s at 48 kHz; 0 overloads |

The 64-frame figure is tight by design. Each engine call is a whole 10 ms chunk, so that the meter rows equal 002's (coach 7), and a 64-frame callback that needs a new chunk does a chunk's work. At macOS's usual 512 frames the worst case is 2.4 %.

## Figures

- **`live_equals_render`**: at seven rates, a run starting a quarter second into the file with seeded callback sizes (1 … 6000 frames) equals the offline render of the same range in samples, gate reports, change log and meter rows.
- **`live_take_reproduces`** (strengthened by the review), at seven rates:
  - each run is stopped after 3 s of input, with the gate pausing (3 reports), a listed change at its position, and seeded commands in both clocks (14 – 41 changes landed);
  - it is written as a version-2 sidecar, restored in a fresh session from the file, and re-rendered bit for bit in samples, reports, log and meter rows.
- **`restored_range_survives_second_generation`**: a take, a re-render of it, and a second restore all reproduce the take's digest.
- **`schedule_limit_refused`**: 70 changes due at one position. The engine accepts 64 and refuses 6, in its own words.
- **Reasons**: the speech coder is "idle", the playback speed "not on the one-pass", a band-filter parameter with the slot empty "slot empty", the route at the after-speaker tap "tap not on this route", and the speaker switch when the stage is off at the start "stage not running".

## Restatements during implementation

- **The queue sizes** are a second's stall plus a worst-case callback. The first sizing (rows 128) overflowed at 6 kHz, where one 6000-frame callback runs up to 400 chunks under the budget. The reproduction tests now assert a complete take.
- **A listed change the engine refuses** is reported and skipped while live, not a stop. A re-render treats a refusal as a hard error (plan review B3).

## Post-implementation review (B1 – B15), applied before the PR

1. The list is sorted before it is tagged (`schedule_unsorted_lands_exactly`).
2. An engine failure stops the run, with its words.
3. No panic in the callback.
4. Ranged sidecars survive a second generation.
5. The tail switch as used.
6. The panel takes the end values (`take_tail_switch_and_end_values`, `live_take_accepted_with_its_end_values`).
7. The stronger reproduction test.
8. The wider allocation test.
9. Lost replies counted separately.
10. The device-lost message kept.
11. The stop waits for the FIFO to drain.
12. The monitor volume on live playback.
13. D7: every field with its reach rule (D6 restated as a list).
14. Unknown change identifiers listed, with no range set.
15. Records and the label.

## Convergence

`/speckit-converge` (2026-11-02) checked 11 FRs, 5 SCs, 10 acceptance scenarios, 7 edge cases, D1 – D9, P1 – P6 and the six principles against the code and the tests, and found no gap: ✅ converged. SC-003's audible half and SC-005 are the owner's desk checklist.
