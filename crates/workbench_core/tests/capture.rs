//! Spec 007 [TR: FR-001 – FR-005, SC-001, US1, US2]: the capture core, driven by simulated input
//! callbacks of irregular sizes.

mod support;

use std::sync::atomic::Ordering;

use support::*;
use workbench_core::input::Channel;
use workbench_core::live::{Command, LiveCore, LiveQueues, TakeBuilder, mix_frame};
use workbench_core::render::{Render, RenderRequest};

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

/// Two channels, interleaved: voice on 1, a tone on 2.
fn stereo(rate: u32, secs: f64) -> Vec<f32> {
    let a = voiced(rate, secs, 0.5);
    let b = tone(rate, secs, 0.2);
    a.iter().zip(&b).flat_map(|(x, y)| [*x, *y]).collect()
}

const SIZES: [usize; 9] = [1, 7, 64, 441, 512, 4096, 333, 1000, 2];

/// Runs a capture of `frames` (interleaved, `ch` channels) in callbacks of `SIZES` frames,
/// commands sent before callback numbers; the monitor drained by `drain_monitor` per callback.
fn capture(
    req: &RenderRequest,
    frames: &[f32],
    ch: usize,
    channel: Channel,
    commands: &[(usize, Command)],
    monitor: Option<usize>,
) -> (Render, TakeBuilder, usize, LiveQueues) {
    capture_with(req, &[], frames, ch, channel, commands, monitor)
}

/// [`capture`] with a schedule list (positions from the start, in their clocks).
fn capture_with(
    req: &RenderRequest,
    list: &[(rr_dr60::changes::Clock, u64, u32, f64)],
    frames: &[f32],
    ch: usize,
    channel: Channel,
    commands: &[(usize, Command)],
    monitor: Option<usize>,
) -> (Render, TakeBuilder, usize, LiveQueues) {
    let (mut core, mut q) =
        LiveCore::new_capture(req, list, ch, channel, monitor.is_some()).unwrap();
    let mut b = TakeBuilder::default();
    let mut pos = 0;
    let mut k = 0;
    while pos < frames.len() {
        for (at, c) in commands {
            if *at == k {
                q.commands.push(*c).unwrap();
            }
        }
        let n = (SIZES[k % SIZES.len()] * ch).min(frames.len() - pos);
        core.push(&frames[pos..pos + n]);
        pos += n;
        k += 1;
        b.drain(&mut q);
        if let (Some(per), Some(m)) = (monitor, q.monitor.as_mut()) {
            for _ in 0..per {
                if m.pop().is_err() {
                    break;
                }
            }
        }
    }
    let staged = core.staged();
    b.drain(&mut q);
    let take_req = b.capture_request(req);
    let mut b2 = TakeBuilder::default();
    std::mem::swap(&mut b2, &mut b);
    let replies = b2.replies.clone();
    let captured = b2.captured.clone();
    let take = b2.finish(&take_req, &q.state).unwrap();
    let rest = TakeBuilder {
        replies,
        captured,
        ..TakeBuilder::default()
    };
    (take, rest, staged, q)
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
    assert_eq!(a.meters, b.meters, "{tag}: meter rows");
}

#[test]
fn capture_equals_offline() {
    for rate in RATES {
        let frames = stereo(rate, 1.2);
        let req = request(rate, &[], |_| {});
        let (take, b, staged, q) = capture(&req, &frames, 2, Channel::Index(0), &[], None);
        assert!(!q.state.incomplete.load(Ordering::Relaxed), "{rate}");
        let block = workbench_core::render::block_for(rate);
        assert_eq!(b.captured.len() + staged, frames.len() / 2, "{rate}");
        assert_eq!(b.captured.len() % block, 0, "{rate}: whole chunks");
        let offline = run(&take.request);
        assert_same(&take, &offline, &format!("{rate}"));
        assert!(take.live.as_ref().unwrap().skips.is_empty());
    }
}

#[test]
fn capture_channel_and_mix() {
    let rate = 16_000;
    let frames = stereo(rate, 0.5);
    let req = request(rate, &[], |_| {});
    let (_, b, _, _) = capture(&req, &frames, 2, Channel::Index(1), &[], None);
    let n = b.captured.len();
    let ch2: Vec<f32> = frames
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| f[1])
        .take(n)
        .collect();
    assert!(
        b.captured
            .iter()
            .zip(&ch2)
            .all(|(x, y)| x.to_bits() == y.to_bits())
    );
    let (_, m, _, _) = capture(&req, &frames, 2, Channel::Mix, &[], None);
    let (l, r): (Vec<f32>, Vec<f32>) = frames
        .as_chunks::<2>()
        .0
        .iter()
        .map(|f| (f[0], f[1]))
        .unzip();
    let want = workbench_core::input::mix_floats(&[&l[..], &r[..]]);
    assert!(
        m.captured
            .iter()
            .zip(&want)
            .all(|(x, y)| x.to_bits() == y.to_bits()),
        "001's float mix, frame by frame"
    );
    assert_eq!(
        mix_frame(&[0.25, -0.75, 1.0]),
        ((0.25 - 0.75 + 1.0) / 3.0) as f32
    );
    // One channel.
    let mono = voiced(rate, 0.3, 0.5);
    let (_, one, _, _) = capture(&req, &mono, 1, Channel::Index(0), &[], None);
    assert!(
        one.captured
            .iter()
            .zip(&mono)
            .all(|(x, y)| x.to_bits() == y.to_bits())
    );
}

#[test]
fn capture_commands_reproduce() {
    for rate in [6000, 48_000] {
        let frames = stereo(rate, 1.5);
        let req = request(rate, &[], |p| set_choice(p, "route", "earphone"));
        let cmds = [
            (
                3,
                Command {
                    control: idx("volume"),
                    value: 0.5,
                },
            ),
            (
                9,
                Command {
                    control: idx("agc.knee_dbfs"),
                    value: -20.0,
                },
            ),
            (
                14,
                Command {
                    control: idx("route"),
                    value: 0.0,
                },
            ),
        ];
        let (take, b, _, _) = capture(&req, &frames, 2, Channel::Index(0), &cmds, None);
        assert!(
            b.replies.iter().all(|r| r.error.is_none()),
            "{rate}: {:?}",
            b.replies
        );
        assert!(
            take.log.iter().filter(|e| e.tag >= 1).count() >= 3,
            "{rate}"
        );
        assert_same(&take, &run(&take.request), &format!("{rate}"));
        // The commands changed the take.
        let (plain, _, _, _) = capture(&req, &frames, 2, Channel::Index(0), &[], None);
        assert_ne!(plain.output_sha256, take.output_sha256);
    }
}

#[test]
fn capture_part_chunk() {
    let rate = 8000;
    let block = workbench_core::render::block_for(rate);
    let mono = voiced(rate, 0.5, 0.5);
    let n = 3 * block + 17;
    let req = request(rate, &[], |_| {});
    let (take, b, staged, _) = capture(&req, &mono[..n], 1, Channel::Index(0), &[], None);
    assert_eq!(staged, 17);
    assert_eq!(b.captured.len(), 3 * block);
    assert_eq!(take.live.as_ref().unwrap().fed, 3 * block as u64);
    // Before the first whole chunk: an empty take.
    let (empty, eb, staged, _) = capture(&req, &mono[..block - 1], 1, Channel::Index(0), &[], None);
    assert_eq!((eb.captured.len(), staged), (0, block - 1));
    assert!(empty.samples.is_empty());
}

#[test]
fn capture_wav_roundtrip() {
    for rate in [6000, 44_100] {
        let frames = stereo(rate, 0.6);
        let req = request(rate, &[], |_| {});
        let (take, b, _, _) = capture(&req, &frames, 2, Channel::Mix, &[], None);
        let bytes = workbench_core::export::wav_bytes(&b.captured, rate).unwrap();
        // Review Q4: ±0, subnormals, ±inf and a signalling NaN's payload survive bit for bit.
        let special = [
            0.0f32,
            -0.0,
            1e-40,
            -1e-40,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(0x7fa0_0001),
        ];
        let sb = workbench_core::export::wav_bytes(&special, rate).unwrap();
        let si = workbench_core::input::open_bytes("s.wav", sb, Channel::Index(0)).unwrap();
        assert!(
            si.samples
                .iter()
                .zip(&special)
                .all(|(x, y)| x.to_bits() == y.to_bits())
        );
        let inp =
            workbench_core::input::open_bytes("capture.wav", bytes, Channel::Index(0)).unwrap();
        assert_eq!(inp.rate_hz, rate);
        assert!(
            inp.samples
                .iter()
                .zip(&b.captured)
                .all(|(x, y)| x.to_bits() == y.to_bits())
                && inp.samples.len() == b.captured.len()
        );
        assert_eq!(
            inp.input_sha256, take.request.input_sha256,
            "the capture digest"
        );
    }
}

#[test]
fn capture_sidecar_reproduces() {
    let rate = 48_000;
    let frames = stereo(rate, 1.0);
    let req = request(rate, &[], |p| set_num(p, "vas.hang_ms", 300.0));
    let cmds = [(
        4,
        Command {
            control: idx("volume"),
            value: 0.3,
        },
    )];
    let (take, b, _, _) = capture(&req, &frames, 2, Channel::Index(0), &cmds, None);
    // The exported take's sidecar, reopened with the capture file: a render reproduces it.
    let d = std::env::temp_dir().join(format!("rr-wb-cap-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let path = d.join("capture.wav");
    std::fs::write(
        &path,
        workbench_core::live::capture_wav(&b.captured, rate).unwrap(),
    )
    .unwrap();
    let mut s = workbench_core::session::Session::new();
    s.open(&path, Channel::Index(0)).unwrap();
    let side = workbench_core::sidecar::write(&take);
    let rep = s.restore(&side).unwrap();
    assert!(!rep.input_differs, "the capture digest");
    // The restored sidecar's request (the start values, the range, the changes) reproduces it.
    let again = run(&s.request().unwrap());
    assert_same(&take, &again, "the session's re-render");
    // Accepted as the session's render (the panel takes the end values, as 003's live take).
    assert!(s.accept_live_take(take.clone()));
    assert_eq!(s.render().unwrap().output_sha256, take.output_sha256);
}

#[test]
fn monitor_never_touches_the_take() {
    let rate = 16_000;
    // Longer than the monitor queue holds, so a slow monitor fills it.
    let frames = stereo(rate, 2.5);
    let req = request(rate, &[], |_| {});
    let (plain, ..) = capture(&req, &frames, 2, Channel::Index(0), &[], None);
    // A monitor drained too slowly (drops) and one drained fast; the underrun count is
    // `monitor_latency_bounded`'s.
    let (slow, _, _, q) = capture(&req, &frames, 2, Channel::Index(0), &[], Some(1));
    assert!(
        q.state.monitor_drops.load(Ordering::Relaxed) > 0,
        "a slow monitor drops"
    );
    let (fast, _, _, q2) = capture(&req, &frames, 2, Channel::Index(0), &[], Some(100_000));
    assert_eq!(q2.state.monitor_drops.load(Ordering::Relaxed), 0);
    assert_same(&plain, &slow, "a slow monitor");
    assert_same(&plain, &fast, "a fast monitor");
    assert!(!q.state.incomplete.load(Ordering::Relaxed));
}

#[test]
fn capture_incomplete() {
    let rate = 8000;
    let req = request(rate, &[], |_| {});
    let (mut core, q) = LiveCore::new_capture(&req, &[], 1, Channel::Index(0), false).unwrap();
    // Never drained: past about 10 s the take's queues are full.
    let x = tone(rate, 1.0, 0.1);
    let mut q = q;
    for _ in 0..12 {
        core.push(&x);
    }
    assert!(q.state.incomplete.load(Ordering::Relaxed));
    // The take is marked incomplete and its export refused (coach review 3).
    let mut b = TakeBuilder::default();
    b.drain(&mut q);
    let take_req = b.capture_request(&req);
    let take = b.finish(&take_req, &q.state).unwrap();
    assert!(take.live.as_ref().unwrap().incomplete);
    let e = workbench_core::export::export(
        &mut workbench_core::export::RealFs,
        &std::env::temp_dir().join(format!("rr-wb-inc-{}", std::process::id())),
        &take,
    )
    .unwrap_err();
    assert!(matches!(e, workbench_core::export::ExportError::Incomplete));
}

/// Coach review 3, 4: the window's hand-over. The capture replaces the open file (its render and
/// B are dropped); the take is accepted, not stale; its sidecar, reopened with the capture file,
/// re-renders it bit for bit.
#[test]
fn capture_session_swap() {
    let rate = 48_000;
    let d = std::env::temp_dir().join(format!("rr-wb-swap-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    // An open file with a render and a B.
    let other = d.join("other.wav");
    std::fs::write(&other, wav_i24(rate, &to_i24(&tone(rate, 0.5, 0.1)))).unwrap();
    let mut s = workbench_core::session::Session::new();
    s.open(&other, Channel::Index(0)).unwrap();
    let r = run(&s.request().unwrap());
    assert!(s.accept_render(r));
    assert!(s.keep_as_b());
    // The capture, on channel 2 of a stereo device, with a command.
    let frames = stereo(rate, 1.0);
    let req = request(rate, &[], |p| set_num(p, "vas.hang_ms", 300.0));
    let cmds = [(
        4,
        Command {
            control: idx("volume"),
            value: 0.3,
        },
    )];
    let (take, b, _, _) = capture(&req, &frames, 2, Channel::Index(1), &cmds, None);
    assert_eq!(
        take.request.channel,
        Channel::Index(0),
        "the capture file is mono"
    );
    let path = d.join("capture-1.wav");
    std::fs::write(
        &path,
        workbench_core::live::capture_wav(&b.captured, rate).unwrap(),
    )
    .unwrap();
    assert!(s.accept_capture(&path, take.clone()).unwrap());
    assert!(s.b().is_none(), "the swap drops B");
    assert!(!s.is_stale(), "the take is the session's render, not stale");
    assert!(!s.tail());
    // The exported sidecar, reopened with the capture file, re-renders the take.
    let side = workbench_core::sidecar::write(&take);
    let mut t = workbench_core::session::Session::new();
    t.open(&path, Channel::Index(0)).unwrap();
    assert!(!t.restore(&side).unwrap().input_differs);
    assert_same(&take, &run(&t.request().unwrap()), "the reopened capture");
}

#[test]
fn capture_rate_refused() {
    assert!(workbench_core::capture::rate_refusal(48_000.0).is_none());
    assert!(workbench_core::capture::rate_refusal(6000.0).is_none());
    let e = workbench_core::capture::rate_refusal(22_050.0).unwrap();
    assert!(e.contains("22050") && e.contains("Audio MIDI Setup"), "{e}");
}

#[test]
fn digital_silence_noticed() {
    use workbench_core::capture::SilenceWatch;
    let mut w = SilenceWatch::new(8000);
    w.observe(&vec![0.0; 4000]);
    assert!(!w.silent(), "not before a second");
    w.observe(&vec![0.0; 4000]);
    assert!(w.silent());
    w.observe(&[0.0, 1e-9]);
    assert!(!w.silent(), "a non-zero sample clears it");
    w.observe(&vec![0.0; 80_000]);
    assert!(!w.silent(), "it never returns");
    let mut v = SilenceWatch::new(8000);
    v.observe(&[0.1]);
    v.observe(&vec![0.0; 9000]);
    assert!(!v.silent());
}

#[test]
fn gaps_counted() {
    let g = workbench_core::capture::GapCounter::default();
    // A first callback at sample time 0 (review B4).
    g.observe(0.0, 488, true);
    g.observe(488.0, 512, true);
    g.observe(1000.0, 512, true);
    g.observe(1512.0, 512, true);
    assert_eq!(g.gaps.load(Ordering::Relaxed), 0);
    g.observe(2124.0, 512, true);
    assert_eq!(g.gaps.load(Ordering::Relaxed), 1);
    assert_eq!(g.missing.load(Ordering::Relaxed), 100);
    // A step back is a reset, not a gap; an invalid time re-bases.
    g.observe(10.0, 512, true);
    assert_eq!(g.resets.load(Ordering::Relaxed), 1);
    g.observe(0.0, 512, false);
    g.observe(9999.0, 512, true);
    assert_eq!(g.gaps.load(Ordering::Relaxed), 1);
}

#[test]
fn monitor_latency_bounded() {
    use std::sync::atomic::AtomicU64;
    use workbench_core::capture::{MonitorState, monitor_fill};
    let (mut p, mut c) = rtrb::RingBuffer::<f32>::new(10_000);
    let (u, d) = (AtomicU64::new(0), AtomicU64::new(0));
    let mut m = MonitorState::default();
    let mut out = vec![1.0f32; 100];
    // Priming: nothing plays until the target waits.
    for i in 0..500 {
        p.push(i as f32).unwrap();
    }
    monitor_fill(&mut c, &mut out, &mut m, 1000, 4800, 160, false, &u, &d);
    assert!(!m.primed && out.iter().all(|v| *v == 0.0) && c.slots() == 500);
    // Past the ceiling: whole blocks dropped down to the target, the newest play.
    for i in 500..9000 {
        p.push(i as f32).unwrap();
    }
    monitor_fill(&mut c, &mut out, &mut m, 1000, 4800, 160, false, &u, &d);
    let dropped = d.load(Ordering::Relaxed);
    assert_eq!(dropped % 160, 0, "whole blocks");
    assert!((8000..8160).contains(&dropped), "{dropped}");
    assert_eq!(out[0], dropped as f32);
    // An underrun while the gate is paused is not counted, and re-primes.
    let mut big = vec![1.0f32; 2000];
    monitor_fill(&mut c, &mut big, &mut m, 1000, 4800, 160, true, &u, &d);
    assert_eq!(u.load(Ordering::Relaxed), 0);
    assert!(!m.primed && big[1999] == 0.0);
    // A pull of 2000 primes at max(1000, 2000 + 160) = 2160 (the target follows the pull,
    // review B1); then a second pull with 200 left underruns by 1800, counted with the gate open.
    for i in 0..2200 {
        p.push(i as f32).unwrap();
    }
    monitor_fill(&mut c, &mut big, &mut m, 1000, 4800, 160, false, &u, &d);
    assert!(m.primed && u.load(Ordering::Relaxed) == 0);
    monitor_fill(&mut c, &mut big, &mut m, 1000, 4800, 160, false, &u, &d);
    assert_eq!(
        u.load(Ordering::Relaxed),
        1800,
        "counted while the gate is open"
    );
}

#[test]
fn capture_folder_names() {
    let d = std::env::temp_dir().join(format!("rr-wb-names-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let a = workbench_core::capture::next_path(&d);
    assert!(a.ends_with("capture-1.wav"));
    std::fs::write(&a, b"x").unwrap();
    assert!(workbench_core::capture::next_path(&d).ends_with("capture-2.wav"));
    assert!(workbench_core::capture::folder().ends_with("captures"));
}

/// D3: the panel's values when its rate is the device's, otherwise the defaults, noted.
#[test]
fn capture_default_panel() {
    use workbench_core::live::capture_start_request;
    let mut p = workbench_core::panel::Panel::new(48_000).unwrap();
    set_num(&mut p, "agc.knee_dbfs", -6.0);
    let (r, note) = capture_start_request(Some(&p), 48_000).unwrap();
    assert!(note.is_none());
    assert_eq!(r.settings.agc.knee_dbfs, -6.0);
    let (r, note) = capture_start_request(Some(&p), 44_100).unwrap();
    assert!(note.unwrap().contains("44100 Hz"));
    assert_eq!(r.rate_hz, 44_100);
    assert_eq!(
        r.settings,
        workbench_core::panel::Panel::new(44_100)
            .unwrap()
            .snapshot()
            .0
    );
    let (_, note) = capture_start_request(None, 16_000).unwrap();
    assert!(note.unwrap().contains("No file is open"));
    assert!(capture_start_request(None, 22_050).is_err());
}
