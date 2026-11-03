# Feature Specification: Engine 0.93 adoption (the engine's host reads replace the mirrors)

**Feature Branch**: `008-engine-0-93`

**Created**: 2026-11-03

**Status**: Under the owner's autopilot rule (2026-11-02); the owner asked for it on 2026-11-03 ("yes" to the workbench adopting 0.93.0). The draft and post-implementation reviews ran inline in the implementing agent's session (a forked worker, which may not start review agents); their findings are recorded in records.md.

**Input**: Engine spec 026 (rr-dr60-emu PR #80, merge `3890bfe`, version 0.93.0) answered the engine issues the workbench filed for its mirrors: #72 (whether the gate runs), #73 (a one-pass drain figure), #74 (the wording lists from the core), #75 (the meter descriptions, reach and lag), #78 (the latency the gate reports align with), #79 (the stage manifest's words). #77 was the engine's Swift package (the workbench already restored the slot at every component's offset). #76 (a public re-render) is engine spec 027: the workbench's recipe stays a mirror until then.

## Decisions (autopilot; the agent's defaults)

| # | Decision | Default |
|---|---|---|
| D1 | **The pin** moves to `3890bfe` in its own commit with a CHANGELOG line (the standing rule). | Done first |
| D2 | **Whether the gate runs** (#72): `render::gate_reached` becomes `Settings::one_pass_vas_runs`. The behavior test (`readings`) stays and now checks the engine's call. | Engine call |
| D3 | **The tail** (#73): the render's tail becomes `Pipeline::one_pass_drain_samples` (the engine's bound, impulse rule). It replaces "latency + playback drain, 0 where the latency is 0". This changes the render's length: the old tail was the player's drain, not the one-pass's, and was 0 at the front taps where stage 2 and the slot still ring. The tail's label and the sidecar's `render.tail` words follow. | Engine figure; the length change recorded |
| D4 | **The wording check** (#74): the test reads `rr_dr60::wording::{first_hit, first_claim_hit}` instead of parsing the engine harness's source; the workbench's supplementary list stays its own. | Both engine tiers |
| D5 | **The meters** (#75): each field's identifier, unit, kind and sentence come from `rr_dr60::meters::FIELDS`; reach and lag from `Pipeline::meter_reached` / `meter_lag_samples` on the render's own engine (stored with the render and the live run). The workbench keeps only the value accessors and its device-rate split of the input side (fields whose record-call lag is positive), pinned against the engine. On screen the sentence is the engine's, no longer a "workbench note". | Engine table and reads |
| D6 | **The reports' latency** (#78): `codec_delay` becomes `latency_samples − one_pass_report_latency_samples`. The 005b follow-up is closed with it: every map of an output position through the gate's reports (A/B, the meters' placement, the spectrum's spans) uses the reports' clock, `onepass_output_to_input(o − C, L − C, …)`, so a splice is counted when it reaches the output, not the coder's delay early. With the coder off nothing changes. | Engine figure; the map fixed |
| D7 | **The stage words** (#79): status, path and confidence show the engine's `label()` (the tour and the stage panels), the status counts line uses the labels. | Engine labels |
| D8 | **The reach test** counts the four new `Pipeline` methods (74 in all) and asserts every one reached. | 74/74 |

## Requirements

- **FR-001**: D1 – D8, each with a test that fails on the old mirror's behavior where it differed from the engine, and holds the old behavior where it agreed.
- **FR-002**: Every mirror the engine now publishes is removed from `workbench_core`; #76's recipe stays, labelled as waiting for engine spec 027.
- **FR-003**: Disagreements between a removed mirror and the engine are recorded in records.md as findings.

## Success Criteria

- **SC-001**: `cargo test --all-features` green; reach 74/74.
- **SC-002**: The six engine issues' mirrors are gone (a source scan finds no `gate_reached` rule, no harness-source parsing, no quoted meter table, no crate-private latency rule).
