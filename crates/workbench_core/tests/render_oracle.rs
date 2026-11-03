//! Spec 001 T008 [TR: FR-007, SC-001, US1 AS1]: every render equals the engine's direct output
//! for the same calls, in one whole-input call and at block size 1, at seven rates, on three
//! stimuli, in drop and mute mode.

mod support;

use support::*;

fn stimuli(rate: u32) -> Vec<(&'static str, Vec<f32>)> {
    vec![
        ("tone", tone(rate, 0.6, 0.5)),
        ("noise", noise(rate, 0.6, 0.3)),
        ("voiced", voiced(rate, 1.5, 0.5)),
    ]
}

#[test]
fn render_equals_the_oracles() {
    for rate in measurement_rates() {
        for (name, x) in stimuli(rate) {
            for mode in ["drop", "mute"] {
                let req = request(rate, &x, |p| set_choice(p, "vas.mode", mode));
                let r = run(&req);
                let fed = with_tail(&x, r.tail_samples);
                for block in [0usize, 1] {
                    let (o, ev) = oracle(req.settings, &req.engine_values, &fed, block);
                    assert_eq!(
                        &r.samples[..],
                        &o[..],
                        "{rate} Hz {name} {mode} block {block}"
                    );
                    assert_eq!(
                        r.events, ev,
                        "{rate} Hz {name} {mode} block {block}: reports"
                    );
                }
            }
        }
    }
}

/// All seven rates normally; the three conversion classes when `RR_WB_FAST` is set.
fn measurement_rates() -> Vec<u32> {
    if std::env::var_os("RR_WB_FAST").is_some() {
        vec![6000, 44100, 48000]
    } else {
        RATES.to_vec()
    }
}
