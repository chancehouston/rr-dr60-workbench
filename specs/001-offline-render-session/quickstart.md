# Quickstart: validating the offline render session

## Prerequisites

The development Mac, Rust stable, and network access on the first build (the engine is fetched
from GitHub at the pinned commit).

## Automated (no window, no device)

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features                      # every FR/SC test named in tasks.md
cargo test -p workbench_core --test reach -- --nocapture     # prints "Pipeline reach: 13/81" and the uncalled list
cargo test -p workbench_core --release --test render_speed -- --ignored --nocapture   # SC-004
```

Expected:
- `render_oracle` passes at seven rates in drop and mute mode (SC-001).
- `export_roundtrip` and `sidecar_reproduces` pass at seven rates (SC-002).
- `panel_presents_every_control` passes (SC-003).
- `refusals_leave_session_unchanged` passes (SC-006).
- `render_speed` prints the time for one minute at 48 kHz, under 10 s.

## Desk checklist (the owner, on the Mac; SC-005, SC-007)

`cargo run --release -p rr_dr60_workbench`, then:

1. Open a stereo 48 kHz WAV of speech with pauses. Check that channel 1 is used and that the picker offers channel 2 and the labelled mix.
2. Press Render. Check that progress moves and that the readings appear: length, latency, tail, splices, removed, paused at end, peak.
3. Play the original and switch to the render several times, including just after a pause. Check that the content lines up and the switch is immediate. Then switch to "same seconds" and hear the drift.
4. Set the voice-activation sensitivity to 5. Check that the stale marker appears, then render and hear the change.
5. Set the tap to the earphone tap with the speaker route. Check that the refusal shows beside both controls and that Render is unavailable.
6. Insert the band filter and change its frequency, then render and hear it.
7. Export, open the WAV in another player, and check that the sidecar reads sensibly.
8. Quit, open the same file, open the sidecar, render, and check that the output digest equals the sidecar's.
9. Open a 6 kHz file and play it. Check that the device-rate note is shown.
10. Open an MP3 and a 22.05 kHz file. Check that both are refused with clear messages.
