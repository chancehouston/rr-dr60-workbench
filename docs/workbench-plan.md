# The workbench plan of record

The workbench is a desktop power tool for exploring and tuning the rr-dr60-emu engine. It sits
alongside the iOS app and its test bench and replaces neither. This plan sets the order of
features. Each feature is a Spec-Kit feature on its own branch, and its spec decides the details.
The owner set the direction on 2026-11-02: egui, a public repository, a companion tool rather than a product.

Status at bootstrap: the engine is at 0.92.0 (`a06318c`), with 36 controls, 81 public `Pipeline`
methods and 15 stages. The shell lists the control manifest read-only.

## Features

| # | Feature | What it brings into reach | Depends on |
|---|---|---|---|
| 001 | **Offline render session** | Open an audio file at any supported host rate. Edit every control, including the tap and the component slot, through a panel generated from the control manifest. Render through the default one-pass engine, with a tail so that the whole input comes out. Play the original and the render A/B, export the render with its settings sidecar. | — |
| 002 | **Taps, scopes and meters** | Waveform and spectrum views of the input, the tap and the difference. The per-call meters plotted over time. The stage manifest as a panel per stage, with its sources and assumptions. | 001 |
| 003 | **Live engine** | Real-time playback through the engine on the output device. Use CoreAudio's output unit, as 001 does: cpal opens only the device's own rates, and macOS devices offer no 6, 8 or 16 kHz (001 research R-04). Live controls and scheduled changes (`schedule_change`) from the panel. The change log and voice-activation events on a timeline. | 001, 002 |
| 004 | **Record and play paths** | The recorder, the memory (items, capacity, erase, export and import of the stored form), offline playback to a take with the speed levels and modes, route, volume, battery and the stage switches, and a playback sidecar that replays. | 003 |
| 004b | **Live playback** (split from 004) | A clone of the device engine in the output callback for a live playback with live controls; the memory stays usable on the device meanwhile (spec 004b's clarification). | 004 |
| 005 | **Processing records** | Write, read and re-render processing records (spec 020). | 004 |
| 005b | **Comparisons and presets** (split from 005, 2026-11-03) | Two renders side by side with a null test, a spectral difference and a parameter diff. Settings presets saved as files. | 001, 002 |
| 007 | **The Mac's microphone as the input** (deferred from 003) | An input device as the source, the engine run in the input callback so the input clock drives it (no drift gaps), the take keeping the captured input; acoustic feedback and capture digests handled. | 003 |
| 006 | **Coverage close-out** | The `Pipeline` reach test at 100 % (constitution II): every remaining method. A guided tour of the stages. | 001–005b |

## Standing rules

- Engine gaps become engine issues or specs (constitution I). The workbench pin moves only to a
  merged engine commit or tag.
- Each feature lists the `Pipeline` methods and taps it brings into use. The reach test prints
  the running ratio from feature 001 on.
- The owner runs each feature's desk checklist on the Mac before merging.
