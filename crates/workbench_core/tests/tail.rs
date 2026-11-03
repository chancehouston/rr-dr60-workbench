//! Spec 001 T009 [TR: D7, P12]: the tail brings every input sample out and a longer tail changes
//! nothing before it; since spec 008 D3 the tail is the engine's one-pass drain.

mod support;

use support::*;

#[test]
fn tail_emits_the_input() {
    // The test that does not rest on T itself: hang 50 ms, coder on, drop mode, an input that
    // ends on a tone; every input sample must be emitted (produced ≥ input + latency).
    for rate in RATES {
        let x = tone(rate, 0.5, 0.5);
        let req = request(rate, &x, |p| {
            set_num(p, "vas.hang_ms", 50.0);
            set_switch(p, "speech_codec_enabled", true);
            set_choice(p, "vas.mode", "drop");
        });
        let r = run(&req);
        assert!(r.tail_samples > 0, "{rate}");
        assert!(
            r.samples.len() as u64 >= x.len() as u64 + u64::from(r.latency_samples),
            "{rate} Hz: produced {} < input {} + latency {}",
            r.samples.len(),
            x.len(),
            r.latency_samples
        );
    }
}

#[test]
fn tail_is_sufficient() {
    // A further second of tail leaves the render's prefix bit-identical. What follows it is below
    // −100 dBFS with the coder off; with the coder on it is no louder than the coder's own output
    // on digital silence (its idle output is quiet, not zero: a dated restatement of research
    // R-07's bound, records.md). The hiss is off.
    for rate in RATES {
        for codec in [false, true] {
            for mode in ["drop", "mute"] {
                let x = tone(rate, 0.5, 0.5);
                let edit = |p: &mut workbench_core::panel::Panel| {
                    set_switch(p, "mic.capsule_enabled", false);
                    set_switch(p, "speech_codec_enabled", codec);
                    set_choice(p, "vas.mode", mode);
                };
                let r = run(&request(rate, &x, edit));
                let longer = with_tail(&x, r.tail_samples + u64::from(rate));
                let mut req2 = request(rate, &longer, edit);
                req2.tail = false;
                let r2 = run(&req2);
                let tag = format!("{rate} Hz codec {codec} {mode}");
                assert!(r2.samples.len() >= r.samples.len(), "{tag}");
                assert_eq!(
                    &r2.samples[..r.samples.len()],
                    &r.samples[..],
                    "{tag}: prefix"
                );
                let extra = r2.samples[r.samples.len()..]
                    .iter()
                    .fold(0.0f32, |m, v| m.max(v.abs()));
                if codec {
                    let mut silent = request(rate, &vec![0.0; x.len()], edit);
                    silent.tail = false;
                    let floor = run(&silent)
                        .samples
                        .iter()
                        .fold(0.0f32, |m, v| m.max(v.abs()));
                    let margin = 20.0 * (f64::from(floor) / f64::from(extra.max(1e-30))).log10();
                    println!(
                        "{tag}: after the tail {extra:e}, the coder on silence {floor:e}, margin {margin:.1} dB"
                    );
                    assert!(
                        floor > 0.0 && extra <= floor,
                        "{tag}: extra {extra:e} above the coder on silence {floor:e}"
                    );
                } else {
                    println!("{tag}: after the tail {extra:e}");
                    assert!(
                        extra < 1e-5,
                        "{tag}: extra output peaks at {extra:e} (−100 dBFS is 1e-5)"
                    );
                }
            }
        }
    }
}

#[test]
fn tail_at_the_defaults() {
    // The default engine (hiss and coder on): the prefix identity, and the level after the tail
    // reported beside the default engine's own output on digital silence.
    for rate in RATES {
        let x = tone(rate, 0.5, 0.5);
        let r = run(&request(rate, &x, |_| {}));
        let longer = with_tail(&x, r.tail_samples + u64::from(rate));
        let mut req2 = request(rate, &longer, |_| {});
        req2.tail = false;
        let r2 = run(&req2);
        assert_eq!(
            &r2.samples[..r.samples.len()],
            &r.samples[..],
            "{rate}: prefix"
        );
        let extra = r2.samples[r.samples.len()..]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        let mut silent = request(rate, &vec![0.0; x.len()], |_| {});
        silent.tail = false;
        let floor = run(&silent)
            .samples
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()));
        println!(
            "{rate} Hz defaults: after the tail {extra:e}, the default engine on silence {floor:e}"
        );
    }
}

#[test]
fn reported_high_q_filter_ring() {
    // Reported, not asserted (research R-07): the energy after T with a high-Q band filter.
    let rate = 48000;
    let x = tone(rate, 0.5, 0.5);
    let edit = |p: &mut workbench_core::panel::Panel| {
        set_switch(p, "mic.capsule_enabled", false);
        set_choice(p, "vas.mode", "mute");
        set_choice(p, "insert.before_limiter", "band_filter");
        set_choice(p, "band_filter.type", "band_pass");
        set_num(p, "band_filter.frequency_hz", 1000.0);
        let q = match p.row("band_filter.q").unwrap().entry.value {
            rr_dr60::controls::ControlValue::Continuous { max, .. } => max,
            _ => unreachable!(),
        };
        set_num(p, "band_filter.q", f64::from(q));
    };
    let r = run(&request(rate, &x, edit));
    let longer = with_tail(&x, r.tail_samples + u64::from(rate));
    let mut req2 = request(rate, &longer, edit);
    req2.tail = false;
    let r2 = run(&req2);
    let extra = r2.samples[r.samples.len()..]
        .iter()
        .fold(0.0f32, |m, v| m.max(v.abs()));
    let db = if extra > 0.0 {
        20.0 * f64::from(extra).log10()
    } else {
        f64::NEG_INFINITY
    };
    println!("high-Q band filter: peak after the tail {extra:e} ({db:.1} dBFS)");
}

/// Spec 008 D3: the tail is the engine's one-pass drain at every tap. At the front taps it is
/// the settle of the stages that run: 0 at the capsule's tap, stage 2's (498 at 48 kHz) after
/// it, where the old rule ("0 where the latency is 0") emitted no tail although stage 2 and the
/// slot still ring.
#[test]
fn tail_is_the_engines_drain() {
    let x = tone(48000, 0.1, 0.5);
    for tap in [
        "after_mic_capsule",
        "after_mic_amp",
        "after_insert",
        "after_agc",
        "after_record",
        "route_output",
    ] {
        let req = request(48000, &x, |p| set_choice(p, "tap", tap));
        let r = run(&req);
        let p = workbench_core::render::fresh_engine(&req).unwrap();
        assert_eq!(
            r.tail_samples,
            u64::from(p.one_pass_drain_samples()),
            "{tap}"
        );
        if matches!(
            tap,
            "after_mic_capsule" | "after_mic_amp" | "after_insert" | "after_agc"
        ) {
            assert_eq!(r.latency_samples, 0, "{tap}");
            assert_eq!(
                r.samples.len() as u64,
                x.len() as u64 + r.tail_samples,
                "{tap}: no gate, every sample out"
            );
        }
    }
    let at = |tap: &str| run(&request(48000, &x, |p| set_choice(p, "tap", tap))).tail_samples;
    assert_eq!(at("after_mic_capsule"), 0);
    assert_eq!(
        at("after_mic_amp"),
        498,
        "stage 2's settle (the engine's figure)"
    );
}

/// The review of 008, W6 (reported, not asserted): in drop mode, an input that ends inside the
/// hang (a tone, then 45 ms of silence, against the 50 ms minimum hang) lets the gate pause
/// before the kept samples are out; it holds them until a resume that never comes, so the tail
/// does not bring them out (the engine's documented caveat; the hang's minimum, 50 ms, already
/// outlasts the default latency, so "a hang shorter than the latency" cannot be configured).
#[test]
fn tail_with_a_short_hang_is_reported() {
    for rate in [6000, 48_000] {
        let mut x = tone(rate, 0.5, 0.5);
        x.resize(x.len() + rate as usize * 45 / 1000, 0.0);
        let req = request(rate, &x, |p| {
            set_num(p, "vas.hang_ms", 50.0);
            set_switch(p, "speech_codec_enabled", true);
            set_choice(p, "vas.mode", "drop");
        });
        let r = run(&req);
        let want = x.len() as u64 + u64::from(r.latency_samples);
        println!(
            "{rate} Hz, input ending 45 ms into a 50 ms hang (latency {} samples): produced {} of {want}, held {}",
            r.latency_samples,
            r.samples.len(),
            want.saturating_sub(r.samples.len() as u64)
        );
    }
}
