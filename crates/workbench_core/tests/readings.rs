//! Spec 001 T010 [TR: FR-009]: the readings, and the engine's gate rule (spec 008 D2:
//! `Settings::one_pass_vas_runs`) checked against its behavior.

mod support;

use support::*;

/// Every tap, from the engine's tap control (so a new tap is covered without a change here).
fn taps() -> Vec<&'static str> {
    let e = rr_dr60::controls::control("tap").expect("the tap control");
    let rr_dr60::controls::ControlValue::Choice { options, .. } = e.value else {
        panic!("the tap is a choice")
    };
    options.iter().map(|o| o.identifier).collect()
}

/// A burst then 3 s of silence: drop mode removes the silence exactly when the gate runs.
#[test]
fn gate_predicate_pinned() {
    let rate = 16000;
    let mut x = tone(rate, 0.3, 0.5);
    x.resize(x.len() + 3 * rate as usize, 0.0);
    let taps = taps();
    assert!(taps.len() >= 12);
    for tap in taps {
        for enabled in [true, false] {
            let req = request(rate, &x, |p| {
                if tap == "after_earphone" {
                    set_choice(p, "route", "earphone");
                }
                set_choice(p, "tap", tap);
                set_switch(p, "vas.enabled", enabled);
                set_choice(p, "vas.mode", "drop");
            });
            let r = run(&req);
            let shorter = r.not_emitted > 0;
            assert_eq!(
                shorter,
                req.settings.one_pass_vas_runs(),
                "{tap} enabled={enabled}: the engine's rule disagrees with its behavior"
            );
            assert_eq!(r.gate_reached, req.settings.one_pass_vas_runs());
        }
    }
}

/// Three 0.4 s voiced bursts separated by 3 s of silence.
fn bursts(rate: u32) -> Vec<f32> {
    let b = voiced(rate, 0.4, 0.5);
    let gap = vec![0.0f32; 3 * rate as usize];
    [b.clone(), gap.clone(), b.clone(), gap, b].concat()
}

#[test]
fn readings_on_known_stimuli() {
    for rate in [6000, 48000] {
        let x = bursts(rate); // three bursts with 3 s gaps (longer than the default hang)
        let drop = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
        assert!(drop.gate_reached && drop.drop_mode);
        assert!(drop.splices_or_regions >= 1, "{rate}: splices in drop mode");
        assert_eq!(drop.splices_or_regions as usize, drop.events.len());
        assert_eq!(
            drop.not_emitted,
            x.len() as u64 + drop.tail_samples - drop.samples.len() as u64
        );
        assert!(drop.not_emitted > 0);
        assert!(drop.latency_samples > 0 && drop.tail_samples > u64::from(drop.latency_samples));
        let peak = drop.samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert_eq!(drop.peak, peak);

        let mute = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "mute")));
        assert!(!mute.drop_mode);
        assert!(
            mute.splices_or_regions >= 1,
            "{rate}: muted regions in mute mode"
        );
        assert_eq!(mute.not_emitted, 0, "{rate}: mute mode emits every sample");

        let off = run(&request(rate, &x, |p| set_switch(p, "vas.enabled", false)));
        assert!(!off.gate_reached);
        assert_eq!(off.not_emitted, 0, "{rate}: the gate bypassed");
        assert_eq!(off.splices_or_regions, 0);

        let front = run(&request(rate, &x, |p| set_choice(p, "tap", "after_agc")));
        assert_eq!(front.not_emitted, 0, "{rate}: a front tap");
        assert!(!front.gate_reached);
    }
}

#[test]
fn paused_at_end_reads_the_last_call() {
    let rate = 8000;
    let mut x = tone(rate, 0.3, 0.5);
    x.resize(x.len() + 2 * rate as usize, 0.0);
    let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
    assert!(r.paused_at_end, "a final pause");
    let t = tone(rate, 0.5, 0.5);
    let mut req = request(rate, &t, |_| {});
    req.tail = false;
    assert!(!run(&req).paused_at_end, "ends on a tone");
}
