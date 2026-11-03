# Contract: the record-input file, version 1

Written beside a processing record (`<stem>.record.txt`) as `<stem>.record-input.txt`. Lines `key value`; `#` comments and blank lines ignored; every key required and once; a missing or unknown key, or another version line, is refused. `input.sha256` is the digest of the whole converted channel (001's), not of the span.

```text
rr-dr60-workbench record-input 1
workbench.version 0.x.y
engine.commit <40 hex>
host_rate <Hz>
record.file <stem>.record.txt
record.sha256 <64 hex>            # the record file's bytes
origin engine | imported
input.file <name> | none          # none for an imported item
input.sha256 <64 hex> | none      # the converted input (001's digest)
channel 1 | 2 | … | mix | none
conversion 1 | none               # 001's conversion rule
start <host samples> | none       # the start offset in the converted input
fed <host samples> | none         # the stop's input position
feed_point i24 | f32 | none
```

`feed_point i24` means `<stem>.record-input.i24` holds the fed span: `fed` little-endian `i32` values in −2²³ … 2²³ − 1, and `samples_from_i24` of them equals `input[start .. start + fed]` bit for bit. With `f32`, the record re-renders in the workbench only, from the named input file.
