# Quickstart: validating the offline render session

## Prerequisites

The development Mac, Rust stable, and network access on the first build (the engine is fetched
from GitHub at the pinned commit).

## Automated (no window, no device)

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features                      # every FR/SC test named in tasks.md
cargo test -p workbench_core --test reach -- --nocapture     # prints "Pipeline reach: 12/70" and the uncalled list
cargo test -p workbench_core --test tail -- --nocapture --test-threads=1   # the tail figures records.md quotes
cargo test -p workbench_core --release --test render_speed -- --ignored --nocapture   # SC-004
cargo test -p workbench_core --test monitor_alloc -- --test-threads=1                 # FR-011
cargo run -p rr_dr60_workbench --bin playback_check   # silent CoreAudio check (macOS)
```

Expected:
- `render_oracle::render_equals_the_oracles` and `render_oracle::panel_free_oracle_with_live_values` pass at seven rates in drop and mute mode, with the gate pausing (SC-001).
- `export::roundtrip` and `sidecar::reproduces` pass at seven rates (SC-002).
- `panel::presents_every_control` and `panel::grows_with_the_engine` pass (SC-003).
- `session::refusals_leave_session_unchanged` passes (SC-006).
- `render_progress::progress_and_cancel` passes (FR-008).
- `render_speed` prints the time for one minute at 48 kHz, under 10 s.

## Desk checklist (the owner, on the Mac; SC-005, SC-007)

Generate the desk files (speech-like bursts with pauses, stereo, at 6 and 48 kHz, plus an MP3 stub,
a 22.05 kHz WAV and an 8-bit WAV) into an ignored folder, and use headphones (the Mac's built-in
speakers add their own processing):

```sh
cargo run -p workbench_core --example desk_files -- out/desk
cargo run --release -p rr_dr60_workbench
```

Then:

1. Open a stereo 48 kHz WAV of speech with pauses. Check that channel 1 is used and that the picker offers channel 2 and the labelled mix.
2. Press Render. Check that progress moves and that the readings appear: length, latency, tail, splices, removed, paused at end, peak.
3. Play the original and switch to the render several times, including just after a pause. Check that the content lines up and the switch is immediate. Then switch to "same seconds" and hear the drift.
4. Set the voice-activation sensitivity to 5. Check that the stale marker appears, then render and hear the change.
5. Set the tap to the earphone tap with the speaker route. Check that the refusal shows beside both controls and that Render is unavailable.
6. Insert the band filter and change its frequency, then render and hear it.
7. Export, open the WAV in another player, and check that the sidecar reads sensibly.
8. Quit, open the same file, open the sidecar, render, and check that the output digest equals the sidecar's.
9. Open a 6 kHz file and play it. Check that the device-rate note is shown.
10. Open the MP3 stub, the 22.05 kHz file and the 8-bit file. Check that each is refused with a clear message.
11. Start a render of the long desk file and cancel it. Check that it stops at once and the previous render stays.
12. Set the volume to its maximum on the speaker route and render. Check that the peak warning and its headroom hint appear.
13. Export over an existing export. Confirm the overwrite, and check that the WAV and the sidecar are both new.
14. Unplug the headphones (or change the output device) during playback. Check that playback stops with a message.
15. Press "reset to defaults". Check that every control returns to its default and the render is marked stale.
