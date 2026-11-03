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
            render: take.clone(),
            b: take,
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
    live_check();
    live_play_check();
}

/// The live playback path (spec 004b): a recorded tone played on a clone of the device in the
/// output callback, with the monitor volume at 0 (nothing audible), for half a second; the
/// player's produced count must advance.
fn live_play_check() {
    let rate = 48000u32;
    let mut panel = workbench_core::panel::Panel::new(rate).unwrap();
    let i = panel.index_of("mic.capsule_enabled").unwrap();
    panel.set(i, rr_dr60::control_map::ControlReading::Switch(false));
    let mut d = workbench_core::device::Device::new(panel.snapshot().0).unwrap();
    let tone: Vec<f32> = (0..rate as usize * 2)
        .map(|k| 0.3 * (std::f32::consts::TAU * 1000.0 * k as f32 / rate as f32).sin())
        .collect();
    d.record(&tone, 0, None, &[]).unwrap();
    let (mut player, q) = d.live_player(1, &[], &[]).unwrap();
    let shared = Arc::new(Shared::default());
    shared.set_volume(0.0);
    let p = playback::Playback::start_live(rate, shared, move |o| player.fill(o))
        .expect("live playback start");
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!(
        "live playback at {rate} Hz: the player produced {} samples in 0.5 s (about {}), device lost {}",
        q.state.produced.load(Ordering::Relaxed),
        rate / 2,
        p.device_lost()
    );
}

/// The live path (spec 003): a fresh engine on silence (the capsule off, so nothing is audible)
/// in the output callback for half a second; the engine's produced count must advance.
fn live_check() {
    use std::sync::atomic::Ordering;
    let rate = 48000u32;
    let mut panel = workbench_core::panel::Panel::new(rate).unwrap();
    let i = panel.index_of("mic.capsule_enabled").unwrap();
    panel.set(i, rr_dr60::control_map::ControlReading::Switch(false));
    let samples: Arc<[f32]> = Arc::from(vec![0.0f32; rate as usize * 2]);
    let (settings, engine_values) = panel.snapshot();
    let sha = workbench_core::input::samples_sha256(&samples);
    let req = workbench_core::render::RenderRequest {
        input: samples,
        input_sha256: sha,
        file_name: "silence".into(),
        frames: u64::from(rate) * 2,
        raw_sha256: sha,
        rate_hz: rate,
        channel: workbench_core::input::Channel::Index(0),
        settings,
        engine_values,
        tail: false,
        block: workbench_core::render::block_for(rate),
        start: 0,
        len: None,
        changes: Vec::new(),
    };
    let (mut core, q) = workbench_core::live::LiveCore::new(&req, &[]).unwrap();
    let p =
        playback::Playback::start_live(rate, Arc::new(Shared::default()), move |o| core.fill(o))
            .expect("live start");
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!(
        "live at {rate} Hz: the engine produced {} samples in 0.5 s (about {}), overloads 0 expected, device lost {}",
        q.state.produced.load(Ordering::Relaxed),
        rate / 2,
        p.device_lost()
    );
}
