//! Opening a file and converting one channel to the engine's input (spec 001 FR-001 – FR-003;
//! research R-01, R-02, R-15).
//!
//! The decoder is symphonia with only WAV, AIFF, FLAC and PCM. Samples are routed by the codec's
//! bits per sample, never by the buffer type: FLAC delivers 16- and 24-bit samples shifted into
//! 32 bits (R-01).

use std::fmt;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use symphonia::core::audio::{Audio, GenericAudioBufferRef};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::well_known::{CODEC_ID_PCM_ALAW, CODEC_ID_PCM_MULAW};
use symphonia::core::codecs::audio::{AudioCodecId, AudioDecoderOptions};
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

use crate::text;

/// The container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// RIFF WAVE.
    Wav,
    /// AIFF or AIFF-C.
    Aiff,
    /// FLAC.
    Flac,
}

/// The decoded sample type after routing by bits per sample (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleType {
    /// 16-bit integer.
    I16,
    /// 24-bit integer.
    I24,
    /// 32-bit integer (one rounding to f32, labelled).
    I32,
    /// 32-bit float.
    F32,
}

/// Which channel feeds the engine (D2). `Index(0)` is "channel 1" on screen and in the sidecar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// One channel, 0-based.
    Index(u16),
    /// The mean of all channels, a boundary conversion.
    Mix,
}

/// A refusal to open a file (FR-001). Each message names what is supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The file could not be read.
    Io(String),
    /// Not WAV, AIFF or FLAC, or a lossy or companded codec.
    UnsupportedFormat(String),
    /// A sample type other than 16/24/32-bit integer or 32-bit float.
    UnsupportedSampleType(String),
    /// A rate outside the engine's supported host rates.
    UnsupportedRate(u32),
    /// The channel asked for does not exist.
    NoSuchChannel(u16),
    /// The decoder failed part-way.
    Decode(String),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{}: {e}", text::OPEN_IO),
            Self::UnsupportedFormat(what) => {
                write!(
                    f,
                    "{} ({what}). {}",
                    text::OPEN_FORMAT,
                    text::SUPPORTED_FORMATS
                )
            }
            Self::UnsupportedSampleType(what) => {
                write!(
                    f,
                    "{} ({what}). {}",
                    text::OPEN_SAMPLE_TYPE,
                    text::SUPPORTED_SAMPLE_TYPES
                )
            }
            Self::UnsupportedRate(r) => {
                write!(
                    f,
                    "{} ({r} Hz). {}",
                    text::OPEN_RATE,
                    text::supported_rates()
                )
            }
            Self::NoSuchChannel(c) => write!(f, "{} {}", text::OPEN_CHANNEL, c + 1),
            Self::Decode(e) => write!(f, "{}: {e}", text::OPEN_DECODE),
        }
    }
}

impl std::error::Error for OpenError {}

/// An opened file and its selected channel as the engine's input (data-model "Input").
#[derive(Clone, Debug)]
pub struct Input {
    /// The file name, without directories.
    pub file_name: String,
    /// The container.
    pub format: Format,
    /// The decoded sample type.
    pub sample_type: SampleType,
    /// The sample rate, one of the engine's supported host rates.
    pub rate_hz: u32,
    /// The channel count.
    pub channels: u16,
    /// Frames per channel.
    pub frames: u64,
    /// The selected channel.
    pub channel: Channel,
    /// The selected channel after FR-002's conversion, built once.
    pub samples: Arc<[f32]>,
    /// SHA-256 of the file's bytes (informational, D9).
    pub raw_sha256: [u8; 32],
    /// SHA-256 of `samples` as f32 little-endian bytes: decides "same input" (D9).
    pub input_sha256: [u8; 32],
    /// NaN and infinite samples in `samples` (the engine zeroes them; counted, passed unchanged).
    pub non_finite: u64,
    /// Subnormal samples in `samples` (likewise).
    pub subnormal: u64,
}

/// 16-bit integers to the engine's input: x ÷ 2¹⁵, exact (FR-002).
pub fn convert_i16(src: &[i16], dst: &mut [f32]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = f32::from(s) / 32768.0;
    }
}

/// 24-bit integers (sign-extended in `i32`) to the engine's input: the engine's own published
/// feed-point rule, called, not copied (FR-002; engine spec 020).
pub fn convert_i24(src: &[i32], dst: &mut [f32]) {
    rr_dr60::input::samples_from_i24(src, dst);
}

/// 32-bit integers to the engine's input: x ÷ 2³¹ rounded once to nearest even, a labelled
/// boundary conversion (FR-002).
pub fn convert_i32(src: &[i32], dst: &mut [f32]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d = (f64::from(s) / 2_147_483_648.0) as f32;
    }
}

/// The mean of integer channels already scaled to the container's full scale: summed exactly in
/// `i64`, divided in `f64`, rounded once to `f32` (research R-02, B9). `scale` is 2^(b−1).
pub fn mix_integers(channels: &[&[i32]], scale: f64) -> Vec<f32> {
    let n = channels.first().map_or(0, |c| c.len());
    let count = channels.len() as f64;
    (0..n)
        .map(|i| {
            let sum: i64 = channels.iter().map(|c| i64::from(c[i])).sum();
            (sum as f64 / count / scale) as f32
        })
        .collect()
}

/// The mean of float channels: summed in `f64` in channel order, divided in `f64`, rounded to
/// `f32` (deterministic; the spec's dated restatement of FR-002).
pub fn mix_floats(channels: &[&[f32]]) -> Vec<f32> {
    let n = channels.first().map_or(0, |c| c.len());
    let count = channels.len() as f64;
    (0..n)
        .map(|i| {
            let sum: f64 = channels.iter().map(|c| f64::from(c[i])).sum();
            (sum / count) as f32
        })
        .collect()
}

/// SHA-256 of samples as f32 little-endian bytes (R-11).
pub fn samples_sha256(samples: &[f32]) -> [u8; 32] {
    let mut h = Sha256::new();
    for s in samples {
        h.update(s.to_le_bytes());
    }
    h.finalize().into()
}

/// Lowercase hex of a digest.
pub fn hex(d: &[u8; 32]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Opens `path` and converts `channel` (FR-001 – FR-003).
///
/// # Errors
///
/// Every refusal of FR-001, a missing channel, and read or decode failures.
pub fn open(path: &Path, channel: Channel) -> Result<Input, OpenError> {
    let bytes = fs::read(path).map_err(|e| OpenError::Io(e.to_string()))?;
    let file_name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    open_bytes(&file_name, bytes, channel)
}

/// Opens in-memory file bytes (the tests' entry point; [`open`] reads the file and calls this).
///
/// # Errors
///
/// As [`open`].
pub fn open_bytes(file_name: &str, bytes: Vec<u8>, channel: Channel) -> Result<Input, OpenError> {
    let raw_sha256: [u8; 32] = Sha256::digest(&bytes).into();
    let format = sniff(&bytes)?;
    let mss = MediaSourceStream::new(
        Box::new(Cursor::new(bytes)),
        MediaSourceStreamOptions::default(),
    );
    let mut reader = symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| OpenError::UnsupportedFormat(e.to_string()))?;
    let track = reader
        .default_track(TrackType::Audio)
        .ok_or_else(|| OpenError::UnsupportedFormat("no audio track".into()))?;
    let track_id = track.id;
    let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
        return Err(OpenError::UnsupportedFormat("no audio parameters".into()));
    };
    refuse_codec(params.codec)?;
    let rate_hz = params
        .sample_rate
        .ok_or_else(|| OpenError::UnsupportedFormat("no sample rate".into()))?;
    if !rr_dr60::SUPPORTED_HOST_RATES.contains(&rate_hz) {
        return Err(OpenError::UnsupportedRate(rate_hz));
    }
    let channels = params.channels.as_ref().map_or(0, |c| c.count()) as u16;
    if channels == 0 {
        return Err(OpenError::UnsupportedFormat("no channels".into()));
    }
    if let Channel::Index(c) = channel
        && c >= channels
    {
        return Err(OpenError::NoSuchChannel(c));
    }
    let bits = params.bits_per_sample;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| OpenError::UnsupportedFormat(e.to_string()))?;

    let mut acc = Accumulator::new(channel, channels);
    let mut sample_type = None;
    while let Some(packet) = reader
        .next_packet()
        .map_err(|e| OpenError::Decode(e.to_string()))?
    {
        if packet.track_id != track_id {
            continue;
        }
        let buf = decoder
            .decode(&packet)
            .map_err(|e| OpenError::Decode(e.to_string()))?;
        let st = acc.push(buf, bits, format)?;
        if sample_type.is_some_and(|t| t != st) {
            return Err(OpenError::Decode("sample type changed mid-stream".into()));
        }
        sample_type = Some(st);
    }
    let sample_type = match sample_type {
        Some(t) => t,
        None => empty_type(bits, format)?,
    };
    let frames = acc.frames;
    let samples: Arc<[f32]> = acc.finish(sample_type).into();
    let input_sha256 = samples_sha256(&samples);
    let non_finite = samples.iter().filter(|s| !s.is_finite()).count() as u64;
    let subnormal = samples.iter().filter(|s| s.is_subnormal()).count() as u64;
    Ok(Input {
        file_name: file_name.to_owned(),
        format,
        sample_type,
        rate_hz,
        channels,
        frames,
        channel,
        samples,
        raw_sha256,
        input_sha256,
        non_finite,
        subnormal,
    })
}

/// The container from the first bytes; anything else is refused before the probe (D1).
fn sniff(bytes: &[u8]) -> Result<Format, OpenError> {
    match bytes.get(..12) {
        Some(h) if &h[..4] == b"RIFF" && &h[8..12] == b"WAVE" => Ok(Format::Wav),
        Some(h) if &h[..4] == b"FORM" && (&h[8..12] == b"AIFF" || &h[8..12] == b"AIFC") => {
            Ok(Format::Aiff)
        }
        Some(h) if &h[..4] == b"fLaC" => Ok(Format::Flac),
        _ => Err(OpenError::UnsupportedFormat("not WAV, AIFF or FLAC".into())),
    }
}

/// Companded PCM is refused by codec id (R-01): `pcm` would decode it to 16 bits.
fn refuse_codec(codec: AudioCodecId) -> Result<(), OpenError> {
    if codec == CODEC_ID_PCM_ALAW || codec == CODEC_ID_PCM_MULAW {
        return Err(OpenError::UnsupportedFormat("A-law or µ-law".into()));
    }
    Ok(())
}

/// The sample type of a file with no packets, from its parameters.
fn empty_type(bits: Option<u32>, format: Format) -> Result<SampleType, OpenError> {
    match (bits, format) {
        (Some(16), _) => Ok(SampleType::I16),
        (Some(24), _) => Ok(SampleType::I24),
        (Some(32), Format::Flac) => Ok(SampleType::I32),
        (Some(32), _) => Ok(SampleType::F32),
        (b, _) => Err(OpenError::UnsupportedSampleType(format!("{b:?} bits"))),
    }
}

/// Collects the selected channel (or every channel for the mix) as integers or floats.
struct Accumulator {
    channel: Channel,
    channels: u16,
    ints: Vec<Vec<i32>>,
    floats: Vec<Vec<f32>>,
    frames: u64,
}

impl Accumulator {
    fn new(channel: Channel, channels: u16) -> Self {
        let n = match channel {
            Channel::Index(_) => 1,
            Channel::Mix => usize::from(channels),
        };
        Self {
            channel,
            channels,
            ints: vec![Vec::new(); n],
            floats: vec![Vec::new(); n],
            frames: 0,
        }
    }

    fn wanted(&self) -> Vec<usize> {
        match self.channel {
            Channel::Index(c) => vec![usize::from(c)],
            Channel::Mix => (0..usize::from(self.channels)).collect(),
        }
    }

    /// Takes one decoded buffer; returns its routed sample type.
    fn push(
        &mut self,
        buf: GenericAudioBufferRef<'_>,
        bits: Option<u32>,
        format: Format,
    ) -> Result<SampleType, OpenError> {
        let wanted = self.wanted();
        let frames = match &buf {
            GenericAudioBufferRef::S16(b) => {
                for (k, &c) in wanted.iter().enumerate() {
                    let p = b.plane(c).unwrap_or(&[]);
                    self.ints[k].extend(p.iter().map(|&s| i32::from(s)));
                }
                check_bits(bits, 16)?;
                (b.frames(), SampleType::I16)
            }
            GenericAudioBufferRef::S24(b) => {
                for (k, &c) in wanted.iter().enumerate() {
                    let p = b.plane(c).unwrap_or(&[]);
                    self.ints[k].extend(p.iter().map(|s| s.inner()));
                }
                check_bits(bits, 24)?;
                (b.frames(), SampleType::I24)
            }
            GenericAudioBufferRef::S32(b) => {
                // FLAC delivers b-bit samples shifted left by 32 − b (R-01, hardware review B2).
                let b_bits = bits.unwrap_or(32);
                let (shift, st) = match (format, b_bits) {
                    (_, 32) => (0, SampleType::I32),
                    (Format::Flac, 24) => (8, SampleType::I24),
                    (Format::Flac, 16) => (16, SampleType::I16),
                    (_, other) => {
                        return Err(OpenError::UnsupportedSampleType(format!(
                            "{other} valid bits in a 32-bit container"
                        )));
                    }
                };
                let mask = if shift == 0 { 0 } else { (1i32 << shift) - 1 };
                for (k, &c) in wanted.iter().enumerate() {
                    let p = b.plane(c).unwrap_or(&[]);
                    for &s in p {
                        if s & mask != 0 {
                            return Err(OpenError::Decode(
                                "low bits set below the declared width".into(),
                            ));
                        }
                        self.ints[k].push(s >> shift);
                    }
                }
                (b.frames(), st)
            }
            GenericAudioBufferRef::F32(b) => {
                for (k, &c) in wanted.iter().enumerate() {
                    let p = b.plane(c).unwrap_or(&[]);
                    self.floats[k].extend_from_slice(p);
                }
                (b.frames(), SampleType::F32)
            }
            GenericAudioBufferRef::U8(_) | GenericAudioBufferRef::S8(_) => {
                return Err(OpenError::UnsupportedSampleType("8-bit".into()));
            }
            GenericAudioBufferRef::F64(_) => {
                return Err(OpenError::UnsupportedSampleType("64-bit float".into()));
            }
            _ => {
                return Err(OpenError::UnsupportedSampleType("unsigned or other".into()));
            }
        };
        self.frames += frames.0 as u64;
        Ok(frames.1)
    }

    fn finish(self, st: SampleType) -> Vec<f32> {
        let scale = match st {
            SampleType::I16 => 32768.0,
            SampleType::I24 => 8_388_608.0,
            SampleType::I32 => 2_147_483_648.0,
            SampleType::F32 => 1.0,
        };
        if st == SampleType::F32 {
            if self.channel == Channel::Mix {
                let refs: Vec<&[f32]> = self.floats.iter().map(Vec::as_slice).collect();
                return mix_floats(&refs);
            }
            return self.floats.into_iter().next().unwrap_or_default();
        }
        if self.channel == Channel::Mix {
            let refs: Vec<&[i32]> = self.ints.iter().map(Vec::as_slice).collect();
            return mix_integers(&refs, scale);
        }
        let src = self.ints.into_iter().next().unwrap_or_default();
        let mut out = vec![0.0f32; src.len()];
        match st {
            SampleType::I16 => {
                for (d, &s) in out.iter_mut().zip(&src) {
                    *d = s as f32 / 32768.0;
                }
            }
            SampleType::I24 => convert_i24(&src, &mut out),
            SampleType::I32 => convert_i32(&src, &mut out),
            SampleType::F32 => unreachable!(),
        }
        out
    }
}

/// A PCM buffer's width must equal the declared bits per sample: fewer valid bits than the
/// container (e.g. 20 in 24) is refused in 001 (plan P13).
fn check_bits(bits: Option<u32>, container: u32) -> Result<(), OpenError> {
    match bits {
        Some(b) if b != container => Err(OpenError::UnsupportedSampleType(format!(
            "{b} valid bits in a {container}-bit container"
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i16_edges_exact() {
        let src = [i16::MIN, -1, 0, 1, i16::MAX];
        let mut d = [0.0; 5];
        convert_i16(&src, &mut d);
        assert_eq!(
            d,
            [-1.0, -1.0 / 32768.0, 0.0, 1.0 / 32768.0, 32767.0 / 32768.0]
        );
    }

    #[test]
    fn i32_max_rounds_to_one() {
        let mut d = [0.0; 2];
        convert_i32(&[i32::MAX, i32::MIN], &mut d);
        assert_eq!(d, [1.0, -1.0]);
    }
}
