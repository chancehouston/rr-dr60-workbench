//! Spec 004b [TR: FR-001, FR-002, SC-001, US1]: live playback on a clone of the device.

mod support;

use rr_dr60::changes::Clock;
use rr_dr60::control_map::ControlReading;
use rr_dr60::{Error, Settings};
use support::*;
use workbench_core::device::{self, Device, PlayTake};
use workbench_core::live::{Command, Reply};
use workbench_core::live_play::PlayTakeBuilder;
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

/// Callback sizes, irregular, one above the 4096-frame part.
const SIZES: [usize; 7] = [512, 64, 1000, 4096, 4500, 128, 333];

/// Runs a live playback of item 1 until it has drained or `stop_after` callbacks ran. Each
/// command `(at, control, value)` is sent before the first callback after `at` × 50 ms of
/// output.
fn run_live(
    d: &mut Device,
    values: &[(u32, ControlReading)],
    list: &[Scheduled],
    commands: &[(usize, u32, f64)],
    stop_after: Option<usize>,
) -> (PlayTake, Vec<Reply>) {
    let (mut player, mut q) = d.live_player(1, values, list).unwrap();
    let mut b = PlayTakeBuilder::default();
    let mut buf = vec![0.0f32; 8192];
    let mut k = 0usize;
    let mut sent = vec![false; commands.len()];
    let step = u64::from(q.start.settings.host_rate_hz) / 20;
    loop {
        let produced = q.state.produced.load(std::sync::atomic::Ordering::Relaxed);
        for (i, &(at, control, value)) in commands.iter().enumerate() {
            if !sent[i] && at as u64 * step <= produced {
                q.commands.push(Command { control, value }).unwrap();
                sent[i] = true;
            }
        }
        let n = SIZES[k % SIZES.len()];
        player.fill(&mut buf[..n]);
        b.drain(&mut q);
        let _ = b.take_replies(&mut q);
        let _ = b.take_replies(&mut q);
        k += 1;
        if q.state.drained.load(std::sync::atomic::Ordering::Relaxed)
            || stop_after.is_some_and(|s| k >= s)
        {
            break;
        }
        assert!(k < 1_000_000, "the playback never drained");
    }
    assert!(
        !q.state
            .incomplete
            .load(std::sync::atomic::Ordering::Relaxed)
    );
    b.drain(&mut q);
    let _ = b.take_replies(&mut q);
    let replies = b.replies.clone();
    (b.finish(&q), replies)
}

fn assert_same(live: &PlayTake, off: &PlayTake, what: &str) {
    assert_eq!(live.samples.len(), off.samples.len(), "{what}: length");
    assert!(
        live.samples
            .iter()
            .zip(&off.samples)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "{what}: samples"
    );
    assert_eq!(live.log, off.log, "{what}: log");
    assert_eq!(live.meters, off.meters, "{what}: meter rows");
    assert_eq!((live.frames, live.place), (off.frames, off.place), "{what}");
}

fn seeded_list(rate: u32) -> Vec<Scheduled> {
    let r = u64::from(rate);
    [
        (r / 5, "playback_speed", 4.0),
        (2 * r / 5, "volume", 0.25),
        (3 * r / 5, "route", 1.0),
        (4 * r / 5, "playback_speed_mode", 1.0),
        (r, "playback_speed", 2.0),
    ]
    .iter()
    .enumerate()
    .map(|(k, &(position, id, value))| Scheduled {
        clock: Clock::Output,
        position,
        control: idx(id),
        value,
        tag: k as u64 + 1,
    })
    .collect()
}

#[test]
fn live_play_equals_offline() {
    for rate in RATES {
        let mut d = Device::new(settings(rate, |_| {})).unwrap();
        d.record(&voiced(rate, 2.0, 0.5), 0, None, &[]).unwrap();
        let values = vec![(idx("volume"), ControlReading::Continuous(0.3))];
        let list = seeded_list(rate);
        let (live, replies) = run_live(&mut d, &values, &list, &[], None);
        assert!(
            replies.iter().all(|r| r.error.is_none()),
            "{rate}: {replies:?}"
        );
        assert!(!live.stopped_early);
        assert_eq!(live.changes, list, "{rate}: the list, in scheduling order");
        let off = d.play_to_take(1, &values, &list).unwrap();
        println!(
            "{rate} Hz: {} samples, {} log entries",
            live.samples.len(),
            live.log.len()
        );
        assert_same(&live, &off, &format!("{rate}"));
    }
}

#[test]
fn live_commands_reproduce() {
    for rate in RATES {
        let mut d = Device::new(settings(rate, |_| {})).unwrap();
        d.record(&voiced(rate, 2.0, 0.5), 0, None, &[]).unwrap();
        let list = seeded_list(rate);
        let commands = [
            (3, idx("playback_speed"), 5.0),
            (6, idx("route"), 1.0),
            (6, idx("volume"), 0.2),
            (9, idx("battery_volts"), 2.4),
            (12, idx("route"), 0.0),
            (14, idx("output.speaker_enabled"), 0.0),
            (17, idx("playback_stage_enabled"), 0.0),
            (20, idx("playback_speed_mode"), 0.0),
            (23, idx("output.speaker_enabled"), 1.0),
        ];
        let (live, replies) = run_live(&mut d, &[], &list, &commands, None);
        assert!(
            replies.iter().all(|r| r.error.is_none()),
            "{rate}: {replies:?}"
        );
        assert_eq!(live.changes.len(), list.len() + commands.len(), "{rate}");
        let landed = live
            .log
            .iter()
            .filter(|e| e.tag > list.len() as u64)
            .count();
        assert!(
            landed >= commands.len(),
            "{rate}: every command logged ({landed})"
        );
        // The commands changed the output: the list alone differs.
        let plain = d.play_to_take(1, &live.values, &list).unwrap();
        assert_ne!(plain.samples, live.samples, "{rate}");
        let off = d.play_to_take(1, &live.values, &live.changes).unwrap();
        println!(
            "{rate} Hz: {} samples, {landed} command entries",
            live.samples.len()
        );
        assert_same(&live, &off, &format!("{rate}"));
    }
}

#[test]
fn live_refusal_reported() {
    // A stage that does not run in this configuration (its switch off at construction) is
    // refused in the engine's words and not kept.
    let rate = 16000;
    let s = settings(rate, |p| set_switch(p, "output.speaker_enabled", false));
    let mut d = Device::new(s).unwrap();
    d.record(&voiced(rate, 1.0, 0.5), 0, None, &[]).unwrap();
    let commands = [
        (2, idx("output.speaker_enabled"), 1.0),
        (3, idx("volume"), 0.5),
    ];
    let (live, replies) = run_live(&mut d, &[], &[], &commands, None);
    let refused: Vec<&Reply> = replies.iter().filter(|r| r.error.is_some()).collect();
    assert_eq!(refused.len(), 1, "{replies:?}");
    assert!(matches!(
        refused[0].error,
        Some(Error::StageNotConfigured { .. })
    ));
    println!("refused: {}", refused[0].error.unwrap());
    assert_eq!(live.changes.len(), 1, "only the volume is kept");
    let off = d.play_to_take(1, &live.values, &live.changes).unwrap();
    assert_same(&live, &off, "refusal");
}

#[test]
fn live_take_replays() {
    use sha2::{Digest, Sha256};
    let rate = 44100;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    d.record(&voiced(rate, 1.5, 0.5), 0, None, &[]).unwrap();
    let commands = [(4, idx("playback_speed"), 1.0), (9, idx("route"), 1.0)];
    let (live, _) = run_live(&mut d, &[], &seeded_list(rate), &commands, None);
    let stored = d.export_take(&live).unwrap();
    let sha: [u8; 32] = Sha256::digest(&stored).into();
    let sc = device::read_playback(&device::playback_sidecar(&live, "x.rrdr60", &sha)).unwrap();
    let rep = device::replay(&sc, &stored).unwrap();
    assert!(rep.reproduces());
    assert_eq!(rep.take.log, live.log);
}

#[test]
fn live_stopped_early() {
    use sha2::{Digest, Sha256};
    for rate in [6000, 48000] {
        let mut d = Device::new(settings(rate, |_| {})).unwrap();
        d.record(&voiced(rate, 2.0, 0.5), 0, None, &[]).unwrap();
        let commands = [(2, idx("playback_speed"), 5.0)];
        let (live, _) = run_live(&mut d, &[], &seeded_list(rate), &commands, Some(4));
        assert!(live.stopped_early, "{rate}");
        let off = d.play_to_take(1, &live.values, &live.changes).unwrap();
        let k = live.samples.len();
        assert!(k < off.samples.len(), "{rate}: a prefix");
        assert_eq!(live.samples, off.samples[..k], "{rate}: the prefix");
        let stored = d.export_take(&live).unwrap();
        let sha: [u8; 32] = Sha256::digest(&stored).into();
        let text = device::playback_sidecar(&live, "x.rrdr60", &sha);
        assert!(text.contains("\nstopped early\n"));
        let rep = device::replay(&device::read_playback(&text).unwrap(), &stored).unwrap();
        assert!(rep.reproduces(), "{rate}: the prefix replays");
        assert_eq!(rep.take.log, live.log, "{rate}: the prefix's log");
        assert_eq!(
            rep.take.meters, live.meters,
            "{rate}: the prefix's meter rows"
        );
        assert_eq!(
            (rep.take.frames, rep.take.place),
            (live.frames, live.place),
            "{rate}: the replay stops at the prefix (B7)"
        );
    }
}

#[test]
fn memory_stays_usable_while_live() {
    // The clone (clarification): the device's memory calls act on the device during a live
    // playback, and the live take is unaffected.
    let rate = 8000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    d.record(&voiced(rate, 1.0, 0.5), 0, None, &[]).unwrap();
    d.record(&tone(rate, 0.5, 0.5), 0, None, &[]).unwrap();
    let off = d.play_to_take(1, &[], &[]).unwrap();
    let (mut player, mut q) = d.live_player(1, &[], &[]).unwrap();
    let mut b = PlayTakeBuilder::default();
    let mut buf = vec![0.0f32; 512];
    player.fill(&mut buf);
    d.erase(1).unwrap();
    assert_eq!(d.items().len(), 1);
    while !q.state.drained.load(std::sync::atomic::Ordering::Relaxed) {
        player.fill(&mut buf);
        b.drain(&mut q);
        let _ = b.take_replies(&mut q);
        let _ = b.take_replies(&mut q);
    }
    b.drain(&mut q);
    let _ = b.take_replies(&mut q);
    let live = b.finish(&q);
    assert_eq!(live.samples, off.samples);
    // Item 1 is now the tone: exporting the live take by number is refused (004 B4).
    assert!(d.export_take(&live).is_err());
}

#[test]
fn readings_at_start_cover_the_live_controls() {
    let rate = 16000;
    let mut d = Device::new(settings(rate, |p| {
        set_switch(p, "output.speaker_enabled", false)
    }))
    .unwrap();
    d.record(&tone(rate, 0.3, 0.5), 0, None, &[]).unwrap();
    let values = vec![(idx("volume"), ControlReading::Continuous(0.3))];
    let (_, q) = d.live_player(1, &values, &[]).unwrap();
    let r = workbench_core::live_play::readings_at_start(&q.start);
    assert_eq!(r.len(), workbench_core::live_play::LIVE_PLAY_CONTROLS.len());
    assert!(r.contains(&(idx("volume"), ControlReading::Continuous(0.3))));
    assert!(r.contains(&(idx("output.speaker_enabled"), ControlReading::Switch(false))));
}

#[test]
fn live_ties_reproduce() {
    // B6: a volume list entry on every 10 ms boundary and a volume command at every callback,
    // so commands tie with list entries at one position; the order the engine received them
    // must survive into the replay, and every command sits on a call boundary at or after the
    // output it was sent at.
    for rate in [6000, 48000] {
        let mut d = Device::new(settings(rate, |_| {})).unwrap();
        d.record(&voiced(rate, 1.5, 0.5), 0, None, &[]).unwrap();
        let block = u64::from(rate) / 100;
        let list: Vec<Scheduled> = (1..120)
            .map(|k| Scheduled {
                clock: Clock::Output,
                position: k * block,
                control: idx("volume"),
                value: 0.1 + (k % 7) as f64 * 0.1,
                tag: k,
            })
            .collect();
        let commands: Vec<(usize, u32, f64)> = (0..15)
            .map(|k| (k, idx("volume"), 0.05 + (k % 5) as f64 * 0.15))
            .collect();
        let (live, replies) = run_live(&mut d, &[], &list, &commands, None);
        assert!(
            replies.iter().all(|r| r.error.is_none()),
            "{rate}: {replies:?}"
        );
        let cmds: Vec<&Scheduled> = live.changes.iter().filter(|c| c.tag >= 120).collect();
        assert_eq!(cmds.len(), commands.len(), "{rate}");
        for c in &cmds {
            assert_eq!(
                c.position % block,
                0,
                "{rate}: a command lands on a call boundary"
            );
            let first = live.log.iter().find(|e| e.tag == c.tag).unwrap();
            assert_eq!(
                first.position, c.position,
                "{rate}: it lands where it was scheduled"
            );
        }
        let ties = cmds
            .iter()
            .filter(|c| list.iter().any(|l| l.position == c.position))
            .count();
        assert!(ties > 5, "{rate}: commands tie with list entries ({ties})");
        let off = d.play_to_take(1, &live.values, &live.changes).unwrap();
        assert_same(&live, &off, &format!("{rate} ties"));
    }
}

#[test]
fn command_after_finish_refused() {
    // B3: once the playback has finished, a command is refused (not playing) and not kept.
    let rate = 48000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    d.record(&tone(rate, 0.3, 0.5), 0, None, &[]).unwrap();
    let (mut player, mut q) = d.live_player(1, &[], &[]).unwrap();
    let mut b = PlayTakeBuilder::default();
    let mut buf = vec![0.0f32; 64];
    while !q.state.finished.load(std::sync::atomic::Ordering::Relaxed) {
        player.fill(&mut buf);
    }
    q.commands
        .push(Command {
            control: idx("volume"),
            value: 0.9,
        })
        .unwrap();
    player.fill(&mut buf);
    b.drain(&mut q);
    let r = b.take_replies(&mut q).to_vec();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].error, Some(Error::NotPlaying));
    assert!(b.finish(&q).changes.is_empty());
}

#[test]
fn incomplete_take_not_exported() {
    // B1: a take whose capture overflowed is refused at export.
    let rate = 8000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    d.record(&tone(rate, 0.3, 0.5), 0, None, &[]).unwrap();
    let mut t = d.play_to_take(1, &[], &[]).unwrap();
    assert!(d.export_take(&t).is_ok());
    t.incomplete = true;
    assert!(d.export_take(&t).is_err());
}

#[test]
fn takes_leave_the_device_alike() {
    // B5: a landed stage switch in an offline take does not carry into the device, as in a
    // live take (both play on a clone).
    let rate = 16000;
    let mut d = Device::new(settings(rate, |_| {})).unwrap();
    d.record(&voiced(rate, 0.5, 0.5), 0, None, &[]).unwrap();
    let off = vec![Scheduled {
        clock: Clock::Output,
        position: 100,
        control: idx("output.speaker_enabled"),
        value: 0.0,
        tag: 1,
    }];
    let t1 = d.play_to_take(1, &[], &off).unwrap();
    assert!(t1.log.iter().any(|e| e.tag == 1));
    assert!(
        d.settings().output.speaker_enabled,
        "the device keeps its settings"
    );
    let t2 = d.play_to_take(1, &[], &[]).unwrap();
    assert_eq!(t2.settings, t1.settings);
}
