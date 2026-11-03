# 005b (comparisons and presets): the decisions carried from 005's draft

Split from 005 on 2026-11-03 (the coach's draft review C6). These are 005's draft decisions D5 – D9 with the draft reviews' findings attached. The 005b spec starts from here.

- **Two renders side by side (005 draft D5).** **Keep as B** keeps the current render as B; the current render is A; both waveforms on one time axis; a null test, a spectral difference and a parameter diff; B as a third A/B source. The position maps through the input, start included: input = A.start + to_original(A); B's position = to_render(input − B.start, B), in the user's `AbMode` (B7). A live take may be kept as B. B is dropped when a new file opens, so the host rates are always equal in the window: "rates differ" is a core unit test only (C3, B7).
- **The parameter diff (D6).** By identifier in manifest order, compared as engine readings (spec 025), not strings, so −0.0 equals 0.0 (C8). It also lists the rate, the seed, the input SHA-256, the channel, the start, the tail and the changes. Changes are compared as multisets of (clock, position, identifier, value, tag), listed as "A only" and "B only" in position order (C9).
- **The null test (D7).**
  - Keep the raw bit-identity check: equal lengths, equal bits, and the count of differing samples (C13).
  - When the latencies or starts differ, align B by (L_B − L_A) + (S_B − S_A) [superseded: the sign is −, spec 005b D3 and its Clarifications], labelled "latency-aligned", with sample 0 as an option (C2, B4).
  - Report the figures over two spans: up to the first differing gate event, and the whole common length; label "splices differ" in drop mode (B4).
  - f64 sequential sums; RMS in dB re RMS 1.0; null depth "A is silent" when RMS(A) = 0; the peak of |d| can reach +6 dBFS; the view labelled the workbench's own analysis (C13).
- **The spectral difference (D8).**
  - Each side over its own runs (`spectrum::render_side`, splices excluded), not the common length; refuse only when a side holds no frame; `ratio_db`'s 120 dB window (B5, C7).
  - Oracle: the earphone route, white noise at −20 dBFS, volume positions 1.0 and 0.5. The expected ratio is `output::earphone_gain_db(0.5) − earphone_gain_db(1.0)`, from the engine, never a copied constant. The tolerance is 0.001 dB in the window, with the maximum reproduced before it is quoted. The same pair gives the null depth 20·log10|1 − g| (B6). SC-002's direct computations use hand-built vectors (an impulse of amplitude a at sample k).
- **Presets (D9).** Reuse the sidecar parser with a `rr-dr60-workbench preset 1` header and no input section; the seed and the tap included; the host rate never applied (C14).
