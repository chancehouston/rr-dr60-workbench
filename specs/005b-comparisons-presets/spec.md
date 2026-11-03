# Feature Specification: Comparisons and presets

**Feature Branch**: `005b-comparisons-presets`

**Created**: 2026-11-03

**Status**: Under the owner's autopilot rule (2026-11-02). It starts from 005's draft decisions D5 – D9 and the findings of 005's draft reviews on them (`draft-decisions.md`). The spec-and-plan reviews (coach 1 – 13, hardware Q1 – Q10) are applied; their questions take their defaults.

**Input**: Feature 005b of docs/workbench-plan.md (split from 005 on 2026-11-03): "Two renders side by side with a null test, a spectral difference and a parameter diff. Settings presets saved as files." It builds on 001 (the render, its request and sidecar, the A/B) and 002 (the spectrum).

## Clarifications

### Session 2026-11-03 (autopilot, the spec-and-plan reviews)

- **The shift's sign**: s = (L_B − L_A) − (S_B − S_A), from S_A + i − L_A = S_B + j − L_B. `draft-decisions.md`'s "+ (S_B − S_A)" is superseded (coach 12; checked by the hardware review: the flipped sign nulls at +2.9 dB).
- **A live take as B** is allowed. Its budget skips are silence that is not engine output, and every later sample is shifted against the engine's map, so the spans and the spectrum's runs end at the first skip (labelled), and an incomplete take is labelled (coach 3).
- **SameSeconds with different starts** switches by input time: the input position is S_X + o (coach 4). An input before Y's start maps to Y's start; the switch back then lands at Y's start, not where it came from (an accepted asymmetry).
- **Loading a preset** clears a restored live range and its schedule, leaves A stale by the panel's change and B untouched, and is refused with no input open (coach 6).
- **Change values** compare by their f64 bits, as scheduled; −0.0 and 0.0 differ there. Among the controls, `ControlReading` equality treats them as equal (coach 7, hardware Q10).
- **The gain oracle's tolerance** (hardware Q1): 0.001 dB on the bins within 60 dB of both sides' maxima. Deeper bins in the 120 dB window reach the f32 narrowing floor, about 0.03 – 0.04 dB; that figure is reported, not asserted.
- **The latency term** has no engine-identical oracle (the codec changes every sample: hardware Q2, coach 1). It is tested on hand-built renders. The start term is tested on the engine, on the quiet front with a start on the conversion grid. The codec-on-against-off case asserts s and reports the figures.
- **Different starts** give a null floor from the setup (the hiss streams, the converter's phase, the coder's frame grid), so the view says so whenever the starts differ (hardware Q3).

### Session 2026-11-03 (post-implementation reviews: dated restatements)

- **Live skips** (hardware B3, restating the session above): a live take's samples are continuous engine output; the budget's silence went to the device only. So nothing is cut at a skip. The first skip is shown as a label, and an incomplete take is labelled.
- **The gate reports** map to the input with the latency less the coder's delay: the reports' positions exclude it (hardware B2). The rule for when the latency includes it is the engine's, crate-private, so the workbench mirrors it (engine issue #78). The prefix ends before A's coder delay: o_B maps to o_B + C_B − C_A − s. With the coder on, that is conservative by the delay, which covers the frame the coder decides it in.
- **A pause still open at the end** reports no splice. When one side ends paused and the other does not, the prefix ends at the paused side's end, and the view labels it (hardware B1).
- **Keep as B, and every new render, stop the A/B playback**: the takes are fixed when the playback starts. Keep as B says so (coach 5).
- **The A/B switch** cycles original → A → B (when kept) → original (coach 11).
- **Load preset** is refused while a live take runs (coach 2).
- **The Compare tab** recomputes when either request changes, not only either output (coach 1).

## Decisions (in force under autopilot)

| # | Decision | In force |
|---|---|---|
| D1 | **A and B.** **Keep as B** keeps the session's current render as the comparison render **B**. The current render is **A**, and it moves on with every new render; B does not change. A live take may be kept as B. B is dropped when the session drops its render (another file opened). A **Compare** tab shows A and B, their requests and the three comparisons below. | **B kept, A current** |
| D2 | **The parameter diff.** For every control, in manifest order and by identifier, the value each render was made with, as an engine reading (the setting from the request's settings, otherwise the request's engine value). A row is listed when the two readings differ, with both values as the sidecar writes them. `ControlReading` equality treats −0.0 as 0.0. Beside the controls it lists the input's SHA-256, the channel, the start, the length, the tail switch, and the changes. The changes are compared as multisets of (clock, position, identifier, value by its f64 bits, tag) and listed as "A only" and "B only", in position order. A control a request does not carry (the player's live values) reads as absent. "No difference" is shown as such. | **Every difference, by identifier** |
| D3 | **The null test.** The difference is d[i] = A[i] − B[i + s], in 64-bit floats with sequential sums.<br>**Alignment:**<br>• **Sample 0** (s = 0).<br>• **Latency-aligned**: s = (L_B − L_A) − (S_B − S_A), where L is each render's latency (`latency_samples`) and S is its start in the input. This is the default when the two differ. It is labelled, and s is shown.<br>**Spans:** the overlap where both outputs have a sample, and its prefix up to the first splice or muted region where the two renders' gate reports differ. When one report list is a prefix of the other, the prefix ends at the first extra report. "Splices differ" is shown when the reports differ, "the same as the overlap" when they agree. An empty overlap reports "no overlap" and no figures. A live take's spans end at its first budget skip. When the starts differ, the view says the null has a floor from the setup.<br>**For each span:**<br>• bit-identical or not, and the number of differing samples;<br>• the first differing sample;<br>• the peak of \|d\| in dBFS and its position (it can exceed 0 dBFS: up to +6 dBFS with the output clamp on, more without it);<br>• the RMS of d in dB re RMS 1.0;<br>• the null depth, RMS(d) ÷ RMS(A) in dB, or "A is silent" when RMS(A) = 0 over a non-empty span.<br>The raw check (equal lengths, equal bits) is always shown. The renders must be at one host rate; otherwise the test is refused in words (a core rule; in the window B is dropped with its file, so the rates always match). The view is labelled the workbench's own analysis of the engine's outputs: nothing is played or exported from d. | **Labelled alignment, two spans, five figures** |
| D4 | **The spectral difference.** Each side's average power spectrum by 002's method over the side's own runs (002's `render_runs`: splices, muted regions and the latency excluded; a live take's runs end at its first budget skip). The view shows each side's input span. The ratio B ÷ A per bin in dB, drawn where both sides are within 002's 120 dB window of their own largest bin, with both spectra beside it. It is refused, in words, when a side holds no frame or the rates differ. | **002's spectrum, B ÷ A per bin** |
| D5 | **Listening to B.** The A/B playback gains B as a third take. A switch keeps the moment through the input. From a render position o of take X, the input position is S_X + map⁻¹_X(o) (001's map, in the user's mode; in SameSeconds, S_X + o), clamped first to X's own fed length and then to the input. Into take Y, the position is map_Y(input − S_Y), at Y's start for an input before it and clamped to Y's end. The original is the input itself. | **Three takes, the input as the clock** |
| D6 | **Presets.** **Save preset…** writes `rr-dr60-workbench preset 1`: the workbench version, the engine version and commit, then one `control <identifier> <value>` line per manifest control, written as the sidecar writes it, the seed and the tap included. **Load preset…** applies a preset by 001's restore rules: unknown identifiers skipped and listed; missing controls left at their defaults and listed; refused values left at their defaults and listed with the engine's reason; a different engine commit noted. The host rate is never applied: the open file decides it, and a different rate is noted. A preset carries no input and no schedule; a sidecar is not a preset, and a preset is refused as a sidecar. Loading one clears a restored live range and its schedule; it is refused with no input open. | **The panel's values, restored as a sidecar is** |
| D7 | **Not in 005b**: comparisons of playback takes (they have their own replay, 004), a spectrogram difference, and more than one comparison render. | **Deferred** |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Compare two renders (Priority: P1)

A user renders, keeps the render as B, changes one control, renders again and compares: what changed in the settings, how deep the null is, and where in the spectrum the difference lies.

**Independent Test**: build pairs of renders in the core and check every figure against a direct computation or the engine's published law.

**Acceptance Scenarios**:

1. **Given** two renders of one input and one panel, **When** compared, **Then** the null test reports bit-identical on both spans, and the parameter diff reports "no difference" (`null_identical`).
2. **Given** hand-built outputs (an impulse of amplitude a at sample k, on N samples), **When** compared, **Then** the figures equal the closed forms: peak 20·log10 a at k, RMS a/√N, the first difference k, one differing sample (`null_figures`).
3. **Given**, for every control in turn, two requests that differ in that control alone, **When** compared, **Then** the parameter diff lists exactly that control (`diff_lists_one`). Two schedules that share some entries are listed as "A only" and "B only" (`diff_changes_multiset`).
4. **Given** renders at volume positions 1.0 and 0.5 on the earphone route, of the seeded uniform noise at −20 dBFS RMS, in both orders, **When** compared, **Then**:
   - the gate reports are equal (the premise);
   - B = g·A sample by sample within the f32 narrowing;
   - the spectral difference reads earphone_gain_db(v_B) − earphone_gain_db(v_A) within 0.001 dB in every bin within 60 dB of both maxima (at least the 300 – 2000 Hz bins);
   - the null depth reads 20·log10\|1 − g\| within 0.001 dB;
   - at seven rates (`spectral_difference_reads_gain`).
5. **Given** B is A's input rendered from a start of 100 ms of zeros, on the quiet front (no hiss, gate or coder), **When** compared latency-aligned, **Then** s = −S_B and the overlap nulls bit for bit (`null_start_aligned`). **Given** hand-built renders, B being A delayed by k with a latency k larger, **Then** s = k and nothing differs (`null_latency_aligned`). **Given** the codec off against on, **Then** s = L_B − L_A and the figures are reported (`null_codec_latency_reported`).
6. **Given** renders at different rates, **When** compared, **Then** the null test and spectral difference are refused in words and the parameter diff is still produced (`rates_differ`).

### User Story 2 - Listen to A and B (Priority: P2)

**Independent Test**: the switch rule on renders with different starts, in both modes, without a device.

**Acceptance Scenarios**:

1. **Given** A, B with another start, and the original, **When** the listening switches between any ordered pair in either mode, **Then** the position follows D5's rule (`ab_three_takes`); the callback plays the third take and packs it in the cursor (`third_take_packs_and_plays`).
2. **Given** a kept B, **When** another file is opened, **Then** B is dropped; reopening the same file keeps it (`b_follows_the_file`).

### User Story 3 - Save and load a preset (Priority: P3)

**Independent Test**: write, read and restore presets against panels, without a window.

**Acceptance Scenarios**:

1. **Given** a panel, **When** a preset is saved and loaded into a default panel, **Then** every control except the host rate equals the saved one (`preset_roundtrip`).
2. **Given** a preset with an unknown identifier, a missing control, a refused value and another host rate, **When** loaded, **Then** each is reported as in 001's restore, and the rate is not applied (`preset_reports`).
3. **Given** a sidecar opened as a preset, or a preset opened as a sidecar, **Then** each is refused in words (`preset_is_not_a_sidecar`).
4. **Given** no input open, or a restored live range, **When** a preset is loaded, **Then** it is refused, or the range is cleared (`preset_needs_input_and_clears_range`).

### Edge Cases

- **Renders of different lengths** (a changed threshold in drop mode): the overlap and the prefix span, the lengths shown (`null_prefix_drop_mode`).
- **A render with no output**, or a shift past the end: "no overlap" (`null_no_overlap`); the spectral difference is refused.
- **A live take with budget skips as B**: the first skip labelled (`compare_live_skips`).
- **A pause open at the end on one side** (`null_end_paused`); **the coder off against on with equal gates** reads "the reports agree" (`codec_reports_agree`); **sides with different runs** (drop against mute) read the same frames (`spectral_different_runs`).
- **B kept and the panel changed**: B does not change.
- **A is B** (Keep as B and no new render): bit-identical.

## Requirements *(mandatory)*

- **FR-001**: The session MUST keep B as D1 states (`b_follows_the_file`).
- **FR-002**: The parameter diff MUST follow D2 (`diff_lists_one`, `diff_changes_multiset`, `null_identical`).
- **FR-003**: The null test MUST follow D3 (`null_identical`, `null_figures`, `null_start_aligned`, `null_latency_aligned`, `null_codec_latency_reported`, `null_no_overlap`, `null_prefix_drop_mode`, `compare_live_skips`, `rates_differ`).
- **FR-004**: The spectral difference MUST follow D4 (`spectral_difference_reads_gain`, `rates_differ`).
- **FR-005**: The A/B MUST follow D5 (`ab_three_takes`, `third_take_packs_and_plays`).
- **FR-006**: Presets MUST follow D6 (`preset_roundtrip`, `preset_reports`, `preset_is_not_a_sidecar`, `preset_needs_input_and_clears_range`).
- **FR-007**: Every new string MUST pass the wording check.
- **FR-008**: The reach test prints its ratio. 005b brings `speech_codec_info` into use, for the coder's delay in the gate reports (restated after the hardware review's B2; the draft said no new method).

## Success Criteria *(mandatory)*

- **SC-001**: At seven rates, in both orders, the spectral difference reads the engine's published gain law within 0.001 dB on every bin within 60 dB of both maxima, and the null depth reads its closed form within 0.001 dB.
- **SC-002**: Every null figure equals its closed form on hand-built vectors.
- **SC-003**: The owner's desk checklist: render, keep as B, change one control, render, read the three comparisons, switch A ↔ B while listening; save a preset, reset the panel, load it.

## Assumptions

- The engine is pinned at `a06318c` (0.92.0).
- The published gain law (`output::earphone_gain_db`) is the engine's figure for the volume's scale on the earphone route.
