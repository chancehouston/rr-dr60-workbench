# Feature Specification: Engine 0.94 adoption (the engine's re-render session replaces the copied recipe)

**Feature Branch**: `010-engine-0-94`

**Created**: 2026-11-04

**Status**: Under the owner's autopilot rule (2026-11-02); the owner asked for it on 2026-11-04 ("keep going with Workbench issue #23 on autopilot").

**Input**: Workbench issue #23. Engine spec 027 (rr-dr60-emu PR #84, merge `ad1ba8d`, version 0.94.0, engine issue #76) publishes the re-render: `rr_dr60::rerender::{Plan, ReRender}` (an editable plan read from a processing record, a streaming session, the digest, `matches_original` and a verdict) and `Pipeline::fill_to_remaining`. Since spec 005, the workbench's `records.rs` has kept its own copy of the engine's recipe: the fill by import of a filler recorded on a throwaway engine, the slot restore, the schedule refilled below 64 pending, the stop rule.

## Decisions (autopilot; the agent's defaults)

| # | Decision | Default |
|---|---|---|
| D1 | **The pin** moves to `ad1ba8d` in its own commit with a CHANGELOG line (the standing rule). | Done first |
| D2 | **The re-render is the engine's session.** `records::Prepared` carries the engine's `Plan`; `records::re_render(plan, fed, progress, cancel)` builds a `ReRender`, feeds the session's input with `feed_f32` in 10 ms feeds (the session splits a feed at the next row), checks the cancel between feeds and finishes. `Recipe`, `filler`, the refill closure and the stop rule are deleted. The refusals the workbench made before the session (an imported origin, an unknown component) stay in `records::plan_of`, with the same words. | Engine session |
| D3 | **The outcome** carries the engine's verdict (`Identical`, `Differs`, `Rebuilt`), `matches_original` and `rows_landed`; `reproduces()` is `matches_original`. The UI line shows the verdict in words (`Rebuilt, matching the original` for an edited plan that gives the original's digest) and the rows that landed. | Engine verdict |
| D4 | **A short input is refused.** The engine refuses an input that ends before the record's stop (`InputShort`). The old copy stopped early and reported a difference. `ReRenderError::InputShort(n)` replaces `Fill`, with a sentence. | Engine rule |
| D5 | **The four `Pipeline` methods the copy reached get a real use each**, because constitution II requires every method to be reachable: `fill_to_remaining` (new in 0.94) as the memory view's **Fill the memory**, leaving N s (for exploring a nearly full memory and the self-stop); `encoding` on the memory status line; `new_at_counter` as **Reset device at counter** (an empty device whose next recording uses that counter's hiss stream); `record`, the plain call, which the device uses when no record-side change is scheduled or pending (the change log is empty either way; `RecordOutcome.plain_call` says which call ran, and a twin test pins it against the logged call). The fill and the reset at a counter are labelled additions of the emulator (constitution VI). | One use each |
| D6 | **The deltas test** edits the plan through its setters (`set_seed`, `set_counter`, `set_settings`, `set_row`, `set_component`, `set_parameter`, `set_remaining_samples` for "no fill"). The stop has no setter, so "the stop one frame earlier" edits the record's line and re-signs the checksum (the engine's own test does the same); that record is unedited as a plan, so its verdict is `Differs`. | Plan setters |
| D7 | **The settings diff** keeps its own slot restore on its small check engine (it reads the slot controls by `control_value`), now private and documented as the diff's helper. The re-render no longer uses it. | Kept, private |

## Requirements

- **FR-001**: D1 – D7, each with a test; the seven-rate digest tests (`re_render_reproduces`, the deltas, the slot, the offset, the refill past 64 pending, the live-value twins) unchanged in what they assert (engine spec 027 SC-004). The copied filler's own test (`filler_has_the_length`) goes with the copy: the filler's length is the engine's, tested by engine spec 027 (its `fill_rounded_up` mutant), and the workbench's call is tested by `fill_and_encoding`.
- **FR-002**: No copy of the recipe in `workbench_core`: no filler, no schedule refill, no stop rule in `records.rs` (a source scan).
- **FR-003**: Reach stays complete: 75/75 (0.93's 74 and `fill_to_remaining`).

## Success Criteria

- **SC-001**: `cargo test --all-features` green; reach 75/75.
- **SC-002**: The scan of FR-002 (`no_recipe_copy`) finds none of `fn filler(`, `pending_changes(Stream::Record)`, `stop_recording(`, `schedule_change(`, `.import(`, `new_at_counter(`, `struct Recipe` in `records.rs`, and finds the session's calls `ReRender::new(` and `.feed_f32(`.
- **SC-003**: The owner's desk checklist (`quickstart.md`) runs on the Mac.
