# Contract: `workbench_core` additions for 002

```rust
pub mod meters { pub struct MeterRow; pub struct MeterField; pub const FIELDS: [MeterField; 13];
                 pub fn reached(f: &MeterField, s: &Settings) -> bool;
                 pub fn engine_field_names() -> Vec<String>; /* from Debug */ }
pub mod analysis {
    pub mod envelope { pub struct Envelope; impl Envelope { pub fn new(x: &[f32]) -> Self;
                       pub fn columns(&self, range: (u64, u64), cols: usize) -> Vec<(f32, f32)>; } }
    pub mod spectrum { pub fn n_for(rate: u32) -> usize; pub struct Spectrum;
                       pub fn welch(x: &[f32], rate: u32, spans: &[(u64, u64)]) -> Result<Spectrum, TooShort>;
                       pub fn pair(input: &[f32], r: &Render, range: (u64, u64)) -> Result<SpectrumPair, TooShort>; }
    pub mod spectrogram { pub fn compute(x: &[f32], rate: u32, wide: bool, range: (u64, u64),
                          max_cols: usize, progress: &AtomicU64, cancel: &AtomicBool) -> Option<Spectrogram>; }
}
pub mod taps { pub fn browse(input: &Input, panel: &Panel, tail: bool, progress, cancel) -> TapTable; }
pub mod stages { pub fn header_lines() -> Vec<(String, String)>; pub fn stage_fields(n: u8) -> Vec<(&'static str, String)>; }
pub mod csv { pub fn spectrum(p: &SpectrumPair, r: &Render) -> String; pub fn meters(r: &Render) -> String;
              pub fn taps(t: &TapTable) -> String; pub fn write_with_sidecar(fs, stem, csv, r) -> io::Result<()>; }
```
