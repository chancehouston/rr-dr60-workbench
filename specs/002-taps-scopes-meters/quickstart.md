# Quickstart: taps, scopes and meters

```sh
cargo test --all-features
cargo test -p workbench_core --test spectrum -- --nocapture       # the calibration figures (bin 200; noise density)
cargo test -p workbench_core --test meters -- --nocapture         # the timeline and the reach table
cargo test -p workbench_core --test reach -- --nocapture          # 14/70
cargo test -p workbench_core --release --test speed_002 -- --ignored --nocapture   # SC-004
```

## Desk checklist

Use the 001 desk files (`cargo run -p workbench_core --example desk_files -- out/desk`), then:

1. Render the stereo speech file and open Meters. Check that the gain reduction rises on loud syllables and that the gate's paused trace follows the pauses.
2. Set the tap to after AGC. Check that the record-side and output-side traces turn grey, "not reached".
3. On Scopes, check that the waveforms line up, then switch to raw and see the latency and the removed pauses.
4. Select a vowel and compare the spectra. Check that the ratio shows the band's roll-offs. Toggle dB/Hz.
5. Open the spectrogram of both takes. Toggle wideband.
6. Run the tap browser and read the table. Set one tap in the panel and listen to it.
7. Open Stages and read stage 7 (the speech coder).
8. Export each CSV and check that the sidecar sits beside it.
