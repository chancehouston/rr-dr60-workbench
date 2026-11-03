//! Spec 003 [TR: FR-001 – FR-008, SC-001, SC-003, US1, US2]: the live core, driven by simulated
//! callbacks (sizes and commands from a fixed seed, printed).

mod support;

use rr_dr60::changes::Clock;
use support::*;
use workbench_core::live::{Command, LiveCore, LiveQueues, TakeBuilder};
use workbench_core::render::{self, Render, RenderRequest};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

/// Runs a live take: seeded callback sizes, `commands` sent at callback numbers, stopped after
/// `stop_after` callbacks or at the end.
fn live_take(
    req: &RenderRequest,
    list: &[(Clock, u64, u32, f64)],
    commands: &[(usize, Command)],
    seed: u64,
    stop_after: Option<usize>,
) -> (Render, Vec<workbench_core::live::Reply>) {
    let (mut core, mut q): (LiveCore, LiveQueues) = LiveCore::new(req, list).unwrap();
    let mut b = TakeBuilder::default();
    let mut rng = Rng(seed);
    let mut buf = vec![0.0f32; 8192];
    let mut k = 0usize;
    loop {
        for (at, c) in commands {
            if *at == k {
                q.commands.push(*c).unwrap();
            }
        }
        let n = 1 + rng.below(if k.is_multiple_of(7) { 6000 } else { 1024 }) as usize;
        core.fill(&mut buf[..n]);
        assert!(
            core.fifo_len() <= workbench_core::live::MAX_PART + 2 * render::block_for(req.rate_hz),
            "the FIFO's bound"
        );
        b.drain(&mut q);
        k += 1;
        let finished = q.state.finished.load(std::sync::atomic::Ordering::Relaxed);
        if stop_after.is_some_and(|s| k >= s) || (finished && core.fifo_len() == 0) {
            break;
        }
    }
    b.drain(&mut q);
    let replies = b.replies.clone();
    (b.finish(req, &q.state).unwrap(), replies)
}

fn assert_same(a: &Render, b: &Render, tag: &str) {
    assert_eq!(a.samples.len(), b.samples.len(), "{tag}: length");
    assert!(
        a.samples
            .iter()
            .zip(b.samples.iter())
            .all(|(x, y)| x.to_bits() == y.to_bits()),
        "{tag}: samples"
    );
    assert_eq!(a.events, b.events, "{tag}: gate reports");
    assert_eq!(a.log, b.log, "{tag}: change log");
    if a.meters != b.meters {
        let i = a.meters.iter().zip(&b.meters).position(|(x, y)| x != y);
        if let Some(i) = i {
            panic!(
                "{tag}: meter rows differ first at row {i} of {}/{}:\nlive {:?}\nre   {:?}",
                a.meters.len(),
                b.meters.len(),
                a.meters[i],
                b.meters[i]
            );
        }
        panic!(
            "{tag}: meter rows differ in count {} vs {}",
            a.meters.len(),
            b.meters.len()
        );
    }
}

#[test]
fn live_equals_render() {
    for rate in RATES {
        let x = voiced(rate, 1.5, 0.5);
        let mut req = request(rate, &x, |p| set_num(p, "vas.hang_ms", 50.0));
        req.start = (rate / 4) as u64; // a mid-file start
        let seed = 0x5EED_0000 + u64::from(rate);
        let (take, _) = live_take(&req, &[], &[], seed, None);
        assert!(!take.live.as_ref().unwrap().incomplete, "{rate}: complete");
        let offline = run(&req);
        println!("{rate} Hz seed {seed:#x}: {} samples", take.samples.len());
        assert_same(&take, &offline, &format!("{rate}"));
    }
}

#[test]
fn live_change_logged() {
    let rate = 48000;
    let x = voiced(rate, 1.0, 0.5);
    let req = request(rate, &x, |_| {});
    let c = Command {
        control: idx("volume"),
        value: 0.5,
    };
    let (take, replies) = live_take(&req, &[], &[(5, c)], 7, None);
    assert!(replies.iter().all(|r| r.error.is_none()));
    let e = take
        .log
        .iter()
        .find(|e| e.tag >= 1)
        .expect("the change is logged");
    assert_eq!(e.control, idx("volume"));
    assert_eq!(e.after, 0.5);
    assert!(e.before != e.after);
}

#[test]
fn live_record_side_change() {
    let rate = 16000;
    let x = voiced(rate, 1.0, 0.5);
    let req = request(rate, &x, |_| {});
    let c = Command {
        control: idx("vas.threshold_dbfs"),
        value: -30.0,
    };
    let (take, _) = live_take(&req, &[], &[(3, c)], 11, None);
    let e = take.log.iter().find(|e| e.tag >= 1).expect("logged");
    assert_eq!(rr_dr60::changes::clock_of(e.control).unwrap(), Clock::Input);
}

#[test]
fn live_disabled_reasons() {
    use workbench_core::live::live_reason;
    use workbench_core::text;
    let rate = 48000;
    let x = tone(rate, 0.1, 0.5);
    let req = request(rate, &x, |_| {});
    assert_eq!(
        live_reason(&req, idx("speech_codec_enabled")),
        Some(text::LIVE_IDLE)
    );
    assert_eq!(
        live_reason(&req, idx("playback_speed")),
        Some(text::LIVE_NOT_ONE_PASS)
    );
    assert_eq!(live_reason(&req, idx("volume")), None);
    assert_eq!(
        live_reason(&req, idx("band_filter.frequency_hz")),
        Some(text::LIVE_SLOT_EMPTY)
    );
    let tapped = request(rate, &x, |p| set_choice(p, "tap", "after_speaker"));
    assert_eq!(
        live_reason(&tapped, idx("route")),
        Some(text::LIVE_TAP_ROUTE),
        "plan review B1"
    );
    let off = request(rate, &x, |p| set_switch(p, "output.speaker_enabled", false));
    assert_eq!(
        live_reason(&off, idx("output.speaker_enabled")),
        Some(text::LIVE_STAGE_OFF)
    );
    let slot = request(rate, &x, |p| {
        set_choice(p, "insert.before_limiter", "band_filter")
    });
    assert_eq!(live_reason(&slot, idx("band_filter.frequency_hz")), None);
}

#[test]
fn live_take_reproduces() {
    for rate in RATES {
        // Long enough that the run is still going when it is stopped by hand at callback 30.
        let x = voiced(rate, 10.0, 0.5);
        let req = request(rate, &x, |p| {
            set_choice(p, "insert.before_limiter", "band_filter");
            set_num(p, "vas.hang_ms", 100.0);
        });
        let seed = 0xC0FFEE ^ u64::from(rate);
        let mut rng = Rng(seed);
        let mut commands = Vec::new();
        for k in 0..12 {
            let (control, value) = match rng.below(4) {
                0 => (idx("volume"), (rng.below(100) as f64) / 100.0),
                1 => (idx("vas.threshold_dbfs"), -(30.0 + rng.below(30) as f64)),
                2 => (idx("route"), rng.below(2) as f64),
                _ => (
                    idx("band_filter.frequency_hz"),
                    300.0 + rng.below(2000) as f64,
                ),
            };
            commands.push((2 + k * 3, Command { control, value }));
        }
        let list = vec![(Clock::Output, u64::from(rate) / 2, idx("volume"), 0.25)];
        // Stopped by hand mid-file.
        let (take, replies) = live_take(&req, &list, &commands, seed, Some(30));
        assert!(!take.live.as_ref().unwrap().incomplete);
        let landed = take.log.iter().filter(|e| e.tag >= 1).count();
        println!(
            "{rate}: {landed} changes landed, {} replies with errors: {:?}",
            replies.iter().filter(|r| r.error.is_some()).count(),
            replies
                .iter()
                .filter_map(|r| r.error)
                .take(3)
                .collect::<Vec<_>>()
        );
        assert!(landed >= 5, "{rate}: changes landed");
        let text = workbench_core::sidecar::write(&take);
        let s = workbench_core::sidecar::read(&text).unwrap();
        assert_eq!(s.version, 2);
        let inp = input_of(rate, &x);
        let _ = inp;
        let re = run(&take.request);
        println!(
            "{rate} Hz seed {seed:#x}: {} samples, {} changes, {} refused",
            take.samples.len(),
            take.request.changes.len(),
            replies.iter().filter(|r| r.error.is_some()).count()
        );
        assert_same(&take, &re, &format!("{rate}"));
        // The sidecar's change lines carry the take's changes exactly.
        assert_eq!(s.changes.len(), take.request.changes.len());
        for ((clock, pos, _, v, tag), c) in s.changes.iter().zip(&take.request.changes) {
            assert_eq!(
                (*clock, *pos, v.to_bits(), *tag),
                (c.clock, c.position, c.value.to_bits(), c.tag)
            );
        }
    }
}

#[test]
fn schedule_lands_exactly() {
    let rate = 8000;
    let x = voiced(rate, 1.0, 0.5);
    let req = request(rate, &x, |_| {});
    let list = vec![
        (Clock::Output, 2000, idx("volume"), 0.5),
        (Clock::Input, 4000, idx("vas.threshold_dbfs"), -35.0),
    ];
    let (take, _) = live_take(&req, &list, &[], 3, None);
    let vol = take.log.iter().find(|e| e.tag == 1).unwrap();
    assert_eq!(vol.position, 2000);
    let thr = take.log.iter().find(|e| e.tag == 2).unwrap();
    assert_eq!(thr.position, 4000);
}

#[test]
fn schedule_limit_refused() {
    let rate = 6000;
    let x = tone(rate, 0.5, 0.5);
    let req = request(rate, &x, |_| {});
    let list: Vec<(Clock, u64, u32, f64)> = (0..70)
        .map(|i| (Clock::Output, 100, idx("volume"), f64::from(i) / 100.0))
        .collect();
    let (_, replies) = live_take(&req, &list, &[], 5, None);
    let refused: Vec<_> = replies.iter().filter(|r| r.error.is_some()).collect();
    assert_eq!(refused.len(), 70 - 64, "beyond the 64 pending per clock");
    assert!(
        refused[0].error.unwrap().to_string().len() > 5,
        "the engine's words"
    );
}

#[test]
fn timeline_lists_reports_and_skips() {
    let rate = 16000;
    let b = voiced(rate, 0.4, 0.5);
    let x = [b.clone(), vec![0.0; 6 * rate as usize], b].concat();
    let req = request(rate, &x, |p| set_choice(p, "vas.mode", "drop"));
    let (take, _) = live_take(&req, &[], &[], 9, None);
    let live = take.live.as_ref().unwrap();
    assert!(!take.events.is_empty(), "the gate's reports");
    assert!(
        !live.skips.is_empty(),
        "the budget's skips during the long pause"
    );
    let csv = workbench_core::csv::timeline_csv(&take);
    let rows = csv.lines().filter(|l| !l.starts_with('#')).count() - 1;
    assert_eq!(rows, take.log.len() + take.events.len() + live.skips.len());
}

#[test]
fn overflow_marks_incomplete() {
    // A UI that never drains: the capture queue fills and the take is marked incomplete.
    let rate = 48000;
    let x = voiced(rate, 3.0, 0.5);
    let req = request(rate, &x, |_| {});
    let (mut core, q) = LiveCore::new(&req, &[]).unwrap();
    let mut buf = vec![0.0f32; 512];
    for _ in 0..300 {
        core.fill(&mut buf);
    }
    assert!(
        q.state
            .incomplete
            .load(std::sync::atomic::Ordering::Relaxed)
    );
}
