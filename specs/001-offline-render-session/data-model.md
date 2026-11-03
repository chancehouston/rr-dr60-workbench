# Data model: the offline render session

The entities live in `workbench_core` with no UI dependency (constitution V). The names are Rust types.

## Input

| Field | Type | Rule |
|---|---|---|
| `path`, `file_name` | path, string | as opened |
| `format` | `Wav \| Aiff \| Flac` | D1; others refused (`OpenError::UnsupportedFormat`) |
| `sample_type` | `I16 \| I24 \| I32 \| F32` | D1; 8-bit, 20-bit and f64 refused (`OpenError::UnsupportedSampleType`) |
| `rate_hz` | `u32` | one of `rr_dr60::SUPPORTED_HOST_RATES`, else `OpenError::UnsupportedRate` |
| `channels` | `u16` | ≥ 1 |
| `frames` | `u64` | per channel |
| `channel` | `Channel::Index(u16) \| Channel::Mix` | 0-based in the model (`Index(0)` is "channel 1" on screen and in the sidecar); default `Index(0)` (D2) |
| `samples` | `Arc<[f32]>` | the selected channel after FR-002's conversion, built once at decode |
| `raw_sha256` | `[u8; 32]` | the file's bytes (informational, D9) |
| `input_sha256` | `[u8; 32]` | `samples` as f32 LE; decides "same input" |
| `non_finite`, `subnormal` | `u64` | counts in `samples`; shown, passed unchanged |

Changing `channel` decodes the file again (R-15).

## ControlRow (one per manifest entry, in manifest order)

| Field | Type | Rule |
|---|---|---|
| `index` | `u32` | the manifest index |
| `entry` | `&'static ControlEntry` | from `rr_dr60::controls()`, never copied |
| `class` | `Setting \| EngineControl` | R-08: `settings_value` Ok ⇒ `Setting` |
| `value` | `ControlReading` | starts at the engine's default reading |
| `status` | `Accepted \| Refused(String) \| NotOnOnePass \| ReadOnly` | `NotOnOnePass` when `entry.paths` excludes the one-pass; `ReadOnly` for the host rate |

**Panel** = `Vec<ControlRow>` + `settings: Settings` (built from the `Setting` rows) + `check_engine: Pipeline` (R-08). `Panel::from_entries(&[ControlEntry], rate)` is the seam; `Panel::new(rate)` passes `controls()`.
`Panel::set(index, reading)` writes through the class's path, re-checks the value (one control,
then the whole settings, then the tap against the route) and updates every row's status.
`Panel::renderable()` is true when no row is `Refused`.

## RenderRequest → Render

`RenderRequest { input: Arc<[f32]> (a clone of `Input.samples`, no copy), rate_hz, settings: Settings, engine_values: Vec<(u32, ControlReading)>, tail: bool, block: usize }`, a snapshot taken at Render.

| Render field | Type | Rule |
|---|---|---|
| `samples` | `Arc<[f32]>` | concatenated `output[..produced]`, built once at the end |
| `request` | the snapshot | used for staleness and the sidecar |
| `tail_samples` | `u64` | T = latency + drain; 0 when off or at a front tap (R-07) |
| `latency_samples`, `drain_samples` | `u32` | from the fresh engine |
| `splices_or_regions` | `u64` | Σ `BlockInfo.events` |
| `events` | `Vec<VasEvent>` | for the A/B map (R-06) |
| `drop_mode` | `bool` | from the settings |
| `gate_reached` | `bool` | the engine's predicate, quoted (R-07); the splice count is shown only when true |
| `paused_at_end` | `bool` | the last call's `paused` |
| `not_emitted` | `u64` | input + tail − `samples.len()`: removed pauses, dropped tail and samples held at a final pause; 0 in mute mode, with the gate bypassed and at front taps |
| `peak` | `f32` | workbench analysis |
| `output_sha256` | `[u8; 32]` | |
| `footprint_bytes` | `usize` | `Pipeline::footprint_bytes()` |

**States**: `Idle → Rendering { progress, cancel } → Done(Render) | Cancelled | Failed(Error)`.
Cancel or failure keeps the previous `Render`. **Stale** ⇔ the current panel snapshot, channel,
input digest or tail choice differs from `render.request`.

## Sidecar

The text form in [contracts/sidecar-v1.md](contracts/sidecar-v1.md). In memory it is a
`Sidecar { fields, controls: Vec<(String, ControlReading)> }` (the slot and its parameters are control lines; the slot is restored first). Restoring it gives a
`RestoreReport { applied, unknown_ids, missing_controls, refused: Vec<(id, reason)>,
commit_differs, input_differs }` (D9).

## Monitor (playback)

`Shared { cursor: AtomicU64, request: AtomicU64, volume_bits: AtomicU32, playing: AtomicBool, overloads: AtomicU64 }`
and `Takes { original: Arc<[f32]>, render: Arc<[f32]>, rate_hz }` (clones, no copies). Cursor bit 63 is the take, and
bits 0–62 the position. The callback function is `monitor::fill(&Takes, &Shared, left: &mut [f32], right: &mut [f32])`, writing the same samples to both (R-04, R-05).
