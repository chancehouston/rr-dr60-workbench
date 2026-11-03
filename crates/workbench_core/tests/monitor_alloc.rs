//! Spec 001 T013 [TR: FR-011]: `fill` allocates and frees nothing. Its own test binary, with a
//! counting global allocator; run it single-threaded (`-- --test-threads=1`) so no other test's
//! allocations land in the window.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use workbench_core::monitor::{Shared, Take, Takes, fill};

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
fn fill_allocates_nothing() {
    let t = Takes {
        original: Arc::from(vec![0.25f32; 48_000]),
        render: Arc::from(vec![-0.25f32; 40_000]),
        rate_hz: 48_000,
    };
    let s = Shared::default();
    s.playing.store(true, Ordering::Relaxed);
    let (mut l, mut r) = (vec![0.0f32; 512], vec![0.0f32; 512]);
    let (a0, f0) = (ALLOCS.load(Ordering::SeqCst), FREES.load(Ordering::SeqCst));
    for k in 0..100_000u64 {
        if k % 97 == 0 {
            s.request(
                if k % 2 == 0 {
                    Take::Render
                } else {
                    Take::Original
                },
                k % 30_000,
            );
            s.set_volume((k % 10) as f32 / 10.0);
        }
        fill(&t, &s, &mut l, &mut r);
    }
    let (a1, f1) = (ALLOCS.load(Ordering::SeqCst), FREES.load(Ordering::SeqCst));
    assert_eq!((a1 - a0, f1 - f0), (0, 0), "fill allocated or freed");
}
