//! Spec 001 T009 [TR: D7, P12]: the tail brings every input sample out, a longer tail changes
//! nothing before it, and the tail is 0 where the engine's latency is 0.

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
                    assert!(
                        floor > 0.0 && extra <= floor,
                        "{tag}: extra {extra:e} above the coder on silence {floor:e}"
                    );
                } else {
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

#[test]
fn tail_zero_at_front_taps() {
    let x = tone(48000, 0.1, 0.5);
    for tap in [
        "after_agc",
        "after_mic_capsule",
        "after_mic_amp",
        "after_insert",
    ] {
        let r = run(&request(48000, &x, |p| set_choice(p, "tap", tap)));
        assert_eq!(r.latency_samples, 0, "{tap}");
        assert_eq!(r.tail_samples, 0, "{tap}");
        assert_eq!(r.samples.len(), x.len(), "{tap}: the identity length");
    }
}
