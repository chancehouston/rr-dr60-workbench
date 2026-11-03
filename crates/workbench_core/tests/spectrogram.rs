//! Spec 002 [TR: FR-007]: the spectrogram's columns and scale.

use std::sync::atomic::{AtomicBool, AtomicU64};

use workbench_core::analysis::spectrogram::{self, SATURATED};

#[test]
fn spectrogram_columns() {
    let rate = 48000;
    let x: Vec<f32> = (0..rate * 4)
        .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin())
        .collect();
    let (p, c) = (AtomicU64::new(0), AtomicBool::new(false));
    let s = spectrogram::compute(&x, rate, false, (0, x.len() as u64), 128, &p, &c).unwrap();
    assert_eq!(s.cols, 128, "reduced to the column limit");
    assert_eq!(s.frame_n, (rate / 25) as usize);
    assert_eq!(s.bins as usize, s.frame_n / 2 + 1);
    assert_eq!(s.data.len(), (s.cols * s.bins) as usize);
    // A full-scale 1 kHz sine at 25 Hz bins sits on bin 40: about 0 dB (index near 240).
    let bin = 40usize;
    let v = s.data[10 * s.bins as usize + bin];
    assert!((236..=241).contains(&v), "the tone's index {v}");
    assert_eq!(spectrogram::index_of(-130.0), 0);
    assert_eq!(spectrogram::index_of(0.0), 240);
    assert_eq!(spectrogram::index_of(3.0), 246, "headroom to +7 dB");
    assert_eq!(spectrogram::index_of(7.0), 254);
    assert_eq!(spectrogram::index_of(7.5), SATURATED);
    let wide = spectrogram::compute(&x, rate, true, (0, x.len() as u64), 4096, &p, &c).unwrap();
    assert_eq!(wide.frame_n, (rate / 100) as usize);
    let cancelled = spectrogram::compute(
        &x,
        rate,
        false,
        (0, x.len() as u64),
        4096,
        &p,
        &AtomicBool::new(true),
    );
    assert!(cancelled.is_none());
}
