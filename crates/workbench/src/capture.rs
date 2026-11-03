//! The capture's input unit (spec 007 D1, D2; plan R-04): CoreAudio's input unit on the default
//! input device, at its nominal rate and channel count, f32 interleaved. The input callback calls
//! only `LiveCore::push` and the gap counter, both real-time safe.

use std::sync::Arc;

use workbench_core::capture::GapCounter;
use workbench_core::live::LiveCore;

/// The default input device as it stands: its id, nominal rate, channels and name.
#[derive(Clone, Debug)]
pub struct InputDevice {
    /// The device id.
    pub id: u32,
    /// Its nominal rate.
    pub rate: f64,
    /// Its input channels.
    pub channels: usize,
    /// Its name.
    pub name: String,
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::Arc;

    use coreaudio::audio_unit::audio_format::LinearPcmFlags;
    use coreaudio::audio_unit::macos_helpers::{
        audio_unit_from_device_id, get_default_device_id, get_device_name,
    };
    use coreaudio::audio_unit::render_callback::{self, data};
    use coreaudio::audio_unit::{AudioUnit, Element, SampleFormat, Scope, StreamFormat};
    use workbench_core::capture::GapCounter;
    use workbench_core::live::LiveCore;

    use super::InputDevice;

    pub fn default_input() -> Result<InputDevice, String> {
        let id = get_default_device_id(true).ok_or("No default input device")?;
        let unit = audio_unit_from_device_id(id, true).map_err(|e| e.to_string())?;
        // The device side of the input element: its rate and channels.
        let hw = unit
            .stream_format(Scope::Input, Element::Input)
            .map_err(|e| e.to_string())?;
        Ok(InputDevice {
            id,
            rate: crate::playback::nominal_rate(id).unwrap_or(hw.sample_rate),
            channels: (hw.channels as usize).max(1),
            name: get_device_name(id).unwrap_or_default(),
        })
    }

    pub struct Unit {
        unit: AudioUnit,
    }

    impl Unit {
        pub fn start(
            dev: &InputDevice,
            mut core: LiveCore,
            gaps: Arc<GapCounter>,
        ) -> Result<Self, String> {
            let mut unit = audio_unit_from_device_id(dev.id, true).map_err(|e| e.to_string())?;
            let fmt = StreamFormat {
                sample_rate: dev.rate,
                sample_format: SampleFormat::F32,
                flags: LinearPcmFlags::IS_FLOAT | LinearPcmFlags::IS_PACKED,
                channels: dev.channels as u32,
            };
            unit.set_stream_format(fmt, Scope::Output, Element::Input)
                .map_err(|e| e.to_string())?;
            type Args = render_callback::Args<data::Interleaved<f32>>;
            unit.set_input_callback(move |args: Args| {
                gaps.observe(args.time_stamp.mSampleTime, args.num_frames);
                core.push(args.data.buffer);
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            unit.start().map_err(|e| e.to_string())?;
            Ok(Self { unit })
        }
    }

    impl Drop for Unit {
        fn drop(&mut self) {
            let _ = self.unit.stop();
        }
    }
}

/// The default input device, or why there is none.
///
/// # Errors
///
/// No default input device, or the platform's refusal; off macOS, unavailable.
pub fn default_input() -> Result<InputDevice, String> {
    #[cfg(target_os = "macos")]
    {
        mac::default_input()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(workbench_core::text::PLAYBACK_UNAVAILABLE.to_owned())
    }
}

/// A running capture: the input unit with the core in its callback.
pub struct Capture {
    #[cfg(target_os = "macos")]
    _unit: mac::Unit,
    device: InputDevice,
}

impl Capture {
    /// Starts `core` in the input callback of `device` (spec 007 D2).
    ///
    /// # Errors
    ///
    /// The platform's refusal.
    pub fn start(
        device: InputDevice,
        core: LiveCore,
        gaps: Arc<GapCounter>,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                _unit: mac::Unit::start(&device, core, gaps)?,
                device,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (core, gaps, device);
            Err(workbench_core::text::PLAYBACK_UNAVAILABLE.to_owned())
        }
    }

    /// The device captured from.
    pub fn device(&self) -> &InputDevice {
        &self.device
    }

    /// Why the capture must stop, if it must (coach review 5): the default input changed, the
    /// device is gone, or its rate changed.
    pub fn lost(&self) -> Option<&'static str> {
        #[cfg(target_os = "macos")]
        {
            use coreaudio::audio_unit::macos_helpers::get_default_device_id;
            if get_default_device_id(true) != Some(self.device.id) {
                return Some("The default input device changed: the capture stopped");
            }
            if !crate::playback::device_alive(self.device.id) {
                return Some("The input device is gone: the capture stopped");
            }
            if crate::playback::nominal_rate(self.device.id) != Some(self.device.rate) {
                return Some("The input device's rate changed: the capture stopped");
            }
            None
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}
