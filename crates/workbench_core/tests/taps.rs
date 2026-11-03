//! Spec 002 [TR: FR-008, SC-003, US3 AS1]: the tap browser.

mod support;

use std::sync::atomic::{AtomicBool, AtomicU64};

use support::*;
use workbench_core::taps;

#[test]
fn tap_browser_digest_equals_render() {
    for rate in RATES {
        let x = voiced(rate, 0.8, 0.5);
        let req = request(rate, &x, |_| {});
        let t = taps::browse(&req, &AtomicU64::new(0), &AtomicBool::new(false));
        let e = rr_dr60::controls::control("tap").unwrap();
        let rr_dr60::controls::ControlValue::Choice { options, .. } = e.value else {
            panic!()
        };
        assert_eq!(t.rows.len(), options.len(), "{rate}: a row per tap");
        let mut refused = 0;
        for row in &t.rows {
            match &row.outcome {
                Ok(r) => {
                    let d = run(&request(rate, &x, |p| set_choice(p, "tap", row.tap)));
                    assert_eq!(r.sha256, d.output_sha256, "{rate} {}: digest", row.tap);
                    assert_eq!(r.latency, d.latency_samples);
                    assert_eq!(r.length, d.samples.len() as u64);
                }
                Err(why) => {
                    refused += 1;
                    assert_eq!(
                        row.tap, "after_earphone",
                        "{rate}: only the other route's tap is refused"
                    );
                    assert!(why.contains("route"), "the engine's words: {why}");
                }
            }
        }
        assert_eq!(
            refused, 1,
            "{rate}: exactly the earphone tap, on the speaker route"
        );
    }
}

#[test]
fn tap_browser_cancel() {
    let rate = 8000;
    let x = voiced(rate, 0.5, 0.5);
    let req = request(rate, &x, |_| {});
    let t = taps::browse(&req, &AtomicU64::new(0), &AtomicBool::new(true));
    assert!(
        t.rows.is_empty(),
        "a cancel before the first tap leaves no rows"
    );
}
