# Contract: the sidecar, version 1

A UTF-8 text file beside each export, named `<export stem>.sidecar.txt`. Lines end in `\n`. The
first line identifies the format, and every other line is `key value…`, one key per line, with no
key repeated. Blank lines and lines starting with `#` are ignored. The example's figures are
illustrative; the grammar is normative. A `#` after a value is not a comment in the file (the
example's trailing notes are for this page only).

```
rr-dr60-workbench sidecar 1
workbench.version 0.1.0
engine.version 0.92.0
engine.commit a06318ca1c7ded42f46d83c34716cdfad8a59446
conversion 1
recording_counter 0
block 4096
tail on                        # the switch: on or off
tail_samples 6128
channel 1                      # 1-based; or: channel mix
input.file voice.wav           # the rest of the line, verbatim
input.frames 2880000
input.raw_sha256 9f…
input.sha256 3c…
render.frames 2716544
render.latency_samples 1923
render.splices_or_regions 4
render.mode drop               # or mute
render.gate_reached yes
render.paused_at_end no
render.not_emitted 169584
render.peak 1.96374 0x3ffb5d2e
render.sha256 77…
control route 0
control volume 0.13345174 0x3e08a73f
control vas.sensitivity 3
control agc.knee_dbfs -1 0xbf800000
control tap 8
control host_rate_hz 48000
control seed 0
…one line per manifest control, in manifest order…
```

## Values

| Manifest type | Written | Read |
|---|---|---|
| switch | `on` / `off` | exactly those words |
| integer | decimal `u64` | decimal, no sign, no leading `+` |
| continuous | `<shortest round-trip decimal> 0x<8 hex digits>` | the bits are the value; the decimal must parse to the same bits, or the line is malformed |
| choice | the engine value as a decimal `u32` | must be one of the control's options, or it is refused (listed, not malformed) |

Values are written as the engine reads them back after the write. −0.0 reads back as 0.0 on
every path (`set_settings_value` and the live path store it as 0.0; engine `control_map.rs:324`).
The component slot's line is applied before its parameters' lines, whatever the file's order.

## Rules

- `tail` is `on` or `off`. Any other value is malformed. It restores the switch, not the length; the length is 0 at the taps before the gate even with the switch on.
- The `input.*` lines come from the render's own request, so a stale render's sidecar names the input it was made from.

- First line other than `rr-dr60-workbench sidecar 1` → refused: "not a sidecar" or "sidecar
  version N is not supported".
- A required key missing, a key repeated, an unknown key outside `control`, or an unparsable
  value → refused as malformed, naming the line. The session is unchanged.
- `control <id>` with an id the engine does not know → skipped and listed (D9).
- A manifest control with no line → left at its default and listed.
- `engine.commit` different from the build's → listed: "made with engine commit X; this build is
  Y; the render may differ".
- `input.sha256` different from the loaded input → listed: "the input differs". Values are still
  restored.
- `control host_rate_hz` is informational: the loaded file's rate is the session's (D4). It is
  listed when it differs and is never applied.
