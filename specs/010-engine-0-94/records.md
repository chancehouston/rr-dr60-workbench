# Records: engine 0.94 adoption

Recorded on 2026-11-04, with the engine at `ad1ba8d` (0.94.0).

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 205 green (0.93 adoption: 201) |
| reach | **75/75** (0.93's 74 and spec 027's `fill_to_remaining`), asserted with a complete list |
| engine issue answered | #76 (the public re-render, engine spec 027) |
| workbench issue closed | #23 |

## The copy replaced

| Before (spec 005's copy of the recipe) | Now |
|---|---|
| `Recipe`, read from the read-out and the change rows | the engine's `rerender::Plan` (`records::plan_of`, the two refusals first) |
| the fill: a filler recorded on a throwaway 6 kHz engine, exported and imported | done by `ReRender::new` (the engine's `fill_to_remaining`, an import-shaped filler written directly) |
| the slot restore at every component's offset | done by `ReRender::new` |
| the schedule refilled below 64 pending after every 10 ms call | the session splits a feed at the next unscheduled row, so no row lands late at any block size |
| the stop rule (no stop call after a self-stop; a stop call otherwise, also on a short input) | `ReRender::finish`; a short input is now refused (`InputShort`) |
| `Outcome { digest, expected }` | the engine's verdict, `matches_original`, `rows_landed` |

## Findings

- **A short input.** The copy stopped wherever the session's input ended and reported "Differs". The engine refuses an input that ends before the record's stop, so the message now names the host samples missing. Nothing else changed in the result: every seven-rate digest test holds unchanged in what it asserts.
- **Reach.** The copy was what reached `encoding`, `new_at_counter` and the plain `record`. With it gone, each got a use of its own (D5): the memory line's encoding, **Reset device at counter**, and the plain call for recordings with no record-side change. `fill_to_remaining` (new in 0.94) is the memory view's **Fill the memory**.
- **The plain call's condition.** With no change scheduled and none pending on the record stream, the record log is empty, so `record` and `record_with_changes` are twins. The one-pass's values reach the recorder by `adopt` at the start, with no entry (engine spec 017). `plain_record_twin` asserts which call ran and that the stored forms and reports are equal.
- **The filler's record.** A filler item has no counter, so its processing record reads as imported, and the engine's session refuses it (asserted).
- **Engine issue #83** (`meter_reached` read the applied route) is fixed on the engine's branch `fix-083-meter-reached-route`, not yet merged. `render::meter_reach_for`'s throwaway sample stays until the pin reaches that fix.

## Reviews

The `speckit-coach` post-implementation review (twelve findings, none high) was applied before the PR:
- `plain_record_twin` could not fail with the old path: `RecordOutcome.plain_call` is now asserted.
- The plain flag is read before `start_recording`, so an error leaves no recording.
- The decision tags were made the spec's D5.
- The fill's doc was wrong about rounding. A 6001-sample case now shows the capacity rounding up with the coder on.
- `Outcome.digest` is a `u64`, because a stop that kept nothing is an error.
- The `Imported` mapping is covered by the filler's record.
- The verdict words are asserted for all three verdicts, and the threshold delta for `edited` and `Rebuilt`.
- The removed filler test is recorded under FR-001.
- The two new controls are labelled additions of the emulator.
- A desk checklist is in `quickstart.md`.
- SC-002 matches the scan.

The hardware reviewer was not run: the feature shows no new readings.

## Lessons

- A twin test of two code paths that give equal results cannot show which path ran. Expose the choice and assert it.
- Removing a copy can un-reach the methods only the copy called. Give each one its own use rather than a dummy call.
