//! Spec 003 [TR: SC-004]: the worst live callback stays below 25 % of its real time at 48 kHz,
//! coder on, drop mode at its budget. Release only:
//! `cargo test -p workbench_core --release --test live_speed -- --ignored --nocapture`.

mod support;

use std::time::Instant;

use support::*;
use workbench_core::live::{LiveCore, TakeBuilder};

#[test]
#[ignore = "release-only timing; quote from a solo run"]
fn worst_callback() {
    let rate = 48000u32;
    // Speech with long pauses, so drop mode runs at its budget.
    let b = voiced(rate, 0.5, 0.5);
    let gap = vec![0.0f32; 4 * rate as usize];
    let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
    for frames in [64usize, 512] {
        let req = request(rate, &x, |p| set_choice(p, "vas.mode", "drop"));
        let (mut core, mut q) = LiveCore::new(&req, &[]).unwrap();
        let mut take = TakeBuilder::default();
        let mut buf = vec![0.0f32; frames];
        let (mut worst, mut total, mut n) = (0.0f64, 0.0f64, 0u64);
        while !q.state.finished.load(std::sync::atomic::Ordering::Relaxed) {
            let t = Instant::now();
            core.fill(&mut buf);
            let dt = t.elapsed().as_secs_f64();
            worst = worst.max(dt);
            total += dt;
            n += 1;
            take.drain(&mut q);
        }
        let budget = frames as f64 / f64::from(rate);
        println!(
            "{frames} frames at 48 kHz: worst {:.1} µs ({:.1} % of {:.0} µs), mean {:.2} µs ({:.2} %), {n} callbacks",
            worst * 1e6,
            100.0 * worst / budget,
            budget * 1e6,
            total / n as f64 * 1e6,
            100.0 * total / n as f64 / budget
        );
        assert!(worst < 0.25 * budget, "SC-004");
    }
}
