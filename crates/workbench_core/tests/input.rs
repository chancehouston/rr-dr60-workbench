//! Spec 001 T007 [TR: FR-001, D1, D3, US1 AS4; FR-002; FR-003, US1 AS3]: opening, refusals,
//! the conversion rules and the digests.

mod support;

use support::*;
use workbench_core::input::{self, Channel, OpenError, SampleType};

fn open(bytes: Vec<u8>, ch: Channel) -> Result<input::Input, OpenError> {
    input::open_bytes("t", bytes, ch)
}

#[test]
fn refuses_formats_rates_and_widths() {
    let x = tone(48000, 0.05, 0.5);
    // MP3 and other non-containers.
    assert!(matches!(
        open(mp3_stub(), Channel::Index(0)),
        Err(OpenError::UnsupportedFormat(_))
    ));
    // A-law WAV.
    let alaw = wav(8000, 1, WavKind::ALaw, &[0x55; 400], false);
    assert!(matches!(
        open(alaw, Channel::Index(0)),
        Err(OpenError::UnsupportedFormat(_))
    ));
    // 8-bit WAV.
    let codes: Vec<i32> = (0..400).map(|i| i % 256).collect();
    let w8 = wav(
        48000,
        1,
        WavKind::Int {
            container: 8,
            valid: 8,
            extensible: false,
        },
        &le_codes(&[codes], 1),
        false,
    );
    assert!(open(w8, Channel::Index(0)).is_err());
    // 20-bit FLAC.
    let c20: Vec<i32> = to_i24(&x).iter().map(|v| v >> 4).collect();
    let f20 = flac(48000, 20, &[c20]);
    assert!(matches!(
        open(f20, Channel::Index(0)),
        Err(OpenError::UnsupportedSampleType(_))
    ));
    // EXTENSIBLE with 20 valid bits in a 24-bit container (plan P13).
    let c = to_i24(&x);
    let e20 = wav(
        48000,
        1,
        WavKind::Int {
            container: 24,
            valid: 20,
            extensible: true,
        },
        &le_codes(&[c], 3),
        false,
    );
    assert!(
        open(e20, Channel::Index(0)).is_err(),
        "20-in-24 must be refused"
    );
    // 64-bit float.
    let f64data: Vec<u8> = x.iter().flat_map(|&v| f64::from(v).to_le_bytes()).collect();
    let w64 = wav(48000, 1, WavKind::Float { bits: 64 }, &f64data, false);
    assert!(open(w64, Channel::Index(0)).is_err());
    // 22.05 kHz.
    let w = wav_i16(22050, &to_i16(&x));
    let e = open(w, Channel::Index(0)).unwrap_err();
    assert_eq!(e, OpenError::UnsupportedRate(22050));
    assert!(
        e.to_string().contains("48000"),
        "names the supported rates: {e}"
    );
}

#[test]
fn every_supported_rate_opens() {
    for rate in RATES {
        let x = to_i16(&tone(rate, 0.05, 0.5));
        let i = open(wav_i16(rate, &x), Channel::Index(0)).unwrap();
        assert_eq!(i.rate_hz, rate);
        assert_eq!(i.samples.len(), x.len());
    }
}

#[test]
fn conversion_rules() {
    // 16 bits: every code in a 2^16-step sweep, x / 2^15 exactly.
    let codes: Vec<i16> = (i16::MIN..=i16::MAX)
        .step_by(7)
        .chain([i16::MAX, -1, 0, 1])
        .collect();
    let i = open(wav_i16(48000, &codes), Channel::Index(0)).unwrap();
    assert_eq!(i.sample_type, SampleType::I16);
    for (s, &c) in i.samples.iter().zip(&codes) {
        assert_eq!(*s, f32::from(c) / 32768.0);
    }
    // 24 bits in every container equals the engine's own rule.
    let c24: Vec<i32> = (-8_388_608..=8_388_607)
        .step_by(256 * 7 + 1)
        .chain([-8_388_608, -1, 0, 1, 8_388_607])
        .collect();
    let mut want = vec![0.0f32; c24.len()];
    rr_dr60::input::samples_from_i24(&c24, &mut want);
    let containers: Vec<(&str, Vec<u8>)> = vec![
        ("wav", wav_i24(48000, &c24)),
        ("aiff", aiff(48000, 24, &c24, false)),
        ("flac", flac(48000, 24, std::slice::from_ref(&c24))),
    ];
    for (name, bytes) in containers {
        let i = open(bytes, Channel::Index(0)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(i.sample_type, SampleType::I24, "{name}");
        assert_eq!(
            &i.samples[..],
            &want[..],
            "{name}: 24 bits must equal samples_from_i24"
        );
    }
    // AIFF-C sowt: accepted if symphonia opens it, then equal.
    if let Ok(i) = open(aiff(48000, 24, &c24, true), Channel::Index(0)) {
        assert_eq!(&i.samples[..], &want[..], "sowt");
    }
    // FLAC 16 bits arrives shifted in 32 and is routed to the 16-bit rule.
    let c16: Vec<i32> = codes.iter().map(|&v| i32::from(v)).collect();
    let i = open(flac(48000, 16, &[c16]), Channel::Index(0)).unwrap();
    assert_eq!(i.sample_type, SampleType::I16);
    for (s, &c) in i.samples.iter().zip(&codes) {
        assert_eq!(*s, f32::from(c) / 32768.0);
    }
    // 32 bits: 2^31 - 1 becomes exactly 1.0; one rounding.
    let c32 = vec![i32::MAX, i32::MIN, 0, 1, -1, 123_456_789];
    let w = wav(
        48000,
        1,
        WavKind::Int {
            container: 32,
            valid: 32,
            extensible: true,
        },
        &le_codes(std::slice::from_ref(&c32), 4),
        false,
    );
    let i = open(w, Channel::Index(0)).unwrap();
    assert_eq!(i.sample_type, SampleType::I32);
    for (s, &c) in i.samples.iter().zip(&c32) {
        assert_eq!(*s, (f64::from(c) / 2_147_483_648.0) as f32);
    }
    assert_eq!(i.samples[0], 1.0);
    // Float passes unchanged, beyond ±1.0 included.
    let f = vec![0.25f32, -1.5, 2.0, f32::MIN_POSITIVE];
    let i = open(wav_f32(48000, &f), Channel::Index(0)).unwrap();
    assert_eq!(&i.samples[..], &f[..]);
}

#[test]
fn channels_and_mix() {
    let a = to_i16(&tone(16000, 0.05, 0.5));
    let b: Vec<i16> = a.iter().map(|v| v / 2).collect();
    let ch = vec![
        a.iter().map(|&v| i32::from(v)).collect::<Vec<_>>(),
        b.iter().map(|&v| i32::from(v)).collect::<Vec<_>>(),
    ];
    let w = wav(
        16000,
        2,
        WavKind::Int {
            container: 16,
            valid: 16,
            extensible: false,
        },
        &le_codes(&ch, 2),
        false,
    );
    let i1 = open(w.clone(), Channel::Index(0)).unwrap();
    assert_eq!(i1.channels, 2);
    for (s, &c) in i1.samples.iter().zip(&a) {
        assert_eq!(*s, f32::from(c) / 32768.0);
    }
    let i2 = open(w.clone(), Channel::Index(1)).unwrap();
    for (s, &c) in i2.samples.iter().zip(&b) {
        assert_eq!(*s, f32::from(c) / 32768.0);
    }
    let m = open(w.clone(), Channel::Mix).unwrap();
    for ((s, &x), &y) in m.samples.iter().zip(&a).zip(&b) {
        let want = ((i64::from(x) + i64::from(y)) as f64 / 2.0 / 32768.0) as f32;
        assert_eq!(*s, want);
    }
    assert!(matches!(
        open(w, Channel::Index(2)),
        Err(OpenError::NoSuchChannel(2))
    ));
    assert!(workbench_core::text::MIX_LABEL.contains("boundary"));
}

#[test]
fn digests() {
    let x = to_i16(&tone(8000, 0.05, 0.5));
    let bytes = wav_i16(8000, &x);
    let i = open(bytes.clone(), Channel::Index(0)).unwrap();
    use sha2::{Digest, Sha256};
    let raw: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(i.raw_sha256, raw);
    assert_eq!(i.input_sha256, input::samples_sha256(&i.samples));
    // Same samples in another container: same converted digest, different raw digest.
    let c: Vec<i32> = x.iter().map(|&v| i32::from(v)).collect();
    let j = open(flac(8000, 16, &[c]), Channel::Index(0)).unwrap();
    assert_eq!(i.input_sha256, j.input_sha256);
    assert_ne!(i.raw_sha256, j.raw_sha256);
}

#[test]
fn counts_non_finite() {
    let f = vec![0.0f32, f32::NAN, f32::INFINITY, -f32::INFINITY, 1e-40, 0.5];
    let i = open(wav_f32(48000, &f), Channel::Index(0)).unwrap();
    assert_eq!(i.non_finite, 3);
    assert_eq!(i.subnormal, 1);
    assert!(i.samples[1].is_nan(), "passed unchanged");
}
