//! Spec 004 [TR: FR-001 – FR-006, SC-001, SC-002, US1, US2]: the device.

mod support;

use rr_dr60::changes::Clock;
use rr_dr60::control_map::ControlReading;
use rr_dr60::{Pipeline, Settings, VasEvent};
use support::*;
use workbench_core::device::{self, Device, RecordEnd};
use workbench_core::render::Scheduled;

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

fn settings(rate: u32, edit: impl FnOnce(&mut workbench_core::panel::Panel)) -> Settings {
    let mut p = workbench_core::panel::Panel::new(rate).unwrap();
    edit(&mut p);
    assert!(p.renderable());
    p.snapshot().0
}

/// A direct engine's stored form for `x`, recorded in calls of the given sizes.
fn direct_stored(s: Settings, x: &[f32], sizes: &[usize]) -> Vec<u8> {
    let mut p = Pipeline::new(s).unwrap();
    p.start_recording().unwrap();
    let mut ev = vec![VasEvent::new(0, 0); p.max_record_events(x.len().max(1)).max(1)];
    let mut pos = 0;
    let mut k = 0;
    while pos < x.len() {
        let n = sizes[k % sizes.len()].min(x.len() - pos);
        let _ = p.record(&x[pos..pos + n], &mut ev).unwrap();
        pos += n;
        k += 1;
    }
    let _ = p.stop_recording(&mut ev).unwrap();
    let mut out = vec![0u8; p.export_len(1).unwrap()];
    let n = p.export(1, &mut out).unwrap();
    out.truncate(n);
    out
}

#[test]
fn recording_partition_twin() {
    for rate in RATES {
        let x = voiced(rate, 1.5, 0.5);
        let s = settings(rate, |_| {});
        let mut d = Device::new(s).unwrap();
        let out = d.record(&x, 0, None, &[]).unwrap();
        assert!(
            matches!(out.end, RecordEnd::Kept { number: 1, .. }),
            "{rate}: {:?}",
            out.end
        );
        let ours = d.export(1).unwrap();
        assert_eq!(ours, direct_stored(s, &x, &[x.len()]), "{rate}: one call");
        assert_eq!(
            ours,
            direct_stored(s, &x, &[1, 997, 64, 4801]),
            "{rate}: irregular calls"
        );
        assert_eq!(d.items()[0].counter, Some(0));
    }
}

#[test]
fn memory_full_self_stops() {
    let rate = 8000;
    let s = settings(rate, |p| set_num(p, "memory.capacity_s", 2.0));
    let mut d = Device::new(s).unwrap();
    let x = voiced(rate, 5.0, 0.5);
    let out = d.record(&x, 0, None, &[]).unwrap();
    assert!(
        matches!(out.end, RecordEnd::SelfStopped { number: 1 }),
        "{:?}",
        out.end
    );
    assert!(out.fed < x.len() as u64, "stopped before the input's end");
    let e = d.record(&x, 0, None, &[]).unwrap_err();
    println!("full memory: {e}");
    assert!(e.to_string().len() > 5, "refused in the engine's words");
}

#[test]
fn record_refused_when_no_slots() {
    let rate = 6000;
    let s = settings(rate, |p| set_num(p, "memory.max_recordings", 1.0));
    let mut d = Device::new(s).unwrap();
    let x = tone(rate, 0.3, 0.5);
    d.record(&x, 0, None, &[]).unwrap();
    assert!(d.record(&x, 0, None, &[]).is_err(), "no slots left");
}

#[test]
fn erase_updates_memory() {
    let rate = 16000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    for secs in [0.3, 0.5, 0.7] {
        d.record(&tone(rate, secs, 0.5), 0, None, &[]).unwrap();
    }
    let before = d.items();
    assert_eq!(before.len(), 3);
    d.erase(2).unwrap();
    let after = d.items();
    assert_eq!(after.len(), 2);
    assert_eq!(after[1].digest, before[2].digest, "item 3 renumbered to 2");
    assert_eq!(d.status().count, 2);
    d.erase_all().unwrap();
    assert!(d.items().is_empty());
}

#[test]
fn export_import_roundtrip() {
    let rate = 48000;
    let s = settings(rate, |_| {});
    let mut d = Device::new(s).unwrap();
    d.record(&voiced(rate, 1.0, 0.5), 0, None, &[]).unwrap();
    let bytes = d.export(1).unwrap();
    let mut e = Device::new(s).unwrap();
    let n = e.import(&bytes).unwrap();
    assert_eq!(e.items()[0].digest, d.items()[0].digest);
    assert_eq!(e.items()[0].counter, None, "imported");
    let a = d.play_to_take(1, &[], &[]).unwrap();
    let b = e.play_to_take(n, &[], &[]).unwrap();
    assert_eq!(a.samples, b.samples, "the same playback");
    let other = settings(rate, |p| set_switch(p, "speech_codec_enabled", false));
    let mut f = Device::new(other).unwrap();
    assert!(f.import(&bytes).is_err(), "another encoding is refused");
    assert!(f.items().is_empty(), "the memory is unchanged");
}

#[test]
fn empty_recording_discarded() {
    let rate = 8000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    let out = d.record(&[], 0, None, &[]).unwrap();
    assert_eq!(out.end, RecordEnd::DiscardedEmpty);
    assert!(d.items().is_empty());
}

#[test]
fn playback_partition_twin() {
    // The device's 10 ms playback equals a direct engine's playback in one large call.
    let rate = 44100;
    let s = settings(rate, |_| {});
    let mut d = Device::new(s).unwrap();
    d.record(&voiced(rate, 1.0, 0.5), 0, None, &[]).unwrap();
    let bytes = d.export(1).unwrap();
    let take = d.play_to_take(1, &[], &[]).unwrap();
    let mut p = Pipeline::new(s).unwrap();
    p.import(&bytes).unwrap();
    p.start_playback(1).unwrap();
    let mut out = vec![0.0f32; take.samples.len() + 100_000];
    let mut all = Vec::new();
    loop {
        let info = p.play(&mut out).unwrap();
        all.extend_from_slice(&out[..info.produced]);
        if info.finished {
            break;
        }
    }
    assert_eq!(take.samples, all);
}

fn seeded_changes(rate: u32, seed: u64) -> Vec<Scheduled> {
    let mut x = seed;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let mut v = Vec::new();
    for k in 0..10u64 {
        let position = (k + 1) * u64::from(rate) / 10;
        let (control, value) = match next() % 5 {
            0 => (idx("playback_speed"), 1.0 + (next() % 5) as f64),
            1 => (idx("route"), (next() % 2) as f64),
            2 => (idx("volume"), (next() % 100) as f64 / 100.0),
            3 => (idx("output.speaker_enabled"), (next() % 2) as f64),
            _ => (idx("playback_speed_mode"), (next() % 2) as f64),
        };
        v.push(Scheduled {
            clock: Clock::Output,
            position,
            control,
            value,
            tag: k + 1,
        });
    }
    v
}

#[test]
fn playback_take_reproduces() {
    for rate in RATES {
        let s = settings(rate, |_| {});
        let mut d = Device::new(s).unwrap();
        d.record(&voiced(rate, 2.0, 0.5), 0, None, &[]).unwrap();
        let stored = d.export(1).unwrap();
        let values = vec![
            (idx("volume"), ControlReading::Continuous(0.3)),
            (idx("playback_speed"), ControlReading::Integer(4)),
        ];
        let changes = seeded_changes(rate, 0xFEED ^ u64::from(rate));
        let take = d.play_to_take(1, &values, &changes).unwrap();
        let landed = take.log.iter().filter(|e| e.tag >= 1).count();
        assert!(landed >= 3, "{rate}: changes landed ({landed})");
        use sha2::{Digest, Sha256};
        let sha: [u8; 32] = Sha256::digest(&stored).into();
        let text = device::playback_sidecar(&take, "item.rrdr60", &sha);
        let sc = device::read_playback(&text).unwrap();
        let re = device::replay(&sc, &stored).unwrap();
        println!(
            "{rate} Hz: {} samples, {landed} changes landed, {} frames",
            take.samples.len(),
            take.frames
        );
        assert!(
            take.samples
                .iter()
                .zip(&re.samples)
                .all(|(a, b)| a.to_bits() == b.to_bits())
                && take.samples.len() == re.samples.len(),
            "{rate}: the output"
        );
        assert_eq!(take.log, re.log, "{rate}: the log");
        assert_eq!(sc.produced, take.samples.len() as u64);
    }
}

#[test]
fn playback_start_values_recorded() {
    // A take after another take: its sidecar records the values in force at its own start.
    let rate = 16000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    d.record(&voiced(rate, 1.0, 0.5), 0, None, &[]).unwrap();
    let first = vec![Scheduled {
        clock: Clock::Output,
        position: 100,
        control: idx("volume"),
        value: 0.7,
        tag: 1,
    }];
    let _ = d.play_to_take(1, &[], &first).unwrap();
    let second = d.play_to_take(1, &[], &[]).unwrap();
    let vol = second
        .values
        .iter()
        .find(|(i, _)| *i == idx("volume"))
        .unwrap()
        .1;
    assert_eq!(
        vol,
        ControlReading::Continuous(0.7),
        "the landed volume carried into the next start"
    );
}
