//! Spec 001 post-implementation review, coach 1 – 3, 6 [TR: D5, D9, FR-014, SC-006, US3 AS2,
//! AS4]: the session's rules, tested without a window.

mod support;

use std::path::{Path, PathBuf};

use support::*;
use workbench_core::input::Channel;
use workbench_core::session::{RestoreError, Session};
use workbench_core::{export, sidecar};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rr-wb-s-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn stereo(path: &Path, rate: u32) {
    let a: Vec<i32> = to_i24(&voiced(rate, 1.2, 0.5));
    let b: Vec<i32> = to_i24(&tone(rate, 1.2, 0.3));
    let w = wav(
        rate,
        2,
        WavKind::Int {
            container: 24,
            valid: 24,
            extensible: false,
        },
        &le_codes(&[a, b], 3),
        false,
    );
    std::fs::write(path, w).unwrap();
}

fn render_now(s: &mut Session) {
    let r = run(&s.request().unwrap());
    assert!(s.accept_render(r));
}

#[test]
fn a_render_of_another_file_is_discarded() {
    let d = dir("discard");
    let (a, b) = (d.join("a.wav"), d.join("b.wav"));
    stereo(&a, 16000);
    std::fs::write(&b, wav_i16(48000, &to_i16(&tone(48000, 0.3, 0.5)))).unwrap();
    let mut s = Session::new();
    s.open(&a, Channel::Index(0)).unwrap();
    let r = run(&s.request().unwrap()); // finishes after the user has opened another file
    s.open(&b, Channel::Index(0)).unwrap();
    assert!(
        !s.accept_render(r),
        "a render of a.wav must not attach to b.wav"
    );
    assert!(s.render().is_none());
}

#[test]
fn a_channel_change_keeps_the_render_stale_and_its_sidecar_true() {
    let d = dir("channel");
    let a = d.join("a.wav");
    stereo(&a, 16000);
    let mut s = Session::new();
    s.open(&a, Channel::Index(0)).unwrap();
    render_now(&mut s);
    s.open(&a, Channel::Index(1)).unwrap();
    assert!(
        s.render().is_some() && s.is_stale(),
        "the old render is kept, marked stale"
    );
    // Exporting it writes its own request's input (channel 1), not the current channel 2.
    let stem = d.join("out");
    export::export(&mut export::RealFs, &stem, s.render().unwrap()).unwrap();
    let text = std::fs::read_to_string(export::paths(&stem).1).unwrap();
    assert!(text.contains("\nchannel 1\n"));
    // A fresh session on channel 2 restores channel 1 first, so the input matches (B4).
    let mut t = Session::new();
    t.open(&a, Channel::Index(1)).unwrap();
    let rep = t.restore(&text).unwrap();
    assert_eq!(t.input().unwrap().channel, Channel::Index(0));
    assert!(!rep.input_differs, "{rep:?}");
    render_now(&mut t);
    let want = s.render().unwrap().output_sha256;
    assert_eq!(
        t.render().unwrap().output_sha256,
        want,
        "bit for bit after the channel restore"
    );
}

#[test]
fn a_mix_sidecar_restores_the_mix() {
    let d = dir("mix");
    let a = d.join("a.wav");
    stereo(&a, 8000);
    let mut s = Session::new();
    s.open(&a, Channel::Mix).unwrap();
    render_now(&mut s);
    let text = sidecar::write(s.render().unwrap());
    let mut t = Session::new();
    t.open(&a, Channel::Index(0)).unwrap();
    let rep = t.restore(&text).unwrap();
    assert_eq!(t.input().unwrap().channel, Channel::Mix);
    assert!(!rep.input_differs && rep.renderable);
}

#[test]
fn the_tail_switch_survives_a_front_tap() {
    // Hardware review B5 / coach 9: the switch is written, not inferred from a zero tail.
    let d = dir("tail");
    let a = d.join("a.wav");
    stereo(&a, 16000);
    let mut s = Session::new();
    s.open(&a, Channel::Index(0)).unwrap();
    set_choice(s.panel_mut().unwrap(), "tap", "after_agc");
    render_now(&mut s);
    assert_eq!(s.render().unwrap().tail_samples, 0);
    let text = sidecar::write(s.render().unwrap());
    assert!(text.contains("\ntail on\n"));
    let mut t = Session::new();
    t.open(&a, Channel::Index(0)).unwrap();
    t.set_tail(false);
    t.restore(&text).unwrap();
    assert!(t.tail(), "the tail switch restored on");
}

#[test]
fn refusals_leave_session_unchanged() {
    let d = dir("refuse");
    let a = d.join("a.wav");
    stereo(&a, 16000);
    let (mp3, rate22) = (d.join("x.mp3"), d.join("x.wav"));
    std::fs::write(&mp3, mp3_stub()).unwrap();
    std::fs::write(&rate22, wav_i16(22050, &[0; 100])).unwrap();
    let mut s = Session::new();
    s.open(&a, Channel::Index(0)).unwrap();
    set(
        s.panel_mut().unwrap(),
        "vas.sensitivity",
        rr_dr60::control_map::ControlReading::Integer(5),
    );
    render_now(&mut s);
    let before = s.snapshot();
    let e = s.open(&mp3, Channel::Index(0)).unwrap_err();
    assert!(
        e.to_string().contains("WAV, AIFF and FLAC"),
        "names the formats: {e}"
    );
    let e = s.open(&rate22, Channel::Index(0)).unwrap_err();
    assert!(e.to_string().contains("48000"), "names the rates: {e}");
    let e = s.open(&a, Channel::Index(5)).unwrap_err();
    assert!(e.to_string().contains('6'), "names the channel: {e}");
    assert!(matches!(
        s.restore("nonsense"),
        Err(RestoreError::Sidecar(_))
    ));
    assert!(matches!(
        s.restore("rr-dr60-workbench sidecar 7\n"),
        Err(RestoreError::Sidecar(_))
    ));
    let e = export::check_size(u64::MAX / 4).unwrap_err();
    assert!(e.to_string().contains("limit"), "names the limit: {e}");
    assert_eq!(
        s.snapshot(),
        before,
        "every refusal leaves the session unchanged"
    );
    let mut empty = Session::new();
    assert!(matches!(
        empty.restore(&sidecar::write(s.render().unwrap())),
        Err(RestoreError::NoInput)
    ));
}

#[test]
fn a_live_sidecar_restores_and_reproduces() {
    // Spec 003 D5 end to end: a live take → its v2 sidecar → a fresh session's restore → the
    // session's request (range and changes) → a render equal to the take.
    use rr_dr60::changes::Clock;
    use workbench_core::live::{Command, LiveCore, TakeBuilder};
    let d = dir("live");
    let a = d.join("a.wav");
    stereo(&a, 16000);
    let mut s = Session::new();
    s.open(&a, Channel::Index(0)).unwrap();
    let mut req = s.plain_request().unwrap();
    req.start = 4000;
    let vol = rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == "volume")
        .unwrap() as u32;
    let thr = rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == "vas.threshold_dbfs")
        .unwrap() as u32;
    let (mut core, mut q) = LiveCore::new(&req, &[(Clock::Input, 3000, thr, -30.0)]).unwrap();
    let mut b = TakeBuilder::default();
    let mut buf = vec![0.0f32; 512];
    for k in 0..20 {
        if k == 4 {
            q.commands
                .push(Command {
                    control: vol,
                    value: 0.4,
                })
                .unwrap();
        }
        core.fill(&mut buf);
        b.drain(&mut q);
    }
    b.drain(&mut q);
    let take = b.finish(&req, &q.state).unwrap();
    assert!(
        take.log.iter().filter(|e| e.tag >= 1).count() >= 2,
        "both clocks landed"
    );
    let text = workbench_core::sidecar::write(&take);
    let mut t = Session::new();
    t.open(&a, Channel::Index(0)).unwrap();
    t.restore(&text).unwrap();
    let (start, fed, changes) = t.range().cloned().expect("the live range restored");
    assert_eq!(
        (start, fed, changes.len()),
        (
            4000,
            take.live.as_ref().unwrap().fed,
            take.request.changes.len()
        )
    );
    let r = run(&t.request().unwrap());
    assert!(
        r.samples
            .iter()
            .zip(take.samples.iter())
            .all(|(x, y)| x.to_bits() == y.to_bits())
            && r.samples.len() == take.samples.len(),
        "the restored take re-renders bit for bit"
    );
    assert_eq!(r.log, take.log);
    // The take, accepted with the values at the stop, is not stale.
    assert!(s.accept_live(take, s.plain_request().unwrap()));
    assert!(!s.is_stale());
}
