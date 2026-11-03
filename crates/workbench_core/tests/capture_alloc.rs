//! Spec 007 [TR: FR-004, SC-002]: the input callback's body and the monitor's fill allocate
//! nothing. Its own binary with a counting allocator.

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use support::*;
use workbench_core::input::Channel;
use workbench_core::live::{Command, LiveCore};

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
fn capture_allocates_nothing() {
    let volume = rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == "volume")
        .unwrap() as u32;
    for rate in [6000, 48_000] {
        let req = request(rate, &[], |_| {});
        let (mut core, mut q) = LiveCore::new_capture(&req, &[], 2, Channel::Mix, true).unwrap();
        // Enough 4096-frame callbacks to pass any fixed reserve (review Q1).
        let mono = voiced(rate, 6.0, 0.5);
        let frames: Vec<f32> = mono.iter().flat_map(|v| [*v, -*v]).collect();
        let mut out = vec![0.0f32; 512];
        let (u, d) = (AtomicU64::new(0), AtomicU64::new(0));
        let mut ms = workbench_core::capture::MonitorState::default();
        let mut monitor = q.monitor.take().unwrap();
        let (a0, f0) = (ALLOCS.load(Ordering::SeqCst), FREES.load(Ordering::SeqCst));
        let mut pos = 0;
        for (k, n) in [1usize, 7, 441, 4096, 512, 64].iter().cycle().enumerate() {
            if pos >= frames.len() {
                break;
            }
            if k % 5 == 0 {
                let _ = q.commands.push(Command {
                    control: volume,
                    value: 0.2 + (k % 3) as f64 * 0.1,
                });
            }
            let m = (n * 2).min(frames.len() - pos);
            core.push(&frames[pos..pos + m]);
            pos += m;
            workbench_core::capture::monitor_fill(
                &mut monitor,
                &mut out,
                &mut ms,
                1024,
                4800,
                160,
                false,
                &u,
                &d,
            );
        }
        let (a1, f1) = (ALLOCS.load(Ordering::SeqCst), FREES.load(Ordering::SeqCst));
        assert_eq!(
            (a1 - a0, f1 - f0),
            (0, 0),
            "{rate}: the callbacks allocated"
        );
    }
}
