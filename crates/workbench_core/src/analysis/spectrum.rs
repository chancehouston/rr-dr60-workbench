//! Welch spectra and their ratio (spec 002 D6; research R-02).
//!
//! N = rate ÷ 5, so the bins are 5 Hz at every rate and 1 kHz is bin 200. A periodic Hann
//! window, 50 % overlap, power averaged. Scale: P_k = |X_k|²·4/(Σw)², so a sine of amplitude A at a
//! bin centre reads A² (20·log10 A dB), and white noise of variance σ² reads 4σ²·ENBW/N per bin
//! (ENBW 1.5 bins = 7.5 Hz). Hann's scalloping loss reaches 1.42 dB between bins: tone levels are
//! not read off this view.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner, num_complex::Complex};

use crate::render::Render;

/// The bin spacing in Hz.
pub const BIN_HZ: f64 = 5.0;
/// The Hann window's equivalent noise bandwidth in bins.
pub const ENBW_BINS: f64 = 1.5;
/// The window for drawing a ratio bin: both sides within this many dB of their own largest bin.
pub const RATIO_WINDOW_DB: f64 = 120.0;

/// The FFT size for a rate: rate ÷ 5.
pub const fn n_for(rate: u32) -> usize {
    (rate / 5) as usize
}

/// The shortest range a spectrum takes: one frame.
pub fn min_samples(rate: u32) -> usize {
    n_for(rate)
}

/// A range shorter than one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooShort {
    /// The samples needed.
    pub need: usize,
}

impl std::fmt::Display for TooShort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Select at least 200 ms ({} samples) for a spectrum",
            self.need
        )
    }
}

/// A power spectrum of one take over one range.
#[derive(Clone, Debug, PartialEq)]
pub struct Spectrum {
    /// The rate.
    pub rate: u32,
    /// The FFT size.
    pub n: usize,
    /// The frames averaged.
    pub frames: u32,
    /// The power per bin, linear, sine-calibrated (bins 0 ..= n/2).
    pub power: Vec<f64>,
}

impl Spectrum {
    /// A bin's frequency.
    pub fn freq(&self, k: usize) -> f64 {
        k as f64 * f64::from(self.rate) / self.n as f64
    }

    /// A bin's level in dB re full-scale sine per bin (−∞ for zero power).
    pub fn db(&self, k: usize) -> f64 {
        10.0 * self.power[k].log10()
    }

    /// The largest bin's dB.
    pub fn max_db(&self) -> f64 {
        self.power.iter().cloned().fold(0.0f64, f64::max).log10() * 10.0
    }
}

/// The periodic Hann window and its sum.
fn hann(n: usize) -> (Vec<f64>, f64) {
    let w: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())
        .collect();
    let sum = w.iter().sum();
    (w, sum)
}

/// The frame starts of `spans` (each a `[start, end)` range): every frame lies wholly inside one
/// span, at a hop of N/2.
fn frame_starts(spans: &[(u64, u64)], n: usize) -> Vec<usize> {
    let hop = (n / 2).max(1);
    let mut v = Vec::new();
    for &(a, b) in spans {
        let (a, b) = (a as usize, b as usize);
        let mut s = a;
        while s + n <= b {
            v.push(s);
            s += hop;
        }
    }
    v
}

/// The Welch spectrum of `x` over `spans` at `rate` (D6).
///
/// # Errors
///
/// [`TooShort`] when no span holds one frame.
pub fn welch(x: &[f32], rate: u32, spans: &[(u64, u64)]) -> Result<Spectrum, TooShort> {
    let n = n_for(rate);
    let starts = frame_starts(spans, n);
    if starts.is_empty() {
        return Err(TooShort { need: n });
    }
    let (w, sum) = hann(n);
    let fft: Arc<dyn Fft<f64>> = FftPlanner::new().plan_fft_forward(n);
    let mut buf = vec![Complex::new(0.0, 0.0); n];
    let mut power = vec![0.0f64; n / 2 + 1];
    for &s in &starts {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = Complex::new(f64::from(x[s + i]) * w[i], 0.0);
        }
        fft.process(&mut buf);
        for (k, p) in power.iter_mut().enumerate() {
            *p += buf[k].norm_sqr();
        }
    }
    let scale = 4.0 / (sum * sum) / starts.len() as f64;
    for p in &mut power {
        *p *= scale;
    }
    Ok(Spectrum {
        rate,
        n,
        frames: starts.len() as u32,
        power,
    })
}

/// The original's and the render's spectra over a selected original range, and their ratio
/// (D6): the original's frames from the spans the render kept, the render's range the map of
/// the selection, and render frames that contain a splice dropped.
#[derive(Clone, Debug, PartialEq)]
pub struct SpectrumPair {
    /// The original's spectrum.
    pub original: Spectrum,
    /// The render's spectrum.
    pub render: Spectrum,
    /// The selected original range.
    pub range_in: (u64, u64),
    /// Its map in the render.
    pub range_out: (u64, u64),
    /// Render frames dropped for containing a splice.
    pub dropped_frames: u32,
}

impl SpectrumPair {
    /// The ratio render ÷ original at bin `k` in dB, where both sides are inside the 120 dB
    /// window of their own largest bin.
    pub fn ratio_db(&self, k: usize) -> Option<f64> {
        let (a, b) = (self.original.db(k), self.render.db(k));
        let ok = |d: f64, max: f64| d.is_finite() && d >= max - RATIO_WINDOW_DB;
        (ok(a, self.original.max_db()) && ok(b, self.render.max_db())).then_some(b - a)
    }
}

/// The render's runs inside `range_out` that hold neither a splice (drop mode), a muted region
/// (mute mode) nor an output before the latency (which maps before input 0): research R-02 and
/// the plan review's B3. Each run is shrunk by the engine's playback-side latency at an edge that
/// is a splice or a region boundary, so no frame carries the reconstruction's smear across it
/// (post-implementation review B3).
pub fn render_runs(range_out: (u64, u64), r: &Render) -> Vec<(u64, u64)> {
    let first = crate::ab::to_render(0, r, crate::ab::AbMode::EngineMap);
    let mut cuts: Vec<(u64, u64)> = crate::ab::marks(r)
        .iter()
        .map(|m| {
            if m.removed {
                (m.output_position, m.output_position)
            } else {
                (
                    m.output_position,
                    m.output_position + (m.input_span.1 - m.input_span.0),
                )
            }
        })
        .collect();
    cuts.sort_unstable();
    let guard = u64::from(r.playback_latency_samples);
    let mut runs = Vec::new();
    let mut a = range_out.0.max(first);
    let mut a_edge = false;
    for (s, e) in cuts {
        if e < a || s >= range_out.1 {
            continue;
        }
        if s > a {
            let lo = if a_edge { a + guard } else { a };
            let hi = s.min(range_out.1).saturating_sub(guard);
            if hi > lo {
                runs.push((lo, hi));
            }
        }
        a = a.max(e);
        a_edge = true;
    }
    if a < range_out.1 {
        let lo = if a_edge { a + guard } else { a };
        if range_out.1 > lo {
            runs.push((lo, range_out.1));
        }
    }
    runs
}

/// The original's spans that the render's `runs` came from, mapped by the engine's map
/// (`timeline::onepass_output_to_input` on the gate reports' clock, `render::output_to_input`).
fn original_spans(runs: &[(u64, u64)], r: &Render, input_len: u64) -> Vec<(u64, u64)> {
    runs.iter()
        .filter(|(a, b)| b > a)
        .map(|&(a, b)| {
            let (i0, _) = crate::render::output_to_input(a, r);
            let (i1, _) = crate::render::output_to_input(b - 1, r);
            (i0.min(input_len), (i1 + 1).min(input_len))
        })
        .collect()
}

/// The kept input spans of the original `range` (the inputs the render emitted, mapped through
/// the engine's map; removed pauses, muted regions and the outputs before input 0 excluded).
pub fn kept_spans(range: (u64, u64), r: &Render) -> Vec<(u64, u64)> {
    let o0 = crate::ab::to_render(range.0, r, crate::ab::AbMode::EngineMap);
    let o1 = crate::ab::to_render(range.1, r, crate::ab::AbMode::EngineMap).max(o0);
    original_spans(&render_runs((o0, o1), r), r, range.1)
        .into_iter()
        .map(|(a, b)| (a.max(range.0), b.min(range.1)))
        .filter(|(a, b)| b > a)
        .collect()
}

/// The render-side spectrum over the whole render, its runs only (the tap browser's figure).
///
/// # Errors
///
/// [`TooShort`] when no run holds a frame.
pub fn render_side(r: &Render) -> Result<Spectrum, TooShort> {
    let runs = render_runs((0, r.samples.len() as u64), r);
    welch(&r.samples, r.request.rate_hz, &runs)
}

/// The pair for an original range (D6).
///
/// # Errors
///
/// [`TooShort`] when either side holds no frame.
pub fn pair(input: &[f32], r: &Render, range: (u64, u64)) -> Result<SpectrumPair, TooShort> {
    let rate = r.request.rate_hz;
    let range = (
        range.0.min(input.len() as u64),
        range.1.min(input.len() as u64),
    );
    let o0 = crate::ab::to_render(range.0, r, crate::ab::AbMode::EngineMap);
    let o1 = crate::ab::to_render(range.1, r, crate::ab::AbMode::EngineMap).max(o0);
    let runs = render_runs((o0, o1), r);
    let original = welch(input, rate, &kept_spans(range, r))?;
    let whole = frame_starts(&[(o0, o1)], n_for(rate)).len() as u32;
    let render = welch(&r.samples, rate, &runs)?;
    Ok(SpectrumPair {
        dropped_frames: whole.saturating_sub(render.frames),
        original,
        render,
        range_in: range,
        range_out: (o0, o1),
    })
}
