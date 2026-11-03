//! Spec 001 [TR: FR-008, US1 AS5] (coach 4): progress at least every half second, a cancel within
//! one second, and a cancelled render returning nothing (the session keeps its previous render).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use support::*;

#[test]
fn progress_and_cancel() {
    let rate = 48000;
    // Long enough to run for seconds in the test build.
    let x = voiced(rate, 600.0, 0.5);
    let req = request(rate, &x, |_| {});
    let progress = Arc::new(AtomicU64::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (p, c) = (progress.clone(), cancel.clone());
    let h = std::thread::spawn(move || workbench_core::render::render(&req, &p, &c));
    let mut last = 0;
    for _ in 0..3 {
        std::thread::sleep(Duration::from_millis(500));
        let now = progress.load(Ordering::Relaxed);
        assert!(
            now > last,
            "progress moved within half a second ({last} → {now})"
        );
        last = now;
    }
    assert!(last < x.len() as u64, "still rendering when cancelled");
    let t = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    let result = h.join().unwrap().unwrap();
    assert!(
        t.elapsed() < Duration::from_secs(1),
        "cancelled within a second: {:?}",
        t.elapsed()
    );
    assert!(result.is_none(), "a cancelled render returns nothing");
}
