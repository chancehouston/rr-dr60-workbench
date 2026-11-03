//! Playback through CoreAudio's default output unit (spec 001 FR-010, FR-011; research R-04,
//! R-05). The take's rate is set on the unit's input scope as two identical non-interleaved f32
//! channels, and macOS converts to the device's rate at Mastering quality. The render callback
//! calls only `workbench_core::monitor::fill`.

use std::sync::Arc;

use workbench_core::monitor::{Shared, Takes};

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use coreaudio::audio_unit::audio_format::LinearPcmFlags;
    use coreaudio::audio_unit::render_callback::{self, data};
    use coreaudio::audio_unit::{AudioUnit, Element, IOType, SampleFormat, Scope, StreamFormat};
    use objc2_audio_toolbox::{
        kAudioUnitProperty_SampleRateConverterComplexity,
        kAudioUnitSampleRateConverterComplexity_Mastering,
    };
    use objc2_core_audio::{
        AudioObjectAddPropertyListener, AudioObjectGetPropertyData, AudioObjectPropertyAddress,
        AudioObjectRemovePropertyListener, kAudioDeviceProcessorOverload,
        kAudioDevicePropertyDeviceIsAlive, kAudioDevicePropertyNominalSampleRate,
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    };
    use workbench_core::monitor::{Shared, Takes, fill};

    fn address(selector: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    fn get<T: Copy + Default>(object: u32, selector: u32) -> Option<T> {
        let mut addr = address(selector);
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        // SAFETY: `addr`, `size` and `value` are valid for the call and `size` is `T`'s size.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&mut addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut value).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    /// The current default output device.
    pub fn default_device() -> Option<u32> {
        get::<u32>(
            kAudioObjectSystemObject as u32,
            kAudioHardwarePropertyDefaultOutputDevice,
        )
    }

    /// Whether a device is still alive.
    pub fn device_alive(device: u32) -> bool {
        get::<u32>(device, kAudioDevicePropertyDeviceIsAlive).is_some_and(|v| v != 0)
    }

    /// A device's nominal rate.
    pub fn nominal_rate(device: u32) -> Option<f64> {
        get::<f64>(device, kAudioDevicePropertyNominalSampleRate)
    }

    /// Counts overloads into the `Shared` the client data points to.
    unsafe extern "C-unwind" fn on_overload(
        _object: u32,
        _n: u32,
        _addresses: NonNull<AudioObjectPropertyAddress>,
        client: *mut c_void,
    ) -> i32 {
        // SAFETY: `client` is `Arc::as_ptr` of the `Shared` the `Player` keeps alive until the
        // listener is removed in `Drop`.
        let shared = unsafe { &*client.cast::<Shared>() };
        shared.overloads.fetch_add(1, Ordering::Relaxed);
        0
    }

    /// A running output unit.
    pub struct Player {
        unit: AudioUnit,
        shared: Arc<Shared>,
        device: Option<u32>,
        listening: bool,
    }

    impl Player {
        pub fn start(takes: Takes, shared: Arc<Shared>) -> Result<Self, String> {
            let mut unit = AudioUnit::new(IOType::DefaultOutput).map_err(|e| e.to_string())?;
            let fmt = StreamFormat {
                sample_rate: f64::from(takes.rate_hz),
                sample_format: SampleFormat::F32,
                flags: LinearPcmFlags::IS_FLOAT
                    | LinearPcmFlags::IS_PACKED
                    | LinearPcmFlags::IS_NON_INTERLEAVED,
                channels: 2,
            };
            unit.set_stream_format(fmt, Scope::Input, Element::Output)
                .map_err(|e| e.to_string())?;
            let mastering: u32 = kAudioUnitSampleRateConverterComplexity_Mastering;
            unit.set_property(
                kAudioUnitProperty_SampleRateConverterComplexity,
                Scope::Global,
                Element::Output,
                Some(&mastering),
            )
            .map_err(|e| e.to_string())?;
            let cb_shared = Arc::clone(&shared);
            type Args = render_callback::Args<data::NonInterleaved<f32>>;
            unit.set_render_callback(move |mut args: Args| {
                let mut ch = args.data.channels_mut();
                if let (Some(l), Some(r)) = (ch.next(), ch.next()) {
                    fill(&takes, &cb_shared, l, r);
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            let device = default_device();
            let mut listening = false;
            if let Some(d) = device {
                let mut addr = address(kAudioDeviceProcessorOverload);
                // SAFETY: the listener and client data stay valid until `Drop` removes them.
                listening = unsafe {
                    AudioObjectAddPropertyListener(
                        d,
                        NonNull::from(&mut addr),
                        Some(on_overload),
                        Arc::as_ptr(&shared).cast_mut().cast(),
                    )
                } == 0;
            }
            unit.start().map_err(|e| e.to_string())?;
            Ok(Self {
                unit,
                shared,
                device,
                listening,
            })
        }

        /// The device playback started on.
        pub fn device(&self) -> Option<u32> {
            self.device
        }

        /// True when the default device changed or the device is gone (the spec's edge case).
        pub fn device_lost(&self) -> bool {
            match self.device {
                Some(d) => default_device() != Some(d) || !device_alive(d),
                None => false,
            }
        }
    }

    impl Drop for Player {
        fn drop(&mut self) {
            let _ = self.unit.stop();
            if let (true, Some(d)) = (self.listening, self.device) {
                let mut addr = address(kAudioDeviceProcessorOverload);
                // SAFETY: removes exactly the listener `start` added, with the same client data.
                unsafe {
                    AudioObjectRemovePropertyListener(
                        d,
                        NonNull::from(&mut addr),
                        Some(on_overload),
                        Arc::as_ptr(&self.shared).cast_mut().cast(),
                    );
                }
            }
        }
    }
}

/// Playback, or the reason it is unavailable.
pub struct Playback {
    #[cfg(target_os = "macos")]
    player: mac::Player,
}

impl Playback {
    /// Starts playback of `takes`; the callback reads `shared`.
    ///
    /// # Errors
    ///
    /// The platform's refusal, or `PLAYBACK_UNAVAILABLE` off macOS.
    pub fn start(takes: Takes, shared: Arc<Shared>) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self {
                player: mac::Player::start(takes, shared)?,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (takes, shared);
            Err(workbench_core::text::PLAYBACK_UNAVAILABLE.to_owned())
        }
    }

    /// True when the output device changed or was lost.
    pub fn device_lost(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.player.device_lost()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// The device's nominal rate, for the rate note.
    pub fn device_rate(&self) -> Option<f64> {
        #[cfg(target_os = "macos")]
        {
            self.player.device().and_then(mac::nominal_rate)
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}
