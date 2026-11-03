//! Spec 001 T012 [TR: FR-011]: the callback body's rules.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use workbench_core::monitor::{NO_REQUEST, Shared, Take, Takes, fill, pack, unpack};

fn takes() -> Takes {
    Takes {
        original: Arc::from((0..10).map(|i| i as f32).collect::<Vec<_>>()),
        render: Arc::from((0..6).map(|i| -(i as f32)).collect::<Vec<_>>()),
        b: Arc::from((0..4).map(|i| 100.0 + i as f32).collect::<Vec<_>>()),
        rate_hz: 6000,
    }
}

#[test]
fn fill_rules() {
    let t = takes();
    let s = Shared::default();
    let (mut l, mut r) = ([9.0f32; 4], [9.0f32; 4]);
    // Stopped: zeros, cursor held.
    fill(&t, &s, &mut l, &mut r);
    assert_eq!(l, [0.0; 4]);
    assert_eq!(s.cursor(), (Take::Original, 0));
    // Playing at volume 0.5: identical channels, take × volume.
    s.playing.store(true, Ordering::Relaxed);
    s.set_volume(0.5);
    fill(&t, &s, &mut l, &mut r);
    assert_eq!(l, [0.0, 0.5, 1.0, 1.5]);
    assert_eq!(l, r);
    assert_eq!(s.cursor(), (Take::Original, 4));
    // A request swaps in at the top of the buffer and switches the take.
    s.request(Take::Render, 3);
    s.set_volume(1.0);
    fill(&t, &s, &mut l, &mut r);
    assert_eq!(l, [-3.0, -4.0, -5.0, 0.0], "zeros past the end");
    assert_eq!(s.cursor(), (Take::Render, 6), "the cursor stops at the end");
    assert_eq!(
        s.request.load(Ordering::Relaxed),
        workbench_core::monitor::NO_REQUEST
    );
}

#[test]
fn a_mapped_request_adds_what_played_since() {
    // Hardware review B2: the UI read the original at 2 and mapped it to render 1; a buffer of
    // 2 played before the swap, so the render starts at 1 + 2 = 3, neither repeating nor skipping.
    let t = takes();
    let s = Shared::default();
    s.playing.store(true, Ordering::Relaxed);
    s.request(Take::Original, 2);
    let (mut l, mut r) = ([0.0f32; 2], [0.0f32; 2]);
    fill(&t, &s, &mut l, &mut r); // the jump's swap, then a buffer of 2
    assert_eq!(s.cursor(), (Take::Original, 4));
    s.request_from(Take::Render, 1, 2);
    fill(&t, &s, &mut l, &mut r);
    assert_eq!(l, [-3.0, -4.0]);
    assert_eq!(s.cursor(), (Take::Render, 5));
}

/// Spec 005b D5: the third take packs and plays.
#[test]
fn third_take_packs_and_plays() {
    for t in [Take::Original, Take::Render, Take::B] {
        for p in [0u64, 1, 12_345, (1 << 62) - 1] {
            assert_eq!(unpack(pack(t, p)), (t, p));
        }
    }
    assert_ne!(pack(Take::B, (1 << 62) - 1), NO_REQUEST);
    let t = takes();
    let sh = Shared::default();
    sh.request(Take::B, 1);
    sh.playing.store(true, std::sync::atomic::Ordering::Relaxed);
    let (mut l, mut r) = (vec![0.0; 5], vec![0.0; 5]);
    fill(&t, &sh, &mut l, &mut r);
    assert_eq!(l, [101.0, 102.0, 103.0, 0.0, 0.0]);
    assert_eq!(sh.cursor(), (Take::B, 4));
}
