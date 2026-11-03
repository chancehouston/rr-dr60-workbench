//! Spec 004b [TR: FR-003, SC-002]: the live playback callback allocates nothing. Its own binary
//! with a counting allocator; run it single-threaded (`-- --test-threads=1`).

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use rr_dr60::changes::Clock;
use support::*;
use workbench_core::device::Device;
use workbench_core::live::Command;
use workbench_core::live_play::PlayTakeBuilder;
use workbench_core::render::Scheduled;

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

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

#[test]
fn live_play_allocates_nothing() {
    // Commands (one refused), a list, speed and mode changes, the drain, at 6 kHz and 48 kHz;
    // the counts cover the callbacks only (the UI's drains are outside the window).
    let mut total = (0, 0);
    for rate in [6000u32, 48000] {
        let mut p = workbench_core::panel::Panel::new(rate).unwrap();
        set_switch(&mut p, "output.earphone_enabled", false);
        let mut d = Device::new(p.snapshot().0).unwrap();
        d.record(&voiced(rate, 2.0, 0.5), 0, None, &[]).unwrap();
        let r = u64::from(rate);
        let list: Vec<Scheduled> = (0..20)
            .map(|k| Scheduled {
                clock: Clock::Output,
                position: k * r / 10,
                control: idx(if k % 2 == 0 {
                    "playback_speed"
                } else {
                    "playback_speed_mode"
                }),
                value: if k % 2 == 0 {
                    1.0 + (k % 5) as f64
                } else {
                    (k % 4 / 2) as f64
                },
                tag: k + 1,
            })
            .collect();
        let (mut player, mut q) = d.live_player(1, &[], &list).unwrap();
        let mut b = PlayTakeBuilder::default();
        b.samples.reserve(10 * rate as usize);
        let mut buf = vec![0.0f32; 8192];
        let sizes = [512usize, 64, 5000, 128, 4096];
        let mut k = 0;
        while !q.state.drained.load(Ordering::Relaxed) {
            if k == 2 {
                // A burst past the 64 pending changes: the refusal path (B6).
                for j in 0..70 {
                    q.commands
                        .push(Command {
                            control: idx("volume"),
                            value: 0.01 * j as f64,
                        })
                        .unwrap();
                }
            }
            if k % 3 == 0 {
                q.commands
                    .push(Command {
                        control: idx("volume"),
                        value: 0.1 * (k % 9) as f64,
                    })
                    .unwrap();
                q.commands
                    .push(Command {
                        control: idx("route"),
                        value: (k % 2) as f64,
                    })
                    .unwrap();
                q.commands
                    .push(Command {
                        control: idx("output.earphone_enabled"),
                        value: 1.0,
                    })
                    .unwrap();
            }
            let (a0, f0) = (
                ALLOCS.load(Ordering::Relaxed),
                FREES.load(Ordering::Relaxed),
            );
            player.fill(&mut buf[..sizes[k % sizes.len()]]);
            total.0 += ALLOCS.load(Ordering::Relaxed) - a0;
            total.1 += FREES.load(Ordering::Relaxed) - f0;
            b.drain(&mut q);
            let _ = b.take_replies(&mut q);
            k += 1;
        }
        assert!(
            b.replies.iter().any(|r| r.error.is_some()),
            "the refusal path ran"
        );
        println!("{rate} Hz: {k} callbacks");
    }
    assert_eq!(total, (0, 0), "allocations and frees in the callbacks");
}
