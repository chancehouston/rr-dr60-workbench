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

/// The mean of integer channels: summed exactly in `i64`, divided by the channel count and by
/// `scale` = 2^(b−1) in `f64`, then rounded to `f32` (research R-02). With a channel count that is
/// not a power of two the `f64` divide rounds once before the cast: deterministic, and far below
/// one `f32` step.
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

/// One frame's float mix (spec 007 D1): exactly [`mix_floats`]'s rule on one frame, the same
/// `Iterator::sum` in channel order (its neutral element keeps an all-−0.0 frame at −0.0;
/// review Q3), divided in f64, rounded to f32. Real-time safe.
pub fn mix_frame(frame: &[f32]) -> f32 {
    let sum: f64 = frame.iter().map(|&v| f64::from(v)).sum();
    (sum / frame.len() as f64) as f32
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
    use std::io::Read;
    let io = |e: std::io::Error| OpenError::Io(e.to_string());
    let file_name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    // The raw digest in a streaming pass, and the first bytes for the container (convergence
    // T038: no whole-file buffer).
    let mut f = fs::File::open(path).map_err(io)?;
    let mut h = Sha256::new();
    let mut head = Vec::with_capacity(12);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        if head.len() < 12 {
            let take = (12 - head.len()).min(n);
            head.extend_from_slice(&buf[..take]);
        }
        h.update(&buf[..n]);
    }
    let raw_sha256: [u8; 32] = h.finalize().into();
    let file = fs::File::open(path).map_err(io)?;
    decode(&file_name, Box::new(file), &head, raw_sha256, channel)
}

/// Opens in-memory file bytes (the tests' entry point).
///
/// # Errors
///
/// As [`open`].
pub fn open_bytes(file_name: &str, bytes: Vec<u8>, channel: Channel) -> Result<Input, OpenError> {
    let raw_sha256: [u8; 32] = Sha256::digest(&bytes).into();
    let head = bytes.get(..12).unwrap_or(&bytes).to_vec();
    decode(
        file_name,
        Box::new(Cursor::new(bytes)),
        &head,
        raw_sha256,
        channel,
    )
}

fn decode(
    file_name: &str,
    source: Box<dyn symphonia::core::io::MediaSource>,
    head: &[u8],
    raw_sha256: [u8; 32],
    channel: Channel,
) -> Result<Input, OpenError> {
    let format = sniff(head)?;
    let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
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
    let reserve = track.num_frames.unwrap_or(0).min(1 << 31) as usize;
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

    let mut acc = Accumulator::new(channel, channels, reserve);
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
        acc.push(buf, bits, format)?;
    }
    let sample_type = match acc.sample_type {
        Some(t) => t,
        None => empty_type(bits, format)?,
    };
    let frames = acc.frames;
    let samples: Arc<[f32]> = acc.out.into();
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

/// Converts each decoded packet straight into the engine's input (convergence T038): one f32
/// output, packet-sized temporaries only; the mix is summed per packet.
struct Accumulator {
    channel: Channel,
    channels: u16,
    out: Vec<f32>,
    frames: u64,
    sample_type: Option<SampleType>,
    ints: Vec<Vec<i32>>,
}

impl Accumulator {
    fn new(channel: Channel, channels: u16, reserve: usize) -> Self {
        let n = match channel {
            Channel::Index(_) => 1,
            Channel::Mix => usize::from(channels),
        };
        Self {
            channel,
            channels,
            out: Vec::with_capacity(reserve),
            frames: 0,
            sample_type: None,
            ints: vec![Vec::new(); n],
        }
    }

    fn wanted(&self) -> Vec<usize> {
        match self.channel {
            Channel::Index(c) => vec![usize::from(c)],
            Channel::Mix => (0..usize::from(self.channels)).collect(),
        }
    }

    fn set_type(&mut self, st: SampleType) -> Result<(), OpenError> {
        if self.sample_type.is_some_and(|t| t != st) {
            return Err(OpenError::Decode("sample type changed mid-stream".into()));
        }
        self.sample_type = Some(st);
        Ok(())
    }

    /// Takes one decoded buffer.
    fn push(
        &mut self,
        buf: GenericAudioBufferRef<'_>,
        bits: Option<u32>,
        format: Format,
    ) -> Result<(), OpenError> {
        let wanted = self.wanted();
        for v in &mut self.ints {
            v.clear();
        }
        let (frames, st) = match &buf {
            GenericAudioBufferRef::S16(b) => {
                check_bits(bits, 16)?;
                for (k, &c) in wanted.iter().enumerate() {
                    let p = b.plane(c).unwrap_or(&[]);
                    self.ints[k].extend(p.iter().map(|&s| i32::from(s)));
                }
                (b.frames(), SampleType::I16)
            }
            GenericAudioBufferRef::S24(b) => {
                check_bits(bits, 24)?;
                for (k, &c) in wanted.iter().enumerate() {
                    let p = b.plane(c).unwrap_or(&[]);
                    self.ints[k].extend(p.iter().map(|s| s.inner()));
                }
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
                self.set_type(SampleType::F32)?;
                if self.channel == Channel::Mix {
                    let planes: Vec<&[f32]> =
                        wanted.iter().map(|&c| b.plane(c).unwrap_or(&[])).collect();
                    self.out.extend(mix_floats(&planes));
                } else {
                    self.out
                        .extend_from_slice(b.plane(wanted[0]).unwrap_or(&[]));
                }
                self.frames += b.frames() as u64;
                return Ok(());
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
        self.set_type(st)?;
        self.frames += frames as u64;
        let scale = match st {
            SampleType::I16 => 32768.0,
            SampleType::I24 => 8_388_608.0,
            _ => 2_147_483_648.0,
        };
        if self.channel == Channel::Mix {
            let refs: Vec<&[i32]> = self.ints.iter().map(Vec::as_slice).collect();
            self.out.extend(mix_integers(&refs, scale));
            return Ok(());
        }
        let src = &self.ints[0];
        let start = self.out.len();
        self.out.resize(start + src.len(), 0.0);
        let dst = &mut self.out[start..];
        match st {
            SampleType::I16 => {
                let narrow: Vec<i16> = src.iter().map(|&s| s as i16).collect();
                convert_i16(&narrow, dst);
            }
            SampleType::I24 => convert_i24(src, dst),
            _ => convert_i32(src, dst),
        }
        Ok(())
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
