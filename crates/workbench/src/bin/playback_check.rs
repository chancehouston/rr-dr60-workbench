//! A silent check of the CoreAudio path (spec 001 R-04, FR-011): plays a take at 6 and 48 kHz
//! with the monitor volume at 0 for half a second each and reports how far the callback moved
//! and the overload count. `cargo run -p rr_dr60_workbench --bin playback_check`.

#[path = "../playback.rs"]
mod playback;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use workbench_core::monitor::{Shared, Take, Takes};

fn main() {
    for rate in [6000u32, 48000] {
        let take: Arc<[f32]> = Arc::from(vec![0.1f32; rate as usize * 2]);
        let shared = Arc::new(Shared::default());
        shared.set_volume(0.0);
        let takes = Takes {
            original: take.clone(),
            render: take,
            rate_hz: rate,
        };
        let p = playback::Playback::start(takes, shared.clone()).expect("start");
        shared.request(Take::Original, 0);
        shared.playing.store(true, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let (_, pos) = shared.cursor();
        println!(
            "{rate} Hz: {pos} samples in 0.5 s (expected about {}), device rate {:?}, overloads {}, device lost {}",
            rate / 2,
            p.device_rate(),
            shared.overloads.load(Ordering::Relaxed),
            p.device_lost()
        );
        drop(p);
    }
}
