//! Spec 003 [TR: FR-009, SC-002]: the live callback body allocates nothing. Its own binary with a
//! counting allocator; run it single-threaded (`-- --test-threads=1`).

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use support::*;
use workbench_core::live::{Command, LiveCore, TakeBuilder};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator and only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller's contract is forwarded unchanged.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        FREES.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller's contract is forwarded unchanged.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

#[test]
fn live_callback_allocates_nothing() {
    // Post-implementation review B8: commands in both clocks, a 70-entry list (the refusal path),
    // pauses for the budget's skips, at 6 kHz (the many-chunk callbacks) and at 48 kHz.
    let thr = rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == "vas.threshold_dbfs")
        .unwrap() as u32;
    let vol = rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == "volume")
        .unwrap() as u32;
    let mut total_a = 0;
    let mut total_f = 0;
    for rate in [6000u32, 48000] {
        let b = voiced(rate, 0.4, 0.5);
        let x: Vec<f32> = (0..20)
            .flat_map(|_| {
                b.iter()
                    .copied()
                    .chain(std::iter::repeat_n(0.0, 3 * rate as usize))
            })
            .collect();
        let req = request(rate, &x, |p| {
            set_choice(p, "insert.before_limiter", "band_filter")
        });
        let list: Vec<(rr_dr60::changes::Clock, u64, u32, f64)> = (0..70)
            .map(|i| {
                if i % 2 == 0 {
                    (
                        rr_dr60::changes::Clock::Output,
                        100,
                        vol,
                        f64::from(i) / 100.0,
                    )
                } else {
                    (rr_dr60::changes::Clock::Input, 100, thr, -40.0)
                }
            })
            .collect();
        let (mut core, mut q) = LiveCore::new(&req, &list).unwrap();
        let mut take = TakeBuilder::default();
        let mut buf = vec![0.0f32; 8192];
        let sizes = [1usize, 64, 512, 4096, 6000, 333];
        let mut landed = 0usize;
        for k in 0..100_000usize {
            if k % 50 == 0 {
                let c = if k % 100 == 0 {
                    Command {
                        control: vol,
                        value: (k % 10) as f64 / 10.0,
                    }
                } else {
                    Command {
                        control: thr,
                        value: -45.0,
                    }
                };
                let _ = q.commands.push(c);
            }
            let n = sizes[k % sizes.len()];
            let (a0, f0) = (ALLOCS.load(Ordering::SeqCst), FREES.load(Ordering::SeqCst));
            core.fill(&mut buf[..n]);
            total_a += ALLOCS.load(Ordering::SeqCst) - a0;
            total_f += FREES.load(Ordering::SeqCst) - f0;
            if k % 8 == 0 {
                take.drain(&mut q);
                landed += take.log.iter().filter(|e| e.tag >= 1).count();
                take.samples.clear();
                take.rows.clear();
                take.log.clear();
                take.events.clear();
            }
            if q.state.drained.load(Ordering::Relaxed) {
                break;
            }
        }
        let skips = q.state.skips.load(Ordering::Relaxed);
        println!("{rate} Hz: changes landed {landed}, skips {skips}");
        assert!(
            landed > 0 && skips > 0,
            "{rate}: the test exercises changes and skips"
        );
    }
    assert_eq!(
        (total_a, total_f),
        (0, 0),
        "the callback allocated or freed"
    );
}
