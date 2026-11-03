//! Spec 002 [TR: SC-004]: release-only timings. `cargo test -p workbench_core --release --test
//! speed_002 -- --ignored --nocapture`.

mod support;

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;

use support::*;
use workbench_core::analysis::{spectrogram, spectrum};

#[test]
#[ignore = "release-only timing; quote from a solo run"]
fn one_minute_figures() {
    let rate = 48000;
    let x = voiced(rate, 60.0, 0.5);
    let req = request(rate, &x, |_| {});
    let t = Instant::now();
    let r = run(&req);
    let render = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let _ = spectrum::pair(&x, &r, (0, x.len() as u64)).unwrap();
    let spec = t.elapsed().as_secs_f64();
    let (p, c) = (AtomicU64::new(0), AtomicBool::new(false));
    let t = Instant::now();
    let _ = spectrogram::compute(&x, rate, false, (0, x.len() as u64), 4096, &p, &c).unwrap();
    let sg = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let _ = workbench_core::taps::browse(&req, &p, &c);
    let taps = t.elapsed().as_secs_f64();
    println!(
        "one minute at 48 kHz: render with the timeline {render:.3} s ({} rows), spectrum pair {spec:.3} s, spectrogram {sg:.3} s, tap browser {taps:.3} s",
        r.meters.len()
    );
    assert!(render < 10.0 && spec < 1.0 && sg < 5.0 && taps < 60.0);
}
