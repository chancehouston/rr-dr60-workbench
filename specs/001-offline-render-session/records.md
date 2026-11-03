# Records: the offline render session

The gate on 2026-11-02, on the development Mac (Apple silicon, macOS 26). The engine is pinned at `a06318c` (0.92.0).

## The gate (T036)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --all-targets --all-features -- -D warnings` | clean |
| `cargo test --all-features` | 45 tests green, 1 ignored (the release timing) |
| `cargo test -p workbench_core --test monitor_alloc -- --test-threads=1` | 0 allocations, 0 frees over 10⁵ `fill` calls |
| SC-004, `--release --test render_speed -- --ignored` | one minute at 48 kHz in **0.543 s** (budget 10 s) |
| Reach (FR-017) | **12/70** |
| Silent CoreAudio check (`--bin playback_check`) | 3066 samples in 0.5 s at 6 kHz and 23 963 at 48 kHz, on a 44.1 kHz device, 0 overloads |

## Figures

- **SC-001**: the render equals the whole-call and block-size-1 oracles sample for sample, at seven rates × {tone, noise, voiced} × {drop, mute}, reports included (3.9 s).
- **D7, the tail**:
  - Every input sample comes out (produced ≥ input + latency) at seven rates, with hang 50 ms, the coder on and drop mode.
  - A further second of tail leaves the prefix bit-identical.
  - What follows it: coder off, about 1e-16 (−320 dBFS); coder on, about 1.0 – 1.1e-4, against the coder's own output on silence of 1.07 – 1.19e-3, at every rate.
  - The high-Q band-pass filter (Q at its maximum, 1 kHz) leaves 1.1e-4 (−79.1 dBFS) after the tail (reported).
- **The gate rule** (`render::gate_reached`, quoted from the engine): agrees with the engine's behavior at all twelve taps × enabled and disabled.
- **Range ends round-tripped through the sidecar**: 28 of 34. The other six are not on the one-pass, or the whole-settings check refuses them.
- **The FLAC fixture**: symphonia requires min block = max block for fixed blocking. The writer's first STREAMINFO used min 16, and symphonia read it as no frames.

## Dated restatements (spec Clarifications, 2026-11-02, implementation)

1. **The reach count is 70, not 74.** The engine has eleven `__` hooks, not seven, so constitution 1.0.2 restates it. The 12 methods called directly are listed in the spec. The slot's three methods are reached through the by-index calls.
2. **The tail test's bound with the coder on** is the coder's own output on silence. With the coder off it stays −100 dBFS.
3. **T030** runs the denylist over every literal in the sources, rather than banning literals outside `text`.

## Engine issues filed (constitution I)

[#72](https://github.com/chancehouston/rr-dr60-emu/issues/72) (whether the gate runs), [#73](https://github.com/chancehouston/rr-dr60-emu/issues/73) (a one-pass drain figure), [#74](https://github.com/chancehouston/rr-dr60-emu/issues/74) (the wording denylist from the core).

## Post-implementation reviews (T037)

See below. They are applied before the PR.
