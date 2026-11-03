//! Spec 005b [TR: FR-001 – FR-005, SC-001, SC-002, US1, US2]: two renders compared.

mod support;

use std::path::PathBuf;

use rr_dr60::control_map::{self, ControlReading};
use rr_dr60::controls::ControlValue;
use support::*;
use workbench_core::ab::{self, AbMode};
use workbench_core::analysis::compare::{self, Align, Refused};
use workbench_core::monitor::Take;
use workbench_core::render::{LiveInfo, Render, Scheduled};
use workbench_core::session::{PresetLoadError, Session};

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

/// The chain-only front: the microphone stages, the gate and the coder bypassed, so the engine is
/// linear and silent on silence (no hiss, no idle coder output).
fn quiet(p: &mut workbench_core::panel::Panel) {
    set_switch(p, "mic.capsule_enabled", false);
    set_switch(p, "mic.amp_enabled", false);
    set_switch(p, "vas.enabled", false);
    set_switch(p, "speech_codec_enabled", false);
}

#[test]
fn null_identical() {
    for rate in [6000, 48_000] {
        let x = voiced(rate, 0.8, 0.5);
        let req = request(rate, &x, |_| {});
        let (a, b) = (run(&req), run(&req));
        let t = compare::null_test(&a, &b, Align::Latency).unwrap();
        assert!(t.raw_identical, "{rate}");
        assert_eq!(t.s, 0);
        let w = t.whole.unwrap();
        assert_eq!((w.differing, w.first), (0, None));
        assert_eq!(w.peak_dbfs, f64::NEG_INFINITY);
        assert!(
            t.prefix.is_none(),
            "the reports agree: the prefix is the overlap"
        );
        assert!(compare::param_diff(&a.request, &b.request).is_empty());
    }
}

#[test]
fn null_figures() {
    // An impulse of amplitude a at k on N samples, against zeros (SC-002's closed forms).
    let (n, k, a) = (1000u64, 377u64, 0.25f32);
    let mut x = vec![0.0f32; n as usize];
    x[k as usize] = a;
    let z = vec![0.0f32; n as usize];
    let f = compare::figures(&x, &z, 0, 0, n);
    assert_eq!(
        (f.samples, f.differing, f.first, f.peak_at),
        (n, 1, Some(k), k)
    );
    assert!((f.peak_dbfs - 20.0 * f64::from(a).log10()).abs() < 1e-12);
    let rms = f64::from(a) / (n as f64).sqrt();
    assert!((f.rms_db - 20.0 * rms.log10()).abs() < 1e-12);
    // A against nothing: the depth is 0 dB.
    assert!(f.depth_db.unwrap().abs() < 1e-12);
    // +1 against −1: |d| = 2, +6.0206 dBFS.
    let (p, m) = (vec![1.0f32; 4], vec![-1.0f32; 4]);
    let f = compare::figures(&p, &m, 0, 0, 4);
    assert!((f.peak_dbfs - 6.020_599_913_279_624).abs() < 1e-9);
    // A silent: no depth.
    assert_eq!(compare::figures(&z, &x, 0, 0, n).depth_db, None);
    // A shift and a span: B[i + s].
    let f = compare::figures(&x, &z[..], -1, 1, n);
    assert_eq!((f.samples, f.first), (n - 1, Some(k)));
    assert_eq!(compare::overlap(10, 7, 0), (0, 7));
    assert_eq!(compare::overlap(10, 7, -3), (3, 10));
    assert_eq!(compare::overlap(10, 7, 4), (0, 3));
    assert_eq!(compare::overlap(10, 7, 9), (0, 0));
}

/// A reading other than `r` of the same type (for the one-hot).
fn other(r: ControlReading, v: &ControlValue) -> ControlReading {
    match r {
        ControlReading::Switch(b) => ControlReading::Switch(!b),
        ControlReading::Integer(i) => ControlReading::Integer(i + 1),
        ControlReading::Continuous(c) => ControlReading::Continuous(c + 0.5),
        ControlReading::Choice(c) => match v {
            ControlValue::Choice { options, .. } => ControlReading::Choice(
                options
                    .iter()
                    .map(|o| o.value)
                    .find(|&o| o != c)
                    .unwrap_or(c + 1),
            ),
            _ => ControlReading::Choice(c + 1),
        },
    }
}

/// Coach review 8: a one-hot over every control; B differs from A in that control alone, and
/// the diff lists exactly that row.
#[test]
fn diff_lists_one() {
    let rate = 16_000;
    let x = tone(rate, 0.3, 0.1);
    let a = request(rate, &x, |_| {});
    for (i, e) in rr_dr60::controls::controls().iter().enumerate() {
        let i = i as u32;
        let mut b = a.clone();
        // A control the one-pass request does not carry (the player's live values) reads as
        // absent on A; B gains it.
        let r0 = compare::reading(&a, i);
        let r = r0.unwrap_or_else(|| workbench_core::panel::default_reading(e, rate));
        let o = other(r, &e.value);
        if control_map::set_settings_value(&mut b.settings, i, o).is_err() {
            match b.engine_values.iter_mut().find(|(j, _)| *j == i) {
                Some(slot) => slot.1 = o,
                None => b.engine_values.push((i, o)),
            }
        }
        let d = compare::param_diff(&a, &b);
        let ids: Vec<_> = d.controls.iter().map(|c| c.identifier).collect();
        assert_eq!(ids, [e.identifier], "{}", e.identifier);
        assert_eq!(d.controls[0].a, r0);
        assert_eq!(d.controls[0].b, Some(o));
        assert!(d.a_only.is_empty() && d.b_only.is_empty() && d.input.is_none());
    }
    // −0.0 equals 0.0 among the controls.
    let mut z = a.clone();
    z.settings.mic.hiss_offset_db = -0.0;
    assert!(compare::param_diff(&a, &z).is_empty());
}

#[test]
fn diff_changes_multiset() {
    let rate = 8000;
    let x = tone(rate, 0.3, 0.1);
    let mut a = request(rate, &x, |_| {});
    let mut b = a.clone();
    let ch = |pos: u64, v: f64, tag: u64| Scheduled {
        clock: rr_dr60::changes::Clock::Output,
        position: pos,
        control: idx("volume"),
        value: v,
        tag,
    };
    a.changes = vec![
        ch(10, 0.5, 1),
        ch(20, 0.6, 2),
        ch(20, 0.6, 2),
        ch(90, 0.1, 4),
    ];
    b.changes = vec![
        ch(90, 0.1, 4),
        ch(20, 0.6, 2),
        ch(30, 0.7, 3),
        ch(5, 0.2, 5),
    ];
    let d = compare::param_diff(&a, &b);
    assert_eq!(d.a_only, vec![ch(10, 0.5, 1), ch(20, 0.6, 2)]);
    assert_eq!(d.b_only, vec![ch(5, 0.2, 5), ch(30, 0.7, 3)]);
    // Change values compare by their bits: −0.0 is not 0.0 there.
    a.changes = vec![ch(1, -0.0, 1)];
    b.changes = vec![ch(1, 0.0, 1)];
    let d = compare::param_diff(&a, &b);
    assert_eq!((d.a_only.len(), d.b_only.len()), (1, 1));
    b.start = 7;
    b.tail = false;
    let d = compare::param_diff(&a, &b);
    assert_eq!(d.start, Some((0, 7)));
    assert_eq!(d.tail, Some((true, false)));
}

/// The ratio B ÷ A at bin `k` where both sides are within `window` dB of their own largest bin.
fn ratio_within(
    p: &workbench_core::analysis::spectrum::SpectrumPair,
    k: usize,
    window: f64,
) -> Option<f64> {
    let (a, b) = (p.original.db(k), p.render.db(k));
    let ok = |d: f64, max: f64| d.is_finite() && d >= max - window;
    (ok(a, p.original.max_db()) && ok(b, p.render.max_db())).then_some(b - a)
}

/// US1 AS4, SC-001: the earphone route at volume positions 1.0 and 0.5 on white noise at
/// −20 dBFS RMS, in both orders. Everything before stage 11 is the same (one seed, equal gate
/// reports), and stage 11's sections on the earphone route are position-independent and linear
/// with the loaded law's scale, taken at once by a fresh chain (engine spec 011): so B = g·A
/// sample by sample within the f32 narrowing, B ÷ A is the published gain law in every bin
/// within 60 dB of both maxima (0.001 dB; deeper bins reach the narrowing floor, reported), and
/// the null depth is 20·log10|1 − g| (hardware review Q1, Q4, Q5).
#[test]
fn spectral_difference_reads_gain() {
    let law = rr_dr60::output::earphone_gain_db;
    let mut worst = (0.0f64, 0.0f64, 0.0f64);
    for rate in RATES {
        // Uniform noise of RMS −20 dBFS: amplitude 0.1·√3.
        let x = noise(rate, 1.0, 0.173_205_08);
        let at = |v: f64| {
            run(&request(rate, &x, |p| {
                set_choice(p, "route", "earphone");
                set_num(p, "volume", v);
            }))
        };
        let (one, half) = (at(1.0), at(0.5));
        assert_eq!(
            one.events, half.events,
            "{rate}: the premise, equal gate reports"
        );
        for (a, b, va, vb) in [(&one, &half, 1.0f32, 0.5f32), (&half, &one, 0.5, 1.0)] {
            let g_db = f64::from(law(vb)) - f64::from(law(va));
            let g = 10f64.powf(g_db / 20.0);
            assert_eq!(a.samples.len(), b.samples.len(), "{rate}");
            for (i, (p, q)) in a.samples.iter().zip(b.samples.iter()).enumerate() {
                // The f32 narrowing of each side: half an ULP of B plus g times half an ULP of
                // A (2⁻²⁴ relative each, bounded by 2⁻²³), with an absolute floor near silence.
                let e = (f64::from(*q) - g * f64::from(*p)).abs();
                let bound = (f64::from(q.abs()) + g * f64::from(p.abs())) / f64::from(1u32 << 23);
                assert!(
                    e <= bound + 1e-12,
                    "{rate} sample {i}: {e}; a {p:e} b {q:e} g {g}"
                );
            }
            let pair = compare::spectral(a, b).unwrap();
            let mut drawn = 0;
            for k in 0..pair.original.power.len() {
                if let Some(r) = ratio_within(&pair, k, 60.0) {
                    drawn += 1;
                    let e = (r - g_db).abs();
                    worst.0 = worst.0.max(e);
                    assert!(e < 0.001, "{rate} bin {k}: {r} vs {g_db}");
                }
                if let Some(r) = pair.ratio_db(k) {
                    worst.2 = worst.2.max((r - g_db).abs());
                }
            }
            // At least the voice band's bins (300 – 2000 Hz at 5 Hz) are within 60 dB.
            assert!(drawn >= 340, "{rate}: {drawn} bins");
            let t = compare::null_test(a, b, Align::Latency).unwrap();
            let depth = t.whole.unwrap().depth_db.unwrap();
            let want = 20.0 * (1.0 - g).abs().log10();
            worst.1 = worst.1.max((depth - want).abs());
            assert!(
                (depth - want).abs() < 0.001,
                "{rate}: depth {depth} vs {want}"
            );
        }
    }
    println!(
        "spectral difference: worst bin error within 60 dB {:.2e} dB, in the 120 dB window {:.2e} dB (the narrowing floor); worst depth error {:.2e} dB",
        worst.0, worst.2, worst.1
    );
}

/// Hardware review Q2 (b): the codec on against off differs in every sample; latency-aligned,
/// s is the latency difference, and the figures are reported.
#[test]
fn null_codec_latency_reported() {
    for rate in [6000, 48_000] {
        let x = voiced(rate, 1.0, 0.5);
        let r = |on: bool| {
            run(&request(rate, &x, |p| {
                set_switch(p, "speech_codec_enabled", on);
            }))
        };
        let (a, b) = (r(false), r(true));
        let t = compare::null_test(&a, &b, Align::Latency).unwrap();
        // The coder's published delay, 180 device samples at the host rate (coach review 12).
        let delay = i64::from(rr_dr60::speech_codec::info().delay_samples) * i64::from(rate) / 6000;
        assert_eq!(t.s, delay, "{rate}");
        let w = t.whole.unwrap();
        println!(
            "{rate}: codec off vs on, s = {}, depth {:?} dB, {} of {} differ",
            t.s, w.depth_db, w.differing, w.samples
        );
    }
}

/// Coach review 1 (a): the start term on the engine. B is the same input rendered from S = 100 ms,
/// whose first 100 ms are zeros; on the quiet front the engine stays at rest on zeros, so
/// B equals A shifted by −1000 over the overlap.
#[test]
fn null_start_aligned() {
    for rate in [6000, 44_100, 48_000] {
        // A start of 100 ms is on the conversion grid at every rate (hardware review Q3: off the
        // grid the converter's phase differs, −81 dB at 44.1 kHz).
        let st = rate as usize / 10;
        let mut x = vec![0.0f32; st];
        x.extend(noise(rate, 0.5, 0.1));
        let a = run(&request(rate, &x, quiet));
        let mut req = request(rate, &x, quiet);
        req.start = st as u64;
        let b = run(&req);
        assert_eq!(compare::shift(&a, &b), -(st as i64));
        let t = compare::null_test(&a, &b, Align::Latency).unwrap();
        assert_eq!(t.s, -(st as i64));
        assert_eq!(t.starts_differ, Some((0, st as u64)));
        let w = t.whole.unwrap();
        assert!(w.samples > st as u64, "{rate}");
        assert_eq!(w.differing, 0, "{rate}: {w:?}");
        // Sample 0 compares other moments.
        let z = compare::null_test(&a, &b, Align::SampleZero).unwrap();
        assert!(z.whole.unwrap().differing > 0);
    }
}

/// Coach review 1 (b): the latency term on hand-built renders: B is A delayed by k with its
/// latency k larger.
#[test]
fn null_latency_aligned() {
    let rate = 8000;
    let a = run(&request(rate, &noise(rate, 0.4, 0.1), quiet));
    let k = 137usize;
    let mut b = a.clone();
    let mut v = vec![0.0f32; k];
    v.extend_from_slice(&a.samples);
    b.samples = v.into();
    b.latency_samples += k as u32;
    let t = compare::null_test(&a, &b, Align::Latency).unwrap();
    assert_eq!(t.s, k as i64);
    let w = t.whole.unwrap();
    assert_eq!((w.samples, w.differing), (a.samples.len() as u64, 0));
    assert!(!t.raw_identical);
}

#[test]
fn null_no_overlap() {
    let rate = 6000;
    let a = run(&request(rate, &noise(rate, 0.3, 0.1), quiet));
    let mut b = a.clone();
    b.latency_samples += a.samples.len() as u32 + 10;
    let t = compare::null_test(&a, &b, Align::Latency).unwrap();
    assert!(t.whole.is_none() && t.prefix.is_none());
    let mut e = a.clone();
    e.samples = Vec::new().into();
    let t = compare::null_test(&e, &a, Align::SampleZero).unwrap();
    assert!(t.whole.is_none());
    assert_eq!(compare::spectral(&e, &a).unwrap_err(), Refused::TooShort);
}

/// Coach review 5 and hardware review B4: B gains a splice where A has none (a −43 dBFS tone
/// after voice, gated under a −40 dBFS threshold only, then voice so the pause resumes). The
/// premise: the first j reports agree. The prefix ends at B's extra splice (before the coder's
/// delay with the coder on, a conservative end), nulls bit for bit, and the first real
/// difference is not before it.
#[test]
fn null_prefix_drop_mode() {
    for rate in [6000, 48_000] {
        for codec in [false, true] {
            // Voice after the tone, so B's pause resumes: a pause that never resumes reports no
            // splice in drop mode (engine spec 024's lesson).
            let mut x = voiced(rate, 0.3, 0.5);
            x.extend(tone(rate, 1.6, 0.00708));
            x.extend(voiced(rate, 0.3, 0.5));
            let r = |th: f64| {
                run(&request(rate, &x, |p| {
                    set_switch(p, "speech_codec_enabled", codec);
                    set_choice(p, "vas.mode", "drop");
                    set_num(p, "vas.threshold_dbfs", th);
                }))
            };
            let (a, b) = (r(-43.0), r(-40.0));
            let j = a.events.len();
            assert!(j < b.events.len(), "{rate} {codec}: B pauses");
            assert_eq!(&a.events[..], &b.events[..j], "{rate} {codec}: the premise");
            let t = compare::null_test(&a, &b, Align::Latency).unwrap();
            let p = t.prefix.unwrap();
            let w = t.whole.unwrap();
            let end = p.from + p.samples;
            assert!(end < w.from + w.samples, "{rate} {codec}");
            assert_eq!(p.differing, 0, "{rate} {codec}: the prefix nulls");
            assert!(w.differing > 0);
            assert!(w.first.unwrap() >= end, "{rate} {codec}: conservative");
            assert_ne!(a.samples.len(), b.samples.len());
        }
    }
}

/// Hardware review B1: a pause still open at the end reports no splice; the prefix ends at the
/// paused side's end and the label is set.
#[test]
fn null_end_paused() {
    for rate in [6000, 48_000] {
        let mut x = voiced(rate, 0.3, 0.5);
        x.extend(tone(rate, 1.6, 0.00708));
        let r = |th: f64| {
            run(&request(rate, &x, |p| {
                set_switch(p, "speech_codec_enabled", false);
                set_choice(p, "vas.mode", "drop");
                set_num(p, "vas.threshold_dbfs", th);
            }))
        };
        let (a, b) = (r(-43.0), r(-40.0));
        assert!(!a.paused_at_end && b.paused_at_end, "{rate}");
        let t = compare::null_test(&a, &b, Align::Latency).unwrap();
        assert_eq!(t.ends_paused, Some((false, true)));
        let p = t.prefix.expect("the end pause is a difference");
        assert_eq!(
            p.from + p.samples,
            (b.samples.len() as i64 - t.s) as u64,
            "{rate}"
        );
    }
}

/// Hardware review B2: the coder off against on, with equal gate decisions, reads "the reports
/// agree" (the reports map with the latency less the coder's delay).
#[test]
fn codec_reports_agree() {
    for rate in [6000, 48_000] {
        let mut x = voiced(rate, 0.5, 0.5);
        x.extend(vec![0.0f32; rate as usize * 2]);
        x.extend(voiced(rate, 0.5, 0.5));
        let r = |on: bool| {
            run(&request(rate, &x, |p| {
                set_switch(p, "speech_codec_enabled", on);
                set_choice(p, "vas.mode", "drop");
            }))
        };
        let (a, b) = (r(false), r(true));
        assert!(!a.events.is_empty());
        assert_eq!(b.codec_delay_samples, 180 * rate / 6000);
        assert_eq!(a.codec_delay_samples, 0);
        let t = compare::null_test(&a, &b, Align::Latency).unwrap();
        assert!(t.prefix.is_none(), "{rate}: the gates agree");
    }
}

#[test]
fn rates_differ() {
    let a = run(&request(8000, &tone(8000, 0.5, 0.1), |_| {}));
    let b = run(&request(16_000, &tone(16_000, 0.5, 0.1), |_| {}));
    assert_eq!(
        compare::null_test(&a, &b, Align::Latency).unwrap_err(),
        Refused::RatesDiffer(8000, 16_000)
    );
    assert_eq!(
        compare::spectral(&a, &b).unwrap_err(),
        Refused::RatesDiffer(8000, 16_000)
    );
    assert!(
        Refused::RatesDiffer(8000, 16_000)
            .to_string()
            .contains("one rate")
    );
    let d = compare::param_diff(&a.request, &b.request);
    assert!(d.controls.iter().any(|c| c.identifier == "host_rate_hz"));
}

/// Coach review 3, restated by hardware review B3: a live take's budget skips are labelled; the
/// take's samples are continuous engine output, so nothing is cut. An incomplete take is labelled.
#[test]
fn compare_live_skips() {
    let rate = 6000;
    let a = run(&request(rate, &noise(rate, 2.0, 0.1), quiet));
    let mut b = a.clone();
    b.live = Some(LiveInfo {
        skips: vec![(9000, 60), (5000, 60)],
        incomplete: true,
        ..LiveInfo::default()
    });
    let t = compare::null_test(&a, &b, Align::Latency).unwrap();
    assert_eq!(t.cut_at_skip, Some(5000));
    assert_eq!(t.whole.unwrap().samples, a.samples.len() as u64);
    assert!(t.incomplete);
    let pair = compare::spectral(&a, &b).unwrap();
    assert_eq!(pair.render.frames, pair.original.frames);
}

/// Hardware review B5: the two sides keep different runs (A drops the pause, B mutes it), so the
/// ratio is read from different frames of stationary noise: within the Welch variance of 0 dB in
/// the voice band.
#[test]
fn spectral_different_runs() {
    let rate = 8000;
    let mut x = noise(rate, 1.5, 0.1);
    x.extend(vec![0.0f32; rate as usize * 2]);
    x.extend(noise(rate, 1.5, 0.1));
    let r = |mode: &str| {
        run(&request(rate, &x, |p| {
            set_switch(p, "speech_codec_enabled", false);
            set_switch(p, "mic.capsule_enabled", false);
            set_choice(p, "vas.mode", mode);
        }))
    };
    let (a, b) = (r("drop"), r("mute"));
    assert!(!a.events.is_empty() && !b.events.is_empty());
    assert_ne!(compare::side_runs(&a), compare::side_runs(&b));
    let pair = compare::spectral(&a, &b).unwrap();
    let mut worst = 0.0f64;
    for k in 60..=400 {
        if let Some(d) = pair.ratio_db(k) {
            worst = worst.max(d.abs());
        }
    }
    println!("different runs: worst |B ÷ A| in 300 – 2000 Hz {worst:.3} dB");
    assert!(worst < 1.5, "{worst}");
}

/// Coach review 4: D5's rule at every ordered pair of takes, in both modes, with different
/// starts; closed forms on renders without splices.
#[test]
fn ab_three_takes() {
    let rate = 8000;
    let x = noise(rate, 1.0, 0.1);
    let a = run(&request(rate, &x, quiet));
    let mut req = request(rate, &x, quiet);
    req.start = 2000;
    let b = run(&req);
    let n = x.len() as u64;
    let (la, lb) = (u64::from(a.latency_samples), u64::from(b.latency_samples));
    let pos = 3000u64;
    let takes = [Take::Original, Take::Render, Take::B];
    for mode in [AbMode::EngineMap, AbMode::SameSeconds] {
        let input_of = |t: Take, p: u64| -> u64 {
            match (t, mode) {
                (Take::Original, _) => p,
                (Take::Render, AbMode::EngineMap) => p - la,
                (Take::B, AbMode::EngineMap) => 2000 + p - lb,
                (Take::Render, AbMode::SameSeconds) => p,
                (Take::B, AbMode::SameSeconds) => 2000 + p,
            }
        };
        let into = |t: Take, i: u64| -> u64 {
            match (t, mode) {
                (Take::Original, _) => i,
                (Take::Render, AbMode::EngineMap) => i + la,
                (Take::B, AbMode::EngineMap) => i - 2000 + lb,
                (Take::Render, AbMode::SameSeconds) => i,
                (Take::B, AbMode::SameSeconds) => i - 2000,
            }
        };
        for from in takes {
            for to in takes {
                let got = ab::switch(from, pos, to, Some(&a), Some(&b), n, mode);
                let want = into(to, input_of(from, pos));
                assert_eq!(got, want, "{mode:?} {from:?} → {to:?}");
            }
        }
        // Before B's start: B's start (the accepted asymmetry: the switch back is at B's start).
        assert_eq!(
            ab::switch(Take::Original, 100, Take::B, Some(&a), Some(&b), n, mode),
            into(Take::B, 2000)
        );
    }
    // Past a take's end (coach review 7): within the input, and at the target's end.
    for mode in [AbMode::EngineMap, AbMode::SameSeconds] {
        for from in [Take::Render, Take::B] {
            let len = if from == Take::Render {
                a.samples.len()
            } else {
                b.samples.len()
            } as u64;
            assert!(ab::switch(from, len + 500, Take::Original, Some(&a), Some(&b), n, mode) <= n);
        }
        assert_eq!(
            ab::render_at(n * 10, &b, mode),
            b.samples.len() as u64,
            "{mode:?}"
        );
    }
    // No B: position 0.
    assert_eq!(
        ab::switch(
            Take::Render,
            10,
            Take::B,
            Some(&a),
            None,
            n,
            AbMode::EngineMap
        ),
        0
    );
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rr-wb-c-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn wav_file(path: &std::path::Path, rate: u32, x: &[f32]) {
    std::fs::write(path, wav_i24(rate, &to_i24(x))).unwrap();
}

#[test]
fn b_follows_the_file() {
    let d = dir("b");
    let (p1, p2) = (d.join("one.wav"), d.join("two.wav"));
    wav_file(&p1, 8000, &voiced(8000, 0.5, 0.5));
    wav_file(&p2, 8000, &tone(8000, 0.5, 0.1));
    let mut s = Session::new();
    s.open(&p1, workbench_core::input::Channel::Index(0))
        .unwrap();
    assert!(!s.keep_as_b(), "no render yet");
    let r: Render = run(&s.request().unwrap());
    assert!(s.accept_render(r));
    assert!(s.keep_as_b());
    s.open(&p1, workbench_core::input::Channel::Index(0))
        .unwrap();
    assert!(s.b().is_some(), "the same file keeps B");
    s.open(&p2, workbench_core::input::Channel::Index(0))
        .unwrap();
    assert!(s.b().is_none(), "another file drops B");
}

#[test]
fn preset_needs_input_and_clears_range() {
    let mut s = Session::new();
    let text = workbench_core::preset::write(&workbench_core::panel::Panel::new(8000).unwrap());
    assert_eq!(s.load_preset(&text).unwrap_err(), PresetLoadError::NoInput);
    let d = dir("p");
    let p = d.join("one.wav");
    wav_file(&p, 8000, &voiced(8000, 0.5, 0.5));
    s.open(&p, workbench_core::input::Channel::Index(0))
        .unwrap();
    // A restored live range (spec 003): a v2 sidecar with `live yes`.
    let mut req = s.request().unwrap();
    req.start = 400;
    let r = run(&req);
    let side = workbench_core::sidecar::write(&r);
    s.restore(&side).unwrap();
    assert!(s.range().is_some());
    s.load_preset(&text).unwrap();
    assert!(s.range().is_none(), "a preset clears the restored range");
    assert!(matches!(
        s.load_preset("rr-dr60-workbench sidecar 2\n"),
        Err(PresetLoadError::Preset(
            workbench_core::preset::PresetError::NotAPreset
        ))
    ));
}
