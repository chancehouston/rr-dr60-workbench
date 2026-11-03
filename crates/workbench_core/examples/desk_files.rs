//! Writes the desk checklist's files (spec 001 quickstart) into a folder:
//! `cargo run -p workbench_core --example desk_files -- out/desk`.
//!
//! Speech-like bursts with pauses (a 150 Hz pulse train through a 700 Hz resonance, 0.4 s on,
//! 1.6 s off), stereo at 48 kHz, mono at 6 kHz, a ten-minute file for the cancel step, a loud
//! file for the peak warning, and three files the session must refuse.

use std::f64::consts::PI;
use std::path::Path;

fn voiced(rate: u32, secs: f64, amp: f64, period_s: f64) -> Vec<f64> {
    let fs = f64::from(rate);
    let n = (fs * secs) as usize;
    let pitch = (fs / 150.0).round() as usize;
    let w = 2.0 * PI * 700.0f64.min(fs * 0.4) / fs;
    let r = 0.97f64;
    let (a1, a2) = (2.0 * r * w.cos(), -r * r);
    let (mut y1, mut y2) = (0.0, 0.0);
    let mut v: Vec<f64> = (0..n)
        .map(|i| {
            let on = (i as f64 / fs) % period_s < 0.4;
            let x = if on && i % pitch == 0 { 1.0 } else { 0.0 };
            let y = x + a1 * y1 + a2 * y2;
            y2 = y1;
            y1 = y;
            y
        })
        .collect();
    let peak = v.iter().fold(0.0f64, |m, x| m.max(x.abs())).max(1e-12);
    v.iter_mut().for_each(|x| *x *= amp / peak);
    v
}

fn write(path: &Path, rate: u32, bits: u16, channels: &[Vec<f64>]) {
    let spec = hound::WavSpec {
        channels: channels.len() as u16,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create");
    let full = f64::from(1u32 << (bits - 1)) - 1.0;
    for i in 0..channels[0].len() {
        for c in channels {
            let s = (c[i] * full).round() as i32;
            if bits == 8 {
                w.write_sample(s as i8).unwrap();
            } else {
                w.write_sample(s).unwrap();
            }
        }
    }
    w.finalize().unwrap();
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/desk".to_owned());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("folder");
    let l = voiced(48000, 20.0, 0.5, 2.0);
    let r: Vec<f64> = voiced(48000, 20.0, 0.3, 2.0).iter().map(|x| -x).collect();
    write(&dir.join("speech-stereo-48k.wav"), 48000, 24, &[l, r]);
    write(
        &dir.join("speech-6k.wav"),
        6000,
        16,
        &[voiced(6000, 20.0, 0.5, 2.0)],
    );
    write(
        &dir.join("long-10min-48k.wav"),
        48000,
        16,
        &[voiced(48000, 600.0, 0.5, 2.0)],
    );
    write(
        &dir.join("loud-48k.wav"),
        48000,
        16,
        &[voiced(48000, 10.0, 1.0, 2.0)],
    );
    write(
        &dir.join("refuse-22050.wav"),
        22050,
        16,
        &[voiced(22050, 2.0, 0.5, 2.0)],
    );
    write(
        &dir.join("refuse-8bit.wav"),
        48000,
        8,
        &[voiced(48000, 2.0, 0.5, 2.0)],
    );
    let mut mp3 = b"ID3\x03\x00\x00\x00\x00\x00\x00".to_vec();
    mp3.extend([0xFF, 0xFB, 0x90, 0x64]);
    mp3.extend(vec![0u8; 400]);
    std::fs::write(dir.join("refuse.mp3"), mp3).unwrap();
    println!("desk files written to {}", dir.display());
}
