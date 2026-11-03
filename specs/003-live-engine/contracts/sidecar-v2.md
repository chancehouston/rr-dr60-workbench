# Contract: the sidecar, version 2

Version 1 ([../../001-offline-render-session/contracts/sidecar-v1.md](../../001-offline-render-session/contracts/sidecar-v1.md)) with these changes:

- First line: `rr-dr60-workbench sidecar 2`. A reader accepts 1 and 2.
- New keys, required in version 2:
  - `live yes|no`;
  - `start <u64>` (S, in input samples);
  - `fed <u64>` (the input fed from S, the tail's silence included).
- The value block is the request's snapshot **at the start**.
- `tail on|off` is the tail switch, which was in force only if the input ended during the run.
- New lines, zero or more, in the engine's log order:

  ```
  change out 48000 volume 0.5 0x3fe0000000000000 1
  change in 96000 vas.threshold_dbfs -45 0xc046800000000000 2
  ```

  The fields are the clock (`in` for the control's input clock, `out` for its output clock), the landing position in that clock from the run's start, the control's identifier, the value as a decimal and as its exact f64 bits, and the tag (≥ 1; 0 is refused).
- Restore and re-render: the start values, then input[start .. start + fed] (zero past the file's end, tail off), then the change lines scheduled at their positions in file order. The samples, the gate reports and the change log equal the take's.
