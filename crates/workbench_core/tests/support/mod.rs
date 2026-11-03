//! Shared test support (spec 001 tasks T004): deterministic stimuli, hand-rolled file writers
//! (WAV, AIFF, AIFF-C `sowt`, FLAC with verbatim subframes) and the render oracle. Nothing here
//! uses OS randomness, and no audio file is committed: every fixture is built at test time.

#![allow(dead_code)]

use rr_dr60::control_map::ControlReading;
use rr_dr60::{Pipeline, Settings, VasEvent};

/// The engine's seven host rates.
pub const RATES: [u32; 7] = rr_dr60::SUPPORTED_HOST_RATES;

// ---- Stimuli -------------------------------------------------------------------------------

/// A 1 kHz tone at `amp` for `secs` seconds that ends with no trailing silence.
pub fn tone(rate: u32, secs: f64, amp: f32) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            (amp as f64 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin()) as f32
        })
        .collect()
}

/// White noise in ±amp from a fixed xorshift seed.
pub fn noise(rate: u32, secs: f64, amp: f32) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let u = (x >> 40) as f32 / (1u64 << 24) as f32; // [0, 1)
            amp * (2.0 * u - 1.0)
        })
        .collect()
}

/// A voiced burst with pauses: a 150 Hz pulse train through a two-pole resonance at 700 Hz,
/// 0.4 s on and 0.8 s off, repeated to `secs`, peaking near `amp`.
pub fn voiced(rate: u32, secs: f64, amp: f32) -> Vec<f32> {
    let fs = f64::from(rate);
    let n = (fs * secs) as usize;
    let period = (fs / 150.0).round() as usize;
    let r = 0.97f64;
    let w = 2.0 * std::f64::consts::PI * 700.0f64.min(fs * 0.4) / fs;
    let (a1, a2) = (2.0 * r * w.cos(), -r * r);
    let (mut y1, mut y2) = (0.0f64, 0.0f64);
    let mut raw = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f64 / fs;
        let on = (t % 1.2) < 0.4;
        let x = if on && i % period == 0 { 1.0 } else { 0.0 };
        let y = x + a1 * y1 + a2 * y2;
        y2 = y1;
        y1 = y;
        raw.push(y);
    }
    let peak = raw.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-12);
    raw.iter()
        .map(|v| (v / peak * f64::from(amp)) as f32)
        .collect()
}

/// Float samples to 24-bit codes (round to nearest, clamped).
pub fn to_i24(x: &[f32]) -> Vec<i32> {
    x.iter()
        .map(|&v| ((f64::from(v) * 8_388_608.0).round() as i64).clamp(-8_388_608, 8_388_607) as i32)
        .collect()
}

/// Float samples to 16-bit codes.
pub fn to_i16(x: &[f32]) -> Vec<i16> {
    x.iter()
        .map(|&v| ((f64::from(v) * 32768.0).round() as i64).clamp(-32768, 32767) as i16)
        .collect()
}

// ---- WAV -----------------------------------------------------------------------------------

/// How a WAV writes its samples.
#[derive(Clone, Copy, Debug)]
pub enum WavKind {
    /// PCM integer, format tag 1 (or EXTENSIBLE when `extensible`).
    Int {
        container: u16,
        valid: u16,
        extensible: bool,
    },
    /// IEEE float, 32 or 64 bits.
    Float { bits: u16 },
    /// A-law (format tag 6), 8 bits.
    ALaw,
}

/// A WAV file with interleaved `frames[frame][channel]` integer codes (or float bits) and an
/// optional LIST chunk before the data.
pub fn wav(rate: u32, channels: u16, kind: WavKind, data: &[u8], list_chunk: bool) -> Vec<u8> {
    let (tag, container, valid) = match kind {
        WavKind::Int {
            container,
            valid,
            extensible,
        } => (if extensible { 0xFFFE } else { 1 }, container, valid),
        WavKind::Float { bits } => (3, bits, bits),
        WavKind::ALaw => (6, 8, 8),
    };
    let block_align = channels * container / 8;
    let mut fmt = Vec::new();
    fmt.extend((tag as u16).to_le_bytes());
    fmt.extend(channels.to_le_bytes());
    fmt.extend(rate.to_le_bytes());
    fmt.extend((rate * u32::from(block_align)).to_le_bytes());
    fmt.extend(block_align.to_le_bytes());
    fmt.extend(container.to_le_bytes());
    if tag == 0xFFFE {
        fmt.extend(22u16.to_le_bytes());
        fmt.extend(valid.to_le_bytes());
        fmt.extend(0u32.to_le_bytes()); // channel mask
        // KSDATAFORMAT_SUBTYPE_PCM
        fmt.extend([
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ]);
    }
    let mut body = Vec::new();
    body.extend(b"WAVE");
    body.extend(b"fmt ");
    body.extend((fmt.len() as u32).to_le_bytes());
    body.extend(&fmt);
    if list_chunk {
        let info = b"INFOISFT\x06\x00\x00\x00bench\x00";
        body.extend(b"LIST");
        body.extend((info.len() as u32).to_le_bytes());
        body.extend(info);
    }
    body.extend(b"data");
    body.extend((data.len() as u32).to_le_bytes());
    body.extend(data);
    if data.len() % 2 == 1 {
        body.push(0);
    }
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}

/// Interleaves channels of integer codes into little-endian bytes of `bytes` per sample.
pub fn le_codes(channels: &[Vec<i32>], bytes: usize) -> Vec<u8> {
    let n = channels[0].len();
    let mut out = Vec::new();
    for i in 0..n {
        for c in channels {
            out.extend(&c[i].to_le_bytes()[..bytes]);
        }
    }
    out
}

/// Interleaves channels of f32 into little-endian bytes.
pub fn le_floats(channels: &[Vec<f32>]) -> Vec<u8> {
    let n = channels[0].len();
    let mut out = Vec::new();
    for i in 0..n {
        for c in channels {
            out.extend(c[i].to_le_bytes());
        }
    }
    out
}

/// A mono 16-bit WAV.
pub fn wav_i16(rate: u32, x: &[i16]) -> Vec<u8> {
    let codes: Vec<i32> = x.iter().map(|&v| i32::from(v)).collect();
    wav(
        rate,
        1,
        WavKind::Int {
            container: 16,
            valid: 16,
            extensible: false,
        },
        &le_codes(&[codes], 2),
        false,
    )
}

/// A mono 24-bit WAV.
pub fn wav_i24(rate: u32, x: &[i32]) -> Vec<u8> {
    wav(
        rate,
        1,
        WavKind::Int {
            container: 24,
            valid: 24,
            extensible: false,
        },
        &le_codes(&[x.to_vec()], 3),
        false,
    )
}

/// A mono f32 WAV.
pub fn wav_f32(rate: u32, x: &[f32]) -> Vec<u8> {
    wav(
        rate,
        1,
        WavKind::Float { bits: 32 },
        &le_floats(&[x.to_vec()]),
        false,
    )
}

// ---- AIFF ----------------------------------------------------------------------------------

/// An 80-bit IEEE extended float (AIFF's sample rate).
fn extended(v: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if v == 0 {
        return out;
    }
    let exp = 31 - v.leading_zeros();
    let biased = 16383 + exp;
    out[0] = (biased >> 8) as u8;
    out[1] = biased as u8;
    let mant = u64::from(v) << (63 - exp);
    out[2..].copy_from_slice(&mant.to_be_bytes());
    out
}

/// An AIFF (big-endian) or AIFF-C `sowt` (little-endian) file of mono integer codes.
pub fn aiff(rate: u32, bits: u16, codes: &[i32], sowt: bool) -> Vec<u8> {
    let bytes = usize::from(bits / 8);
    let mut comm = Vec::new();
    comm.extend(1u16.to_be_bytes());
    comm.extend((codes.len() as u32).to_be_bytes());
    comm.extend(bits.to_be_bytes());
    comm.extend(extended(rate));
    if sowt {
        comm.extend(b"sowt");
        let name = b"\x00"; // empty pascal string, padded to even
        comm.extend(name);
        comm.push(0);
    }
    let mut ssnd = vec![0u8; 8];
    for &c in codes {
        let be = c.to_be_bytes();
        let le = c.to_le_bytes();
        if sowt {
            ssnd.extend(&le[..bytes]);
        } else {
            ssnd.extend(&be[4 - bytes..]);
        }
    }
    let mut body = Vec::new();
    body.extend(if sowt { b"AIFC" } else { b"AIFF" });
    if sowt {
        body.extend(b"FVER");
        body.extend(4u32.to_be_bytes());
        body.extend(0xA280_5140u32.to_be_bytes());
    }
    body.extend(b"COMM");
    body.extend((comm.len() as u32).to_be_bytes());
    body.extend(&comm);
    body.extend(b"SSND");
    body.extend((ssnd.len() as u32).to_be_bytes());
    body.extend(&ssnd);
    if ssnd.len() % 2 == 1 {
        body.push(0);
    }
    let mut out = Vec::new();
    out.extend(b"FORM");
    out.extend((body.len() as u32).to_be_bytes());
    out.extend(body);
    out
}

// ---- FLAC (verbatim subframes) ---------------------------------------------------------------

struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            acc: 0,
            n: 0,
        }
    }
    fn put(&mut self, v: u64, bits: u32) {
        for i in (0..bits).rev() {
            self.acc = (self.acc << 1) | ((v >> i) & 1);
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    fn align(&mut self) {
        while self.n != 0 {
            self.put(0, 1);
        }
    }
}

fn crc8(data: &[u8]) -> u8 {
    let mut c = 0u8;
    for &b in data {
        c ^= b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 {
                (c << 1) ^ 0x07
            } else {
                c << 1
            };
        }
    }
    c
}

fn crc16(data: &[u8]) -> u16 {
    let mut c = 0u16;
    for &b in data {
        c ^= u16::from(b) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 {
                (c << 1) ^ 0x8005
            } else {
                c << 1
            };
        }
    }
    c
}

/// A FLAC file of `channels` channels of `bits`-bit codes (independent channels, verbatim
/// subframes, blocks of 4096, a correct STREAMINFO with an unknown MD5).
pub fn flac(rate: u32, bits: u32, channels: &[Vec<i32>]) -> Vec<u8> {
    let n = channels[0].len();
    let nch = channels.len() as u64;
    let block = 4096usize;
    let mut out = Vec::new();
    out.extend(b"fLaC");
    let mut si = Bits::new();
    si.put(u64::from(1u8), 1); // last metadata block
    si.put(0, 7); // STREAMINFO
    si.put(34, 24);
    si.put(block as u64, 16); // min block size = max (fixed blocking; the last block may be shorter)
    si.put(block as u64, 16); // max block size
    si.put(0, 24); // min frame size unknown
    si.put(0, 24); // max frame size unknown
    si.put(u64::from(rate), 20);
    si.put(nch - 1, 3);
    si.put(u64::from(bits - 1), 5);
    si.put(n as u64, 36);
    for _ in 0..16 {
        si.put(0, 8);
    }
    out.extend(si.out);
    let size_code = match bits {
        8 => 0b001,
        12 => 0b010,
        16 => 0b100,
        20 => 0b101,
        24 => 0b110,
        _ => 0b000,
    };
    let mut start = 0usize;
    let mut frame_no = 0u64;
    while start < n {
        let len = block.min(n - start);
        let mut f = Bits::new();
        f.put(0b11_1111_1111_1110, 14);
        f.put(0, 1); // reserved
        f.put(0, 1); // fixed blocking
        f.put(0b0111, 4); // block size: 16 bits at the end of the header
        f.put(0b0000, 4); // sample rate from STREAMINFO
        f.put(nch - 1, 4); // independent channels
        f.put(size_code, 3);
        f.put(0, 1);
        assert!(frame_no < 128, "fixture longer than 128 frames");
        f.put(frame_no, 8); // UTF-8 coded frame number (< 128: one byte)
        f.put(len as u64 - 1, 16);
        let c8 = crc8(&f.out);
        f.put(u64::from(c8), 8);
        for ch in channels {
            f.put(0, 1); // padding
            f.put(0b000001, 6); // VERBATIM
            f.put(0, 1); // no wasted bits
            for &s in &ch[start..start + len] {
                let mask = (1u64 << bits) - 1;
                f.put((s as i64 as u64) & mask, bits);
            }
        }
        f.align();
        let c16 = crc16(&f.out);
        f.put(u64::from(c16), 16);
        out.extend(f.out);
        start += len;
        frame_no += 1;
    }
    out
}

/// A few bytes that look like an MP3 frame header (an ID3 tag, then a frame sync).
pub fn mp3_stub() -> Vec<u8> {
    let mut v = b"ID3\x03\x00\x00\x00\x00\x00\x00".to_vec();
    v.extend([0xFF, 0xFB, 0x90, 0x64]);
    v.extend(vec![0u8; 400]);
    v
}

// ---- The oracle (research R-09) --------------------------------------------------------------

/// The output of a fresh engine with `settings`, the engine values applied in the render's order
/// (the slot, its parameters, the route, the volume, the battery), over `input` processed in
/// blocks of `block` (0 = one whole-input call). Returns the samples and the reports.
pub fn oracle(
    settings: Settings,
    engine_values: &[(u32, ControlReading)],
    input: &[f32],
    block: usize,
) -> (Vec<f32>, Vec<VasEvent>) {
    let mut p = Pipeline::new(settings).expect("valid settings");
    assert_eq!(p.recording_counter(), 0);
    for &(i, v) in engine_values {
        p.set_control_value(i, v).expect("engine value");
    }
    let block = if block == 0 {
        input.len().max(1)
    } else {
        block
    };
    let mut out = Vec::with_capacity(input.len());
    let mut events = Vec::new();
    let mut buf = vec![0.0f32; block];
    let mut ev = vec![VasEvent::new(0, 0); p.max_events(block).max(1)];
    for chunk in input.chunks(block) {
        let o = &mut buf[..chunk.len()];
        let info = p.process_with_events(chunk, o, &mut ev).expect("process");
        out.extend_from_slice(&o[..info.produced]);
        events.extend_from_slice(&ev[..info.events]);
    }
    (out, events)
}

// ---- Requests --------------------------------------------------------------------------------

/// A render request for `input` with the default panel at `rate`, edited by `edit`.
pub fn request(
    rate: u32,
    input: &[f32],
    edit: impl FnOnce(&mut workbench_core::panel::Panel),
) -> workbench_core::render::RenderRequest {
    let mut panel = workbench_core::panel::Panel::new(rate).expect("panel");
    edit(&mut panel);
    assert!(panel.renderable(), "test panel must be renderable");
    let (settings, engine_values) = panel.snapshot();
    let input: std::sync::Arc<[f32]> = input.to_vec().into();
    let sha = workbench_core::input::samples_sha256(&input);
    workbench_core::render::RenderRequest {
        input_sha256: sha,
        file_name: "stimulus".into(),
        frames: input.len() as u64,
        raw_sha256: sha,
        rate_hz: rate,
        input,
        channel: workbench_core::input::Channel::Index(0),
        settings,
        engine_values,
        tail: true,
        block: workbench_core::render::block_for(rate),
        start: 0,
        len: None,
        changes: Vec::new(),
    }
}

/// Renders a request to completion.
pub fn run(req: &workbench_core::render::RenderRequest) -> workbench_core::render::Render {
    use std::sync::atomic::{AtomicBool, AtomicU64};
    workbench_core::render::render(req, &AtomicU64::new(0), &AtomicBool::new(false))
        .expect("render")
        .expect("not cancelled")
}

/// The input plus the render's tail of zeros (what the oracle must be fed).
pub fn with_tail(input: &[f32], tail: u64) -> Vec<f32> {
    let mut v = input.to_vec();
    v.resize(input.len() + tail as usize, 0.0);
    v
}

/// Sets a choice control by option identifier.
pub fn set_choice(panel: &mut workbench_core::panel::Panel, id: &str, option: &str) {
    let row = panel.row(id).expect("control").clone();
    let rr_dr60::controls::ControlValue::Choice { options, .. } = row.entry.value else {
        panic!("{id} is not a choice");
    };
    let v = options
        .iter()
        .find(|o| o.identifier == option)
        .expect("option")
        .value;
    assert!(
        panel.set(row.index, ControlReading::Choice(v)),
        "{id} settable"
    );
}

/// Sets any control by identifier.
pub fn set(panel: &mut workbench_core::panel::Panel, id: &str, v: ControlReading) {
    let i = panel.index_of(id).expect("control");
    assert!(panel.set(i, v), "{id} settable");
}

/// Sets a numeric control by identifier, in its own value type.
pub fn set_num(panel: &mut workbench_core::panel::Panel, id: &str, v: f64) {
    let row = panel.row(id).expect("control").clone();
    let r = match row.entry.value {
        rr_dr60::controls::ControlValue::Integer { .. } => ControlReading::Integer(v as u64),
        rr_dr60::controls::ControlValue::Continuous { .. } => ControlReading::Continuous(v as f32),
        rr_dr60::controls::ControlValue::Choice { .. } => ControlReading::Choice(v as u32),
        rr_dr60::controls::ControlValue::Switch { .. } => ControlReading::Switch(v != 0.0),
        _ => panic!("{id}: unknown type"),
    };
    assert!(panel.set(row.index, r), "{id} settable");
}

/// Sets a switch by identifier.
pub fn set_switch(panel: &mut workbench_core::panel::Panel, id: &str, on: bool) {
    set(panel, id, ControlReading::Switch(on));
}

/// An opened input from a 24-bit WAV of `x` at `rate`.
pub fn input_of(rate: u32, x: &[f32]) -> workbench_core::input::Input {
    workbench_core::input::open_bytes(
        "voice.wav",
        wav_i24(rate, &to_i24(x)),
        workbench_core::input::Channel::Index(0),
    )
    .expect("open")
}

/// A render request built from an opened input and a panel.
pub fn request_for(
    input: &workbench_core::input::Input,
    panel: &workbench_core::panel::Panel,
    tail: bool,
) -> workbench_core::render::RenderRequest {
    workbench_core::render::RenderRequest::new(input, panel, tail)
}
