# Records: coverage close-out

Recorded on 2026-11-03, with the engine at `a06318c`.

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 195 green, 3 ignored |
| `tests/checks.rs` | 10 green |
| reach | **70/70**, asserted, with a complete list (constitution II met) |
| engine issue | #79 (the manifest's words for status, path and confidence) |

## Figures

- **`one_pass_forms_agree`**: at seven rates, in drop and mute modes, with a knee change (input clock) and a volume change (output clock) that both land, and a pause the gate takes.
  - All six call forms agree with the render per 10 ms call: output bits, `BlockInfo` counts and meters. The forms that report them also give the same gate events and change log.
  - The reset render agrees with its fresh-from-state oracle.
- **`reset_keeps_the_landed_values`**:
  - The reset render differs from the first (it starts from the landed volume and knee), and the oracle agrees.
  - With a band-filter slot and a speaker-off landing, the oracle's position-0 landing agrees at 6 and 48 kHz.
- **`in_place_refusal_reported`** and **`refusal_leaves_the_clocks`**:
  - With a named route tap of the other route, the returning forms give `TapNotOnRoute` and leave both clocks.
  - The in-place forms are classified refused.
  - A drop-mode pause is not classified refused.
- **`playback_forms_agree`**:
  - The four takes (`play` and `play_with_changes`, by index and by the plain setters) are bit-identical and equal to 004's play to take.
  - The getters read the values set, the device is unchanged, and the scheduled volume change landed.
  - An empty memory and an out-of-range option are refused.
- **`readings_by_their_calls`**:
  - At seven rates, codec off and on, the pre-encoder latency equals the record latency less 180 · rate / 6000 exactly.
  - The slot's own calls equal the by-index readings.
- **`tour_follows_the_manifest`**:
  - 16 pages (15 stages and the stage-0 page) partition the 36 controls.
  - Stage 4 has two switches; stages 8, 9 and 15 none.
  - The page logic holds no literal.
- **`compare_without_stage`** and **`_no_effect_and_refused`**:
  - Stage 11's switch is turned off and its scheduled entry left out; the diff lists exactly that control and the removed entry.
  - Stage 13 on the speaker route has no effect.
  - A stale render, a switchless stage, and switches already off are refused.

## Reviews, applied

- **Draft** (coach 1 – 10, hardware B1 – B7): the spec's first Clarifications session.
- **Post-implementation** (coach 1 – 12, hardware B1 – B6): the spec's second session.
  - **Coach**:
    - **1**: the tour's request is the one rendered.
    - **2**: no effect is reported.
    - **3**: the verdicts are shown.
    - **4**: the checks run on threads, with cancel.
    - **5, 6**: the reach scan.
    - **7**: stage 9's pointer is dropped.
    - **8**: the gate notice is keyed on the request.
    - **9**: the readings are exact.
    - **10**: the clocks on refusal.
    - **11**: the reset oracle with the slot and a switch.
    - **12**: the empty memory.
  - **Hardware**:
    - **B1**: the reset oracle's landed switches.
    - **B2**: the latency offset.
    - **B3**: the window's reports.
    - **B4**: the plain setters' options.
    - **B5**: the landings.
    - **B6**: exact readings.
