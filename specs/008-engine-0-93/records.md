# Records: engine 0.93 adoption

Recorded on 2026-11-03, with the engine at `3890bfe` (0.93.0).

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 197 green |
| reach | **74/74** (0.92's 70 and spec 026's four), asserted with a complete list |
| engine issues answered | #72, #73, #74, #75, #78, #79 (engine spec 026); #77 was the engine's Swift package |
| engine issue left | #76 (a public re-render): the workbench's recipe stays a mirror until engine spec 027 |
| engine issues filed | #82 (the one-pass map for output positions with the coder on), #83 (`meter_reached` reads the applied route) |

## Mirrors replaced

| Engine issue | The workbench's mirror | Now |
|---|---|---|
| #72 | `render::gate_reached`, a quoted rule | `Settings::one_pass_vas_runs` (agreed at every tap; `readings` checks the engine's rule against its behavior) |
| #73 | `render::tail_for`: latency + the player's drain, 0 where the latency is 0 | `Pipeline::one_pass_drain_samples` |
| #74 | `tests/wording.rs` parsed the engine harness's source | `rr_dr60::wording::{first_hit, first_claim_hit}` plus the workbench's prefixes |
| #75 | `meters::FIELDS` quoted from the rustdoc, `meters::reached`, `meters::route_of` | the engine's `meters::FIELDS`; `Pipeline::meter_reached` / `meter_lag_samples` on the render's engine |
| #78 | `render::codec_delay`, a crate-private rule | `latency_samples − one_pass_report_latency_samples` |
| #79 | the stage words by `{:?}` (`NotModeled`), the counts line by hand | `label()` |

## Where a mirror disagreed with the engine (findings)

- **The tail (#73).** The old tail was the player's drain, not the one-pass's, and it was 0 at the front taps. Stage 2 rings there (the engine's figure is 498 host samples at 48 kHz at `after_mic_amp`), and so does the band filter at `after_insert`. The default render's tail at 48 kHz moves from 1923 + 4208 to 7910 host samples. Renders are longer: a 0.92 sidecar's length does not reproduce at 0.93 (its engine commit differs anyway).
- **The battery factor (#75).** The old reach called `battery_ceiling` reached at every tap. The engine reaches it only at `route_output`; elsewhere it reads its rest value, which follows the battery setting, so the number shown is the same, now marked "not reached (rest)". Every other field agreed at every tap, route and switch combination tried (51 rows, all `battery_ceiling`).
- **The reports' clock (#78, the 005b follow-up).** With the coder on in drop mode, `ab::to_original`, `ab::to_render`, `ab::marks`, the meters' placement and the spectrum's spans used the full latency against the reports' positions. So a splice was counted the coder's delay early, and the marks sat that early. The coder's in-flight samples are held through the pause and come out at the resume, so a splice reaches the output at the report's position + C. Every map now uses the reports' clock (`render::output_to_input`), and `the_map_counts_a_splice_at_the_output` shows the hang's outputs map to silence. With the coder off nothing changes. Engine issue #82 asks the engine to document or publish this.
- **The reach before the first sample.** `meter_reached` reads the applied route, so on a fresh engine with the earphone route set and nothing processed it answers for the speaker. The workbench processes one silent sample on a throwaway engine first (`render::meter_reach_for`). This is engine issue #83.

## Reviews

The reviews ran inline in the implementing session, a forked worker that may not start review agents. They looked at the coach's questions (every decision has a test that fails on the old behavior where it differed: the tail at `after_mic_amp`, the A/B map with the coder on, the stage labels, reach 74) and the hardware reviewer's (the clocks of each map, the held coder samples, the device split pinned against the engine's lag at seven rates). **They are not an independent review; a `speckit-coach` and `hw-emulation-reviewer` pass on this PR is a follow-up.**

## Lessons

- With the coder on in drop mode, the one-pass holds the coder's delay of pre-pause samples through the pause. A splice reaches the output C after its report, inside the call that resumes. A rule keyed on call boundaries (the meters' plot break) must key on the output position.
- A reach read on a fresh engine sees the plain setters not yet landed.
- A synthetic render that clears the events must also clear the coder's delay.
