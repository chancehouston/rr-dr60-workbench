# Quickstart: Engine 0.94 adoption

`cargo test --all-features` (reach 75/75); the feature's own tests: `cargo test -p workbench_core --test records --test device --test reach`.

## Desk checklist

Use the 001 desk files (`cargo run -p workbench_core --example desk_files -- out/desk`), then on the Device tab:

1. Record the session's input, **save record…** on the item, then re-render it from the record. The result line reads "Reproduces", with both digests and the rows that landed.
2. Change one setting in the record's plan (for example open a record whose settings differ from the panel) and re-render: the line reads "Differs" or "Rebuilt"; with the output clamp toggled it reads "Rebuilt, matching the original".
3. Cut the session's input short of the record's stop and re-render: the message names the host samples missing.
4. The memory line shows the encoding ("device samples", or "speech coder frames" with the coder on).
5. Set "leave (s)" to 1.0 and **Fill the memory**: one item appears and about 1 s remains; record: the recording stops by itself. **save record…** on the filler item writes an imported-origin record. The hover text names it an addition of the emulator.
6. Set "at counter" to 1 and **Reset device at counter**; record the same input as on a device reset at 0 that recorded twice: the digest equals that device's second item.
