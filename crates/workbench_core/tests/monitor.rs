//! Spec 001 T012 [TR: FR-011]: the callback body's rules.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use workbench_core::monitor::{Shared, Take, Takes, fill};

fn takes() -> Takes {
    Takes {
        original: Arc::from((0..10).map(|i| i as f32).collect::<Vec<_>>()),
        render: Arc::from((0..6).map(|i| -(i as f32)).collect::<Vec<_>>()),
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
