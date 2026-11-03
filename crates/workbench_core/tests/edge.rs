//! Spec 001 T032 [TR: SC-006, edge cases].

mod support;

use support::*;
use workbench_core::input::{self, Channel};
use workbench_core::panel::Panel;

#[test]
fn empty_file() {
    let i = input::open_bytes("e.wav", wav_i16(48000, &[]), Channel::Index(0)).unwrap();
    assert_eq!(i.samples.len(), 0);
    let p = Panel::new(48000).unwrap();
    let r = run(&request_for(&i, &p, true));
    let (o, _) = oracle(
        r.request.settings,
        &r.request.engine_values,
        &with_tail(&[], r.tail_samples),
        0,
    );
    assert_eq!(&r.samples[..], &o[..], "the tail's output only");
    let mut no_tail = request_for(&i, &p, false);
    no_tail.tail = false;
    assert!(run(&no_tail).samples.is_empty());
    let text = workbench_core::sidecar::write(&i, &r);
    assert!(workbench_core::sidecar::read(&text).is_ok());
}

#[test]
fn metadata_and_many_channels() {
    let rate = 16000;
    let chans: Vec<Vec<i32>> = (0..10)
        .map(|c| {
            tone(rate, 0.05, 0.1 + 0.05 * c as f32)
                .iter()
                .map(|&v| (v * 30000.0) as i32)
                .collect()
        })
        .collect();
    let w = wav(
        rate,
        10,
        WavKind::Int {
            container: 16,
            valid: 16,
            extensible: true,
        },
        &le_codes(&chans, 2),
        true,
    );
    for c in 0..10u16 {
        let i = input::open_bytes("m.wav", w.clone(), Channel::Index(c)).unwrap();
        assert_eq!(i.channels, 10);
        assert_eq!(i.samples.len(), chans[c as usize].len());
        assert_eq!(
            i.samples[5],
            chans[c as usize][5] as f32 / 32768.0,
            "channel {c}"
        );
    }
    assert!(input::open_bytes("m.wav", w, Channel::Mix).is_ok());
}

#[test]
fn non_finite_counted_and_passed() {
    let rate = 48000;
    let mut x = tone(rate, 0.2, 0.5);
    x[100] = f32::NAN;
    x[200] = f32::INFINITY;
    x[300] = 1e-41;
    let i = input::open_bytes("n.wav", wav_f32(rate, &x), Channel::Index(0)).unwrap();
    assert_eq!((i.non_finite, i.subnormal), (2, 1));
    let p = Panel::new(rate).unwrap();
    let r = run(&request_for(&i, &p, true));
    let (o, _) = oracle(
        r.request.settings,
        &r.request.engine_values,
        &with_tail(&x, r.tail_samples),
        0,
    );
    assert_eq!(r.samples.len(), o.len());
    assert!(
        r.samples
            .iter()
            .zip(&o)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "passed unchanged to the engine"
    );
}

#[test]
fn refusals_leave_session_unchanged() {
    // Format, rate and sidecar refusals return an error and build nothing; the panel's state is
    // untouched by them (the panel and sidecar tests cover values and sidecars in depth).
    let p = Panel::new(8000).unwrap();
    let before: Vec<_> = p
        .rows()
        .iter()
        .map(|r| (r.value, r.status.clone()))
        .collect();
    assert!(input::open_bytes("x.mp3", mp3_stub(), Channel::Index(0)).is_err());
    assert!(input::open_bytes("x.wav", wav_i16(22050, &[0; 10]), Channel::Index(0)).is_err());
    assert!(workbench_core::sidecar::read("nope").is_err());
    assert!(workbench_core::export::check_size(u64::MAX / 4).is_err());
    let after: Vec<_> = p
        .rows()
        .iter()
        .map(|r| (r.value, r.status.clone()))
        .collect();
    assert_eq!(before, after);
}
