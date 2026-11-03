//! Spec 001 T011 [TR: FR-010, SC-005]: the A/B map and its inverse.

mod support;

use rr_dr60::VasEvent;
use support::*;
use workbench_core::ab::{self, AbMode};

#[test]
fn hand_built_exact_values() {
    // Latency 10; a splice at output 50 that removed 30 input samples.
    let ev = [VasEvent::new(50, 30)];
    let f = |i| ab::to_render_raw(i, 10, &ev, true, 200);
    assert_eq!(f(0), 10, "input 0 maps to the latency");
    assert_eq!(f(39), 49);
    // Inputs 40..=69 were removed: they land on the splice (output 50).
    assert_eq!(f(40), 50);
    assert_eq!(f(69), 50);
    assert_eq!(f(70), 50);
    assert_eq!(f(71), 51);
    // Past the end clamps to the render's end.
    assert_eq!(f(10_000), 200);
    // Mute mode: o − L.
    assert_eq!(ab::to_render_raw(5, 10, &ev, false, 200), 15);
}

#[test]
fn round_trips_at_every_rate() {
    for rate in RATES {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
        for mode in ["drop", "mute"] {
            let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", mode)));
            let n_in = x.len() as u64;
            let n_out = r.samples.len() as u64;
            for i in (0..n_in).step_by(97) {
                let o = ab::to_render(i, &r, AbMode::EngineMap);
                assert!(o <= n_out);
                if o < n_out {
                    assert!(
                        ab::to_original(o, &r, AbMode::EngineMap, n_in) >= i,
                        "{rate} {mode} i={i}"
                    );
                }
            }
            for o in (0..n_out).step_by(97) {
                // Outputs before the latency align with "before input 0" (the engine clamps them).
                let (_, clamped) = rr_dr60::timeline::onepass_output_to_input(
                    o,
                    r.latency_samples,
                    &r.events,
                    r.drop_mode,
                );
                if clamped {
                    continue;
                }
                let i = ab::to_original(o, &r, AbMode::EngineMap, n_in);
                assert!(
                    ab::to_render(i, &r, AbMode::EngineMap) <= o,
                    "{rate} {mode} o={o}"
                );
            }
            // Same seconds: the identity, clamped.
            assert_eq!(ab::to_render(123, &r, AbMode::SameSeconds), 123);
            assert_eq!(
                ab::to_render(n_in + 5, &r, AbMode::SameSeconds),
                n_out.min(n_in + 5)
            );
            assert_eq!(
                ab::to_original(n_out + 5, &r, AbMode::SameSeconds, n_in),
                n_in.min(n_out + 5)
            );
        }
    }
}

#[test]
fn final_pause_clamps_to_the_end() {
    let rate = 8000;
    let mut x = tone(rate, 0.3, 0.5);
    x.resize(x.len() + 2 * rate as usize, 0.0);
    let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
    assert!(r.paused_at_end);
    let last = x.len() as u64 - 1;
    assert_eq!(
        ab::to_render(last, &r, AbMode::EngineMap),
        r.samples.len() as u64
    );
}
