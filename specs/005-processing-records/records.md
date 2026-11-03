# Records: processing records

Recorded on 2026-11-03, with the engine at `a06318c` (0.92.0, processing record form 2).

| Check | Result |
|---|---|
| fmt, clippy `-D warnings` | clean |
| `cargo test --all-features` | 147 green, 3 ignored |
| `tests/records.rs` | 25 green in about 18 s (debug profile, seven rates) |
| reach | 50/70 (41 → 50: the nine methods of FR-005) |
| engine issues | #76 (a public re-render; the workbench mirrors the recipe), #77 (Swift's slot restore differs from the harness's) |

## Figures

- **`record_written_as_engine`**: at seven rates, in the sixteen cells (codec × changes × drop/mute × stop call/self-stop), the device's record equals a direct engine's for the same input in the same 10 ms calls, byte for byte. The device reader sets no flag.
- **`re_render_reproduces`**: in the same 112 cells, the re-render's digest equals the item's.
- **`re_render_detects_deltas`**: at seven rates every delta in the plan's table differs:
  - the seed, the counter, the knee to −30 dBFS and the threshold −3 dB;
  - the hiss entry moved ⌈rate ÷ 6000⌉ host samples with the codec off, and the knee entry moved 0.1 s with the codec on;
  - the stop 120 device samples earlier, with the codec on and off;
  - no fill, in a self-stop after a prior item, where the fill is asserted needed.
  The output clamp twin reproduces.
- **`live_value_twins_reproduce`**: volume, route, speed level and battery set on the device before the recording all reproduce, at seven rates.
- **`slot_restored`**: with the band filter at 500 Hz, Q 2 and type 1, the recording reproduces at seven rates. Both slot deltas differ: component none, and the frequency × 1.5. The settings diff lists the four slot controls, and nothing once the panel matches. With `UNKNOWN_COMPONENT` set, no slot rows appear.
- **The threshold delta**: a −43.0 dBFS tone after voice. The edges by rate (from the hardware review's bisection): the gate stops pausing above −40.73 … −41.55 dBFS under the −40 dBFS threshold, and above −44.44 … −45.31 dBFS under −43. That leaves about 1.4 dB to each edge at every rate, and the test asserts the cause first, with two recordings whose stored lengths differ. The first draft's 0.006 (−44.4 dBFS) came from a probe at 6 kHz only, and sat 0.00 – 0.01 dB from the edge at 88.2 and 96 kHz.
- **`feed_point_rule`** and **`re_render_from_offset`**: a 16- or 24-bit file's fed span is on the 24-bit grid. Its `.i24` decodes to the fed samples bit for bit, from start 0 and from a start of 0.4 s, and re-renders alone, fed from 0, to the item's digest. A float input scaled off the grid is `f32` and writes no span.
- **`re_render_refills_schedule`**: 72 hiss changes, more than the 64 pending, reproduce.
- **`runs_follow_the_counter`**: an erase renumbers item 2 to 1, and its record is byte-identical.
- **`imported_item_record`**: the engine's imported-origin record (`IMPORTED_ORIGIN`). The re-render is refused in words.
- **Refusals**:
  - another input, another channel, another record's record-input file, and no record-input file, in `re_render_refuses_other_input`;
  - a changed seed digit (`RecordChecksumMismatch`), a missing line, and version 99 (`UnknownRecordVersion`), in `record_refusals`.

## Draft and plan reviews, applied

- **The draft reviews** (coach C1 – C14, hardware B1 – B8):
  - the split into 005 and 005b;
  - "differs" tested in the core;
  - the run's stop fields by the engine's conversions;
  - the `.i24` span;
  - the mirror and its engine issue.
- **The plan reviews** (coach P1 – P10, hardware Q1 – Q7):
  - the slot restore without the crate-private `parameter_at`;
  - `prepare` holding every refusal;
  - the settings diff on the pure `settings_value`, so no engine reserves the record's memory;
  - the buffers sized when used, and the self-stop erase on an overflow;
  - the digest of the kept item;
  - the contract's engine commit, host rate and `none` forms.

## Post-implementation reviews (T009), applied

- **Coach 1 – 13**:
  - The expected label takes its flags from the device reader's read-out, so `HOST_RATE_DIFFERS` is included, and the result line carries it (`expected_note`, `flags_label_expected`).
  - The plan's test names are real tests.
  - The live twins run at seven rates.
  - "No panel open: no diff".
  - The splice rows appear in a grid, all sixteen stored parameters are shown, "Check against item" needs a device, and the doc comments are back above their functions.
  - The Debug names on screen are checked by `wording`.
  - `record_refusals` matches the engine's variants.
  - The −0.0 test goes through `settings_diff`.
- **Hardware B1 – B6**:
  - **B1**: the slot cell.
  - **B2**: the threshold amplitude in the middle of the window, with the cause asserted.
  - **B3**: the late-hang test now asserts that the bound rises on a check engine. The device's over-buffer paths are guards that no 10 ms call reaches.
  - **B4**: the slot is restored at the published offsets of every component (`components::components()`), and engine issue #77 records Swift's divergence.
  - **B5**: the `.i24` span is checked from an offset and re-rendered alone.
  - **B6**: the re-render's report buffer is kept as the harness keeps it, with a comment.
