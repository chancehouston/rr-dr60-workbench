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
    let rate = 48000;
    let x = voiced(rate, 60.0, 0.5);
    let req = request(rate, &x, |p| {
        set_choice(p, "insert.before_limiter", "band_filter")
    });
    let (mut core, mut q) = LiveCore::new(&req, &[]).unwrap();
    let vol = rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == "volume")
        .unwrap() as u32;
    let mut b = TakeBuilder::default();
    let mut buf = vec![0.0f32; 8192];
    let sizes = [1usize, 64, 512, 4096, 5000, 333];
    let mut total_a = 0;
    let mut total_f = 0;
    for k in 0..100_000usize {
        if k % 50 == 0 {
            let _ = q.commands.push(Command {
                control: vol,
                value: (k % 10) as f64 / 10.0,
            });
        }
        let n = sizes[k % sizes.len()];
        let (a0, f0) = (ALLOCS.load(Ordering::SeqCst), FREES.load(Ordering::SeqCst));
        core.fill(&mut buf[..n]);
        total_a += ALLOCS.load(Ordering::SeqCst) - a0;
        total_f += FREES.load(Ordering::SeqCst) - f0;
        // The UI's side (allocations here are allowed and not counted).
        if k % 8 == 0 {
            b.drain(&mut q);
            b.samples.clear();
            b.rows.clear();
            b.log.clear();
            b.events.clear();
        }
        if q.state.finished.load(Ordering::Relaxed) {
            break;
        }
    }
    assert_eq!(
        (total_a, total_f),
        (0, 0),
        "the callback allocated or freed"
    );
}
