//! Spec 002 [TR: FR-006, SC-002, US2 AS1 – AS4]: the spectrum's method and calibration.

mod support;

use support::*;
use workbench_core::analysis::spectrum::{self, n_for};

fn sine(rate: u32, f: f64, amp: f64, secs: f64) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    (0..n)
        .map(|i| (amp * (2.0 * std::f64::consts::PI * f * i as f64 / f64::from(rate)).sin()) as f32)
        .collect()
}

#[test]
fn spectrum_calibration_bin_200() {
    for rate in RATES {
        let n = n_for(rate);
        assert_eq!(n * 5, rate as usize);
        let x = sine(rate, 1000.0, 1.0, 1.0);
        let s = spectrum::welch(&x, rate, &[(0, x.len() as u64)]).unwrap();
        assert_eq!(s.freq(200), 1000.0);
        let db = s.db(200);
        println!(
            "{rate} Hz: N {n}, frames {}, 1 kHz full-scale sine at bin 200 {db:+.4} dB",
            s.frames
        );
        assert!(db.abs() <= 0.05, "{rate}: {db}");
    }
}

/// Gaussian noise from a seeded generator (Box–Muller over xorshift).
fn gaussian(n: usize, sigma: f64) -> Vec<f32> {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        ((x >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    };
    (0..n)
        .map(|_| {
            let (u, v) = (next(), next());
            (sigma * (-2.0 * u.ln()).sqrt() * (2.0 * std::f64::consts::PI * v).cos()) as f32
        })
        .collect()
}

#[test]
fn spectrum_noise_density() {
    let rate = 48000;
    let sigma = 0.1f64;
    let x = gaussian(30 * rate as usize, sigma);
    let s = spectrum::welch(&x, rate, &[(0, x.len() as u64)]).unwrap();
    let nyq = s.power.len() - 1;
    let (a, b) = (nyq / 10, nyq * 9 / 10);
    let mean: f64 = s.power[a..b].iter().sum::<f64>() / (b - a) as f64;
    let got = 10.0 * mean.log10();
    let want = 10.0 * (4.0 * sigma * sigma * spectrum::ENBW_BINS / s.n as f64).log10();
    let var: f64 = x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len() as f64;
    let want_realized = 10.0 * (4.0 * var * spectrum::ENBW_BINS / s.n as f64).log10();
    println!("noise: realized variance {var:.6} → {want_realized:.4} dB");
    assert!(
        (got - want_realized).abs() <= 0.05,
        "{got} vs the realized {want_realized}"
    );
    println!("noise σ {sigma}: mean {got:.4} dB, expected {want:.4} dB");
    assert!((got - want).abs() <= 0.05, "{got} vs {want}");
}

#[test]
fn matches_naive_dft() {
    let rate = 6000;
    let x = gaussian(n_for(rate), 0.3);
    let s = spectrum::welch(&x, rate, &[(0, x.len() as u64)]).unwrap();
    let n = x.len();
    let w: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())
        .collect();
    let sum: f64 = w.iter().sum();
    for k in [0usize, 1, 37, 200, 599, 600] {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for i in 0..n {
            let a = -2.0 * std::f64::consts::PI * (k * i) as f64 / n as f64;
            re += f64::from(x[i]) * w[i] * a.cos();
            im += f64::from(x[i]) * w[i] * a.sin();
        }
        let p = (re * re + im * im) * 4.0 / (sum * sum);
        let d = (10.0 * p.log10() - s.db(k)).abs();
        assert!(d < 1e-6, "bin {k}: {d} dB from the naive DFT");
    }
}

#[test]
fn spectrum_identity_ratio_exact() {
    // The oracle (plan review, coach 3): a synthetic render that is the input delayed by L = 237
    // samples reads exactly 0 dB in every drawn bin, through the same map and frames.
    let rate = 48000;
    let x = gaussian(5 * rate as usize, 0.1);
    let mut r = run(&request(rate, &x[..rate as usize], |p| {
        set_choice(p, "vas.mode", "mute")
    }));
    let l = 237u32;
    let mut delayed = vec![0.0f32; l as usize];
    delayed.extend_from_slice(&x);
    r.samples = delayed.into();
    r.latency_samples = l;
    r.codec_delay_samples = 0; // the synthetic render has no coder
    r.events.clear();
    r.drop_mode = false;
    let p = spectrum::pair(&x, &r, (0, x.len() as u64)).unwrap();
    let mut drawn = 0;
    for k in 0..p.original.power.len() {
        if let Some(d) = p.ratio_db(k) {
            assert!(d.abs() < 1e-9, "bin {k}: {d}");
            drawn += 1;
        }
    }
    assert!(drawn > 4000);
}

#[test]
fn spectrum_identity_ratio() {
    // The identity candidate (research R-04): every stage that changes the band or the level
    // bypassed, at the record filter's tap; only the conversion to 6 kHz and back remains.
    let rate = 48000;
    let x = gaussian(10 * rate as usize, 0.1);
    let r = run(&request(rate, &x, |p| {
        set_switch(p, "mic.capsule_enabled", false);
        set_switch(p, "mic.amp_enabled", false);
        set_switch(p, "agc.enabled", false);
        set_switch(p, "record_stage_enabled", false);
        set_switch(p, "adc_clamp_enabled", false);
        set_choice(p, "tap", "after_record");
    }));
    assert!(r.latency_samples > 0, "a nonzero alignment");
    let pair = spectrum::pair(&x, &r, (0, x.len() as u64)).unwrap();
    let mut worst = 0.0f64;
    for k in 0..pair.original.power.len() {
        let f = pair.original.freq(k);
        if (100.0..=2400.0).contains(&f)
            && let Some(d) = pair.ratio_db(k)
        {
            worst = worst.max(d.abs());
        }
    }
    println!(
        "engine identity at 48 kHz, 100 – 2400 Hz: worst |{worst:.4}| dB (latency {}; reported, the conversion's round trip)",
        r.latency_samples
    );
    assert!(worst <= 0.1, "{worst}");
}

#[test]
fn spectrum_ratio_arithmetic() {
    let rate = 16000;
    let b = voiced(rate, 0.6, 0.5);
    let gap = vec![0.0f32; 3 * rate as usize];
    let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
    let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
    assert!(!r.events.is_empty());
    let range = (0, x.len() as u64);
    let p = spectrum::pair(&x, &r, range).unwrap();
    assert!(
        p.dropped_frames > 0,
        "frames that hold a splice are dropped"
    );
    // Independently: no render run holds a splice position or comes within the guard of one.
    let runs = spectrum::render_runs(p.range_out, &r);
    let guard = u64::from(r.playback_latency_samples);
    // A splice reaches the output at its report position + the coder's delay (spec 008 D6).
    let c = u64::from(r.codec_delay_samples);
    for e in &r.events {
        let at = e.output_position + c;
        for &(a, b) in &runs {
            assert!(at + guard <= a || at >= b + guard, "a run near a splice");
        }
    }
    for k in 0..p.original.power.len() {
        if let Some(d) = p.ratio_db(k) {
            assert_eq!(d, p.render.db(k) - p.original.db(k));
        }
    }
    assert_eq!(
        p.range_out.0,
        workbench_core::ab::to_render(range.0, &r, workbench_core::ab::AbMode::EngineMap)
    );
    // Kept spans only: no original frame inside a removed pause.
    let kept = spectrum::kept_spans(range, &r);
    for m in workbench_core::ab::marks(&r) {
        for &(a, b) in &kept {
            assert!(
                b <= m.input_span.0 || a >= m.input_span.1,
                "a kept span overlaps a removed pause"
            );
        }
    }
    let e = spectrum::welch(&x, rate, &[(0, 100)]).unwrap_err();
    assert!(
        e.to_string().contains("200 ms"),
        "the refusal names the minimum: {e}"
    );
    // Silence: the 120 dB window draws no ratio bins (range_too_short / ratio_window_silence).
    let silent = run(&request(rate, &vec![0.0; rate as usize], |p| {
        set_switch(p, "mic.capsule_enabled", false)
    }));
    if let Ok(q) = spectrum::pair(&vec![0.0; rate as usize], &silent, (0, rate as u64)) {
        assert!(
            (0..q.original.power.len()).all(|k| q.ratio_db(k).is_none()),
            "no ratio over silence"
        );
    }
    // Mute mode: muted regions are excluded on both sides.
    let rm = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "mute")));
    let kept = spectrum::kept_spans(range, &rm);
    for m in workbench_core::ab::marks(&rm) {
        for &(a, b) in &kept {
            assert!(
                b <= m.input_span.0 || a >= m.input_span.1,
                "a kept span overlaps a muted region"
            );
        }
    }
}
