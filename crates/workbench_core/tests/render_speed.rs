//! Spec 001 T033 [TR: SC-004]: one minute at 48 kHz renders in under 10 s with the defaults.
//! Release only: `cargo test -p workbench_core --release --test render_speed -- --ignored --nocapture`.

mod support;

use std::time::Instant;

use support::*;

#[test]
#[ignore = "release-only timing; quote from a solo run"]
fn one_minute_at_48k() {
    let rate = 48000;
    let x = voiced(rate, 60.0, 0.5);
    let req = request(rate, &x, |_| {});
    let t = Instant::now();
    let r = run(&req);
    let dt = t.elapsed();
    println!(
        "one minute at 48 kHz: {:.3} s ({} output samples)",
        dt.as_secs_f64(),
        r.samples.len()
    );
    assert!(dt.as_secs_f64() < 10.0, "SC-004: {dt:?}");
}
