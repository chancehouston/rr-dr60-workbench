//! The spectrogram (spec 002 D7; research R-06): Hann frames of 40 ms (N = rate ÷ 25) or 10 ms
//! wideband (rate ÷ 100), a 10 ms hop, the spectrum's scale per frame bin (25 Hz bins, ENBW
//! 37.5 Hz; 100 Hz and 150 Hz wideband), stored as u8 colour indices in 0.5 dB steps: 0 at
//! −120 dB and below, 240 at 0 dB, 254 at +7 dB, 255 above +7.25 dB (saturated; the speaker
//! route reaches about +7 dB). Plan review B7.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rustfft::{FftPlanner, num_complex::Complex};

/// The index for "above 0 dB".
pub const SATURATED: u8 = 255;

/// A spectrogram image: `cols` columns of `bins` u8 indices, bin 0 first.
#[derive(Clone, Debug, PartialEq)]
pub struct Spectrogram {
    /// Columns.
    pub cols: u32,
    /// Bins per column (N/2 + 1).
    pub bins: u32,
    /// The indices, column-major.
    pub data: Vec<u8>,
    /// The frame size.
    pub frame_n: usize,
    /// The hop.
    pub hop: usize,
    /// The analysed range.
    pub range: (u64, u64),
}

/// The u8 index of a level in dB re full-scale sine (research R-06).
pub fn index_of(db: f64) -> u8 {
    if db.is_nan() || db <= -120.0 {
        0
    } else if db > 7.25 {
        SATURATED
    } else {
        ((db + 120.0) * 2.0).round().clamp(0.0, 254.0) as u8
    }
}

/// Computes the spectrogram of `x[range]` with at most `max_cols` columns (a column is the
/// maximum over its frames). `progress` counts frames done; `None` when cancelled.
pub fn compute(
    x: &[f32],
    rate: u32,
    wide: bool,
    range: (u64, u64),
    max_cols: usize,
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Option<Spectrogram> {
    let n = if wide {
        (rate / 100) as usize
    } else {
        (rate / 25) as usize
    };
    let hop = (rate / 100) as usize;
    let (a, b) = (range.0 as usize, (range.1 as usize).min(x.len()));
    let frames = if b >= a + n { (b - a - n) / hop + 1 } else { 0 };
    let bins = n / 2 + 1;
    let cols = frames.min(max_cols.max(1));
    let mut data = vec![0u8; cols * bins];
    if frames == 0 {
        return Some(Spectrogram {
            cols: 0,
            bins: bins as u32,
            data: Vec::new(),
            frame_n: n,
            hop,
            range,
        });
    }
    let w: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())
        .collect();
    let sum: f64 = w.iter().sum();
    let scale = 4.0 / (sum * sum);
    let fft = FftPlanner::new().plan_fft_forward(n);
    let mut buf = vec![Complex::new(0.0, 0.0); n];
    for f in 0..frames {
        if f % 64 == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
        let s = a + f * hop;
        for (i, v) in buf.iter_mut().enumerate() {
            *v = Complex::new(f64::from(x[s + i]) * w[i], 0.0);
        }
        fft.process(&mut buf);
        let col = f * cols / frames;
        for k in 0..bins {
            let idx = index_of(10.0 * (buf[k].norm_sqr() * scale).log10());
            let cell = &mut data[col * bins + k];
            *cell = (*cell).max(idx);
        }
        progress.store(f as u64 + 1, Ordering::Relaxed);
    }
    Some(Spectrogram {
        cols: cols as u32,
        bins: bins as u32,
        data,
        frame_n: n,
        hop,
        range,
    })
}
