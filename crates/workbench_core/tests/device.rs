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
        matches!(out.end, RecordEnd::SelfStopped { number: 1, length } if length > 0 && Some(length) == d.items().first().map(|i| i.length)),
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
        assert_eq!(landed, 10, "{rate}: every change landed");
        // The changes matter: a take without them differs.
        let plain = d.play_to_take(1, &values, &[]).unwrap();
        assert_ne!(
            plain.samples, take.samples,
            "{rate}: the changes change the output"
        );
        // An independent oracle (B3): a direct engine that imports the stored form, sets the
        // values, schedules every change before the start, and plays in 4096-sample calls.
        let mut p = Pipeline::new(s).unwrap();
        p.import(&stored).unwrap();
        for &(i, v) in &values {
            p.set_control_value(i, v).unwrap();
        }
        for c in &changes {
            p.schedule_change(
                rr_dr60::Stream::Playback,
                c.position,
                c.control,
                c.value,
                c.tag,
            )
            .unwrap();
        }
        p.start_playback(1).unwrap();
        let mut buf = vec![0.0f32; 4096];
        let mut cb =
            vec![rr_dr60::changes::ChangeEntry::default(); rr_dr60::changes::MAX_CHANGES_PER_CALL];
        let (mut direct, mut dlog) = (Vec::new(), Vec::new());
        loop {
            let info = p.play_with_changes(&mut buf, &mut cb).unwrap();
            direct.extend_from_slice(&buf[..info.produced]);
            dlog.extend_from_slice(&cb[..info.changes]);
            if info.finished {
                break;
            }
        }
        assert!(
            direct
                .iter()
                .zip(&take.samples)
                .all(|(a, b)| a.to_bits() == b.to_bits())
                && direct.len() == take.samples.len(),
            "{rate}: the direct engine"
        );
        assert_eq!(dlog, take.log, "{rate}: the direct engine's log");
        // The replay through the sidecar and the stored form, with its checks.
        use sha2::{Digest, Sha256};
        let sha: [u8; 32] = Sha256::digest(&stored).into();
        let text = device::playback_sidecar(&take, "item.rrdr60", &sha);
        let sc = device::read_playback(&text).unwrap();
        let rep = device::replay(&sc, &stored).unwrap();
        println!(
            "{rate} Hz: {} samples, {landed} changes landed, {} frames",
            take.samples.len(),
            take.frames
        );
        assert!(
            rep.reproduces(),
            "{rate}: {:?}",
            (rep.stored_matches, rep.produced_matches, rep.output_matches)
        );
        assert_eq!(take.log, rep.take.log, "{rate}: the log");
        let mut tampered = stored.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(
            device::replay(&sc, &tampered).map_or(true, |r| !r.stored_matches),
            "{rate}: a changed stored form is noticed"
        );
    }
}

#[test]
fn failed_recording_leaves_memory_unchanged() {
    // B1: a record-side change the engine refuses (out of range) leaves no item and an idle
    // engine.
    let rate = 16000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    let bad = vec![Scheduled {
        clock: Clock::Input,
        position: 100,
        control: idx("agc.knee_dbfs"),
        value: 99.0,
        tag: 1,
    }];
    assert!(d.record(&voiced(rate, 1.0, 0.5), 0, None, &bad).is_err());
    assert!(d.items().is_empty(), "no item kept");
    assert!(
        d.record(&tone(rate, 0.3, 0.5), 0, None, &[]).is_ok(),
        "the engine is idle again"
    );
}

#[test]
fn record_changes_twin() {
    // B8: record-side changes against a direct engine that schedules them before one call.
    let rate = 48000;
    let s = settings(rate, |_| {});
    let x = voiced(rate, 1.5, 0.6);
    let changes = vec![
        Scheduled {
            clock: Clock::Input,
            position: 12_000,
            control: idx("agc.knee_dbfs"),
            value: -12.0,
            tag: 1,
        },
        Scheduled {
            clock: Clock::Input,
            position: 30_000,
            control: idx("vas.threshold_dbfs"),
            value: -30.0,
            tag: 2,
        },
    ];
    let mut d = Device::new(s).unwrap();
    let out = d.record(&x, 0, None, &changes).unwrap();
    assert_eq!(out.log.iter().filter(|e| e.tag >= 1).count(), 2);
    let mut p = Pipeline::new(s).unwrap();
    p.start_recording().unwrap();
    for c in &changes {
        p.schedule_change(
            rr_dr60::Stream::Record,
            c.position,
            c.control,
            c.value,
            c.tag,
        )
        .unwrap();
    }
    let mut ev = vec![VasEvent::new(0, 0); p.max_record_events(x.len()).max(1)];
    let _ = p.record(&x, &mut ev).unwrap();
    let _ = p.stop_recording(&mut ev).unwrap();
    let mut direct = vec![0u8; p.export_len(1).unwrap()];
    let n = p.export(1, &mut direct).unwrap();
    direct.truncate(n);
    assert_eq!(d.export(1).unwrap(), direct, "the stored form");
    assert_eq!(out.fed, x.len() as u64);
}

#[test]
fn clocks_and_guards() {
    let rate = 8000;
    let s = settings(rate, |_| {});
    let mut d = Device::new(s).unwrap();
    let x = voiced(rate, 1.0, 0.5);
    // B6: a change in the wrong clock is refused before the start.
    let out_clock = vec![Scheduled {
        clock: Clock::Output,
        position: 10,
        control: idx("volume"),
        value: 0.5,
        tag: 1,
    }];
    assert!(d.record(&x, 0, None, &out_clock).is_err());
    d.record(&x, 0, None, &[]).unwrap();
    let in_clock = vec![Scheduled {
        clock: Clock::Input,
        position: 10,
        control: idx("vas.threshold_dbfs"),
        value: -30.0,
        tag: 1,
    }];
    assert!(d.play_to_take(1, &[], &in_clock).is_err());
    // B4: exporting a take after an erase renumbered the items is refused.
    d.record(&tone(rate, 0.5, 0.5), 0, None, &[]).unwrap();
    let take2 = d.play_to_take(2, &[], &[]).unwrap();
    d.erase(1).unwrap();
    d.record(&noise(rate, 0.4, 0.3), 0, None, &[]).unwrap();
    assert!(
        d.export_take(&take2).is_err(),
        "item 2 is now another recording"
    );
    // B5: the playback reader is strict.
    let stored = d.export(1).unwrap();
    use sha2::{Digest, Sha256};
    let sha: [u8; 32] = Sha256::digest(&stored).into();
    let t = d.play_to_take(1, &[], &[]).unwrap();
    let text = device::playback_sidecar(&t, "x.rrdr60", &sha);
    assert!(device::read_playback(&text).is_ok());
    assert!(
        device::read_playback(&format!("{text}colour blue\n")).is_err(),
        "an unknown key"
    );
    let no_knee: String = text
        .lines()
        .filter(|l| !l.starts_with("setting agc.knee_dbfs "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(
        device::read_playback(&no_knee).is_err(),
        "a missing setting"
    );
    assert!(
        device::read_playback(&format!(
            "{text}change out 5 no.such 1 0x3ff0000000000000 9\n"
        ))
        .is_err(),
        "an unknown control"
    );
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

#[test]
fn playback_changes_logged() {
    // US2 AS1: each requested change is logged at its landing, in position order, with its
    // tag; the place reached is the recording's length.
    for rate in [6000, 48000] {
        let mut d = Device::new(settings(rate, |_| {})).unwrap();
        d.record(&voiced(rate, 1.5, 0.5), 0, None, &[]).unwrap();
        let p = u64::from(rate) / 4;
        let changes = vec![
            Scheduled {
                clock: Clock::Output,
                position: p,
                control: idx("volume"),
                value: 0.4,
                tag: 1,
            },
            Scheduled {
                clock: Clock::Output,
                position: 2 * p,
                control: idx("route"),
                value: 1.0,
                tag: 2,
            },
            Scheduled {
                clock: Clock::Output,
                position: 3 * p,
                control: idx("battery_volts"),
                value: 2.5,
                tag: 3,
            },
        ];
        let take = d.play_to_take(1, &[], &changes).unwrap();
        let landed: Vec<_> = take.log.iter().filter(|e| e.tag >= 1).collect();
        assert_eq!(landed.len(), 3, "{rate}");
        for (e, c) in landed.iter().zip(&changes) {
            assert_eq!(
                (e.tag, e.control, e.position),
                (c.tag, c.control, c.position),
                "{rate}"
            );
            assert_eq!(
                e.after,
                f64::from(c.value as f32),
                "{rate}: the value as stored (f32)"
            );
        }
        assert_eq!(take.place, d.items()[0].length, "{rate}: the place reached");
    }
}
