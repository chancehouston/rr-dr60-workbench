# Records: the offline render session

The gate on 2026-11-02, on the development Mac (Apple silicon, macOS 26). The engine is pinned at `a06318c` (0.92.0).

## The gate (T036)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-features` | 54 tests green, 1 ignored (the release timing), after the post-implementation reviews |
| `cargo test -p workbench_core --test monitor_alloc -- --test-threads=1` | 0 allocations, 0 frees over 10⁵ `fill` calls |
| SC-004, `--release --test render_speed -- --ignored` | one minute at 48 kHz in **0.543 s** (budget 10 s) |
| Reach (FR-017) | **12/70** |
| Silent CoreAudio check (`--bin playback_check`) | 3066 samples in 0.5 s at 6 kHz and 23 963 at 48 kHz, on a 44.1 kHz device, 0 overloads |

## Figures

- **SC-001**: the render equals the whole-call and block-size-1 oracles sample for sample, at seven rates × {tone, noise, voiced} × {drop, mute}, reports included (3.9 s).
- **D7, the tail**:
  - Every input sample comes out (produced ≥ input + latency) at seven rates, with hang 50 ms, the coder on and drop mode.
  - A further second of tail leaves the prefix bit-identical.
  - What follows it (`--test tail -- --nocapture --test-threads=1`, hiss off): coder off, about 1e-16 (−320 dBFS); coder on, 9.6e-5 – 1.07e-4, against the coder's own output on silence of 1.07 – 1.19e-3 (a margin of **20.9 – 21.1 dB**), at every rate.
  - With the default settings (hiss and coder on, reported): 1.14 – 1.45e-3 after the tail, against the default engine's own output on silence of 3.10 – 4.19e-3.
  - The high-Q band-pass filter (Q at its maximum, 1 kHz) leaves 1.1e-4 (−79.1 dBFS) after the tail (reported).
- **The gate rule** (`render::gate_reached`, quoted from the engine): agrees with the engine's behavior at all twelve taps × enabled and disabled.
- **Range ends round-tripped through the sidecar**: 28 of 34. The six skipped are the two ends each of `memory.capacity_s`, `memory.max_recordings` and `playback_speed`, which are not on the one-pass. The test asserts exactly that set. No range end is refused by the engine, so there is no engine issue.
- **The FLAC fixture**: symphonia requires min block = max block for fixed blocking. The writer's first STREAMINFO used min 16, and symphonia read it as no frames.

## Dated restatements (spec Clarifications, 2026-11-02, implementation)

1. **The reach count is 70, not 74.** The engine has eleven `__` hooks, not seven, so constitution 1.0.2 restates it. The 12 methods called directly are listed in the spec. The slot's three methods are reached through the by-index calls.
2. **The tail test's bound with the coder on** is the coder's own output on silence. With the coder off it stays −100 dBFS.
3. **T030** runs the denylist over every literal in the sources, rather than banning literals outside `text`.

## Engine issues filed (constitution I)

[#72](https://github.com/chancehouston/rr-dr60-emu/issues/72) (whether the gate runs), [#73](https://github.com/chancehouston/rr-dr60-emu/issues/73) (a one-pass drain figure), [#74](https://github.com/chancehouston/rr-dr60-emu/issues/74) (the wording denylist from the core).

## Post-implementation reviews (T037), applied before the PR

The owner's autopilot rule (2026-11-02) puts every review default in force. The decisions are in the spec's Clarifications.

**Coach 1 – 13**

1. A render of another file is discarded, and opening a file cancels a running render (`Session::accept_render`).
2. The sidecar is written from the render's own request, so a stale export is true to its render.
3. The sidecar's channel is restored before the digests are compared, in core (`Session::restore`).
4. FR-008 is tested (`render_progress`).
5. SC-001's cells now pause the gate (hang 50 ms; events asserted non-empty, drop removes).
6. The two tautological "unchanged" tests are replaced by `session::refusals_leave_session_unchanged`, which also checks the messages.
7. SC-005 is checked exactly (`ab::the_map_is_exact`).
8. Manifest order is recorded as the engine values' order.
9. The sidecar's `tail on|off` field.
10. The stage groups are titled from the stage manifest (`panel::stage_title`).
11. The skipped range ends are listed exactly, a non-default switch and choice round-trip, the defaults' tail is reported, and reach asserts the items outside `Pipeline`. The taps come from the manifest.
12. A refusal attached to no control is shown at the panel's top.
13. The quickstart and traceability names are corrected, and an export is named from `file_stem`.

**Hardware B1 – B8**

1. A panel-free oracle with the earphone route, volume 0.3, 2.4 V, and the band filter at 800 Hz with Q 3, at 6, 44.1 and 48 kHz and block sizes 0 and 1.
2. The A/B request carries its base position (`Shared::request_from`), and play-the-other-take goes through the map.
3. The tail figures are printed.
4. As coach 3.
5. As coach 9.
6. A restore sets every value, then checks once.
7. The 20-in-24 refusal is pinned to `UnsupportedSampleType`, the mix doc is reworded, and `convert_i16` is reused.
8. The headroom hint.

## Convergence

`/speckit-converge` found one partial gap, F1: a long file's memory bound. `input::open` read the whole file and built `i32` copies before converting.

T038 closes it. The raw digest is now computed in a streaming pass, the decoder reads the file through symphonia's `File` source, each packet is converted straight into the single f32 output (the mix is summed per packet), and the output is reserved from the track's frame count. The 54 tests are green unchanged.
