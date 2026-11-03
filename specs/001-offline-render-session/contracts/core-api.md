# Contract: `workbench_core`'s public surface for 001

The egui binary uses only these items. Each one is tested without a window or a device.

```rust
pub mod input {
    pub fn open(path: &Path, channel: Channel) -> Result<Input, OpenError>;
    pub fn convert_i16(src: &[i16], dst: &mut [f32]);           // exact
    pub fn convert_i24(src: &[i32], dst: &mut [f32]);           // = rr_dr60::input::samples_from_i24
    pub fn convert_i32(src: &[i32], dst: &mut [f32]);           // one rounding, labelled
    pub fn mix(channels: &[&[f32]]) -> Box<[f32]>;              // plus integer variants
}
pub mod panel {
    pub struct Panel { /* rows, settings, check engine */ }
    impl Panel {
        pub fn new(rate_hz: u32) -> Result<Self, rr_dr60::Error>;
        pub fn rows(&self) -> &[ControlRow];
        pub fn set(&mut self, index: u32, value: ControlReading);
        pub fn reset_to_defaults(&mut self);
        pub fn renderable(&self) -> bool;
        pub fn snapshot(&self) -> (Settings, Vec<(u32, ControlReading)>);
    }
}
pub mod render {
    pub fn render(req: &RenderRequest, progress: &AtomicU64, cancel: &AtomicBool)
        -> Result<Option<Render>, rr_dr60::Error>;              // None = cancelled
}
pub mod ab {
    pub fn to_render(i: u64, r: &Render, mode: AbMode) -> u64;
    pub fn to_original(o: u64, r: &Render, mode: AbMode, input_len: u64) -> u64;
}
pub mod export {
    pub fn export(dir_stem: &Path, input: &Input, render: &Render) -> Result<(), ExportError>;
}
pub mod sidecar {
    pub fn write(input: &Input, render: &Render) -> String;
    pub fn read(text: &str) -> Result<Sidecar, SidecarError>;
    pub fn restore(s: &Sidecar, panel: &mut Panel, input: &Input) -> RestoreReport;
}
pub mod monitor {
    pub struct Shared; pub struct Takes;
    pub fn fill(takes: &Takes, shared: &Shared, out: &mut [f32]); // real-time safe
}
pub mod text;                                                    // every workbench string
pub fn engine_commit() -> &'static str;                           // R-12
```
