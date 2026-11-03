//! Spec 006 [TR: FR-001 – FR-004, SC-002, US1, US2]: the entry-point checks and the tour.

mod support;

use std::sync::atomic::AtomicBool;

use rr_dr60::changes::Clock;
use rr_dr60::control_map::ControlReading;
use support::*;
use workbench_core::checks::{self, Form, Refusal};
use workbench_core::device::Device;
use workbench_core::render::Scheduled;
use workbench_core::tour;

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

fn schedule(rate: u32) -> Vec<Scheduled> {
    let fs = u64::from(rate);
    vec![
        Scheduled {
            clock: Clock::Input,
            position: fs * 3 / 10,
            control: idx("agc.knee_dbfs"),
            value: -20.0,
            tag: 1,
        },
        Scheduled {
            clock: Clock::Output,
            position: fs * 6 / 10,
            control: idx("volume"),
            value: 0.3,
            tag: 2,
        },
    ]
}

#[test]
fn one_pass_forms_agree() {
    let no = AtomicBool::new(false);
    for rate in RATES {
        for mode in ["drop", "mute"] {
            // Voice, a pause the gate takes (hang 100 ms), voice.
            let mut x = voiced(rate, 0.6, 0.5);
            x.extend(vec![0.0; rate as usize / 2]);
            x.extend(voiced(rate, 0.6, 0.5));
            let mut req = request(rate, &x, |p| {
                set_choice(p, "vas.mode", mode);
                set_num(p, "vas.hang_ms", 100.0);
            });
            req.changes = schedule(rate);
            let c = checks::one_pass_check(&req, &no);
            for f in &c.forms {
                assert!(
                    f.run.is_ok(),
                    "{rate} {mode} {}: {:?}",
                    f.form.name(),
                    f.run.as_ref().err()
                );
                assert!(f.agrees, "{rate} {mode}: {} disagrees", f.form.name());
            }
            // The render's own form is the render.
            let r = run(&req);
            let first = c.forms[0].run.as_ref().unwrap();
            assert_eq!(first.samples.len(), r.samples.len());
            assert!(
                first
                    .samples
                    .iter()
                    .zip(r.samples.iter())
                    .all(|(a, b)| a.to_bits() == b.to_bits())
            );
            assert_eq!(first.events, r.events);
            assert_eq!(first.log, r.log);
            assert!(!first.events.is_empty(), "{rate} {mode}: the gate acted");
            let reset = c.reset.as_ref().unwrap();
            assert!(
                reset.agrees,
                "{rate} {mode}: the reset render against its oracle"
            );
        }
    }
}

/// Hardware review B1: the reset render starts from the landed values, so it differs from the
/// first render when a schedule moved a value, and its oracle (a fresh engine from the state)
/// still agrees.
#[test]
fn reset_keeps_the_landed_values() {
    let rate = 16_000;
    let no = AtomicBool::new(false);
    let x = voiced(rate, 1.2, 0.5);
    let mut req = request(rate, &x, |_| {});
    req.changes = schedule(rate);
    let first = checks::run_on(
        &mut workbench_core::render::fresh_engine(&req).unwrap(),
        &req,
        Form::WithEventsAndChanges,
        &no,
    )
    .unwrap();
    let rc = checks::reset_check(&req, &no).unwrap();
    assert!(rc.agrees);
    assert_ne!(
        first.samples, rc.reset_run.samples,
        "the reset render starts from the landed volume and knee"
    );
}

#[test]
fn in_place_refusal_reported() {
    let rate = 8000;
    let no = AtomicBool::new(false);
    let x = voiced(rate, 0.5, 0.5);
    // A named route tap of the other route: the speaker tap, the route then moved to the
    // earphone by an engine value (the panel refuses the pair; it arises as a live change).
    let mut req = request(rate, &x, |p| set_choice(p, "tap", "after_speaker"));
    let route = idx("route");
    match req.engine_values.iter_mut().find(|(i, _)| *i == route) {
        Some(v) => v.1 = ControlReading::Choice(1),
        None => req.engine_values.push((route, ControlReading::Choice(1))),
    }
    let c = checks::one_pass_check(&req, &no);
    for f in &c.forms {
        match f.form {
            Form::InPlace | Form::InPlaceWithEvents | Form::InPlaceWithEventsAndChanges => {
                assert_eq!(
                    f.run.as_ref().unwrap_err(),
                    &Refusal::InPlaceRefused,
                    "{}",
                    f.form.name()
                );
            }
            _ => assert!(
                matches!(
                    f.run,
                    Err(Refusal::Engine(rr_dr60::Error::TapNotOnRoute { .. }))
                ),
                "{}: {:?}",
                f.form.name(),
                f.run.as_ref().err()
            ),
        }
    }
    // A drop-mode pause is not a refusal: silence after voice, every form runs.
    let mut quiet = voiced(rate, 0.3, 0.5);
    quiet.extend(vec![0.0; rate as usize * 2]);
    let req = request(rate, &quiet, |p| {
        set_choice(p, "vas.mode", "drop");
        set_num(p, "vas.hang_ms", 50.0);
    });
    let c = checks::one_pass_check(&req, &no);
    let first = c.forms[0].run.as_ref().unwrap();
    assert!(
        first.calls.iter().any(|k| k.paused && k.produced == 0),
        "the gate paused"
    );
    assert!(c.forms.iter().all(|f| f.run.is_ok() && f.agrees));
}

#[test]
fn playback_forms_agree() {
    for rate in [6000, 48_000] {
        let mut p = workbench_core::panel::Panel::new(rate).unwrap();
        set_num(&mut p, "memory.capacity_s", 2.0);
        let mut d = Device::new(p.snapshot().0).unwrap();
        d.record(&voiced(rate, 1.0, 0.5), 0, None, &[]).unwrap();
        let values = vec![
            (idx("route"), ControlReading::Choice(1)),
            (idx("volume"), ControlReading::Continuous(0.4)),
            (idx("battery_volts"), ControlReading::Continuous(2.6)),
            (idx("playback_speed"), ControlReading::Integer(4)),
            (idx("playback_speed_mode"), ControlReading::Choice(1)),
        ];
        let changes = vec![Scheduled {
            clock: Clock::Output,
            position: u64::from(rate) / 4,
            control: idx("volume"),
            value: 0.2,
            tag: 1,
        }];
        let before = d.call_readings().getters;
        let c = d.playback_check(1, &values, &changes).unwrap();
        assert!(c.identical, "{rate}: {:?}", c.lengths);
        assert!(c.equals_take, "{rate}");
        for g in &c.getters {
            assert_eq!(g.route, rr_dr60::Route::Earphone);
            assert_eq!(g.volume, 0.4f32);
            assert_eq!(g.battery_volts, 2.6f32);
            assert_eq!(g.playback_speed, 4);
            assert_eq!(g.playback_speed_mode, rr_dr60::speed::SpeedMode::PitchKept);
        }
        assert_eq!(
            d.call_readings().getters,
            before,
            "{rate}: the device is unchanged"
        );
    }
}

#[test]
fn readings_by_their_calls() {
    use rr_dr60::components::ComponentId;
    for rate in RATES {
        for codec in [false, true] {
            let mut p = workbench_core::panel::Panel::new(rate).unwrap();
            set_switch(&mut p, "speech_codec_enabled", codec);
            set_num(&mut p, "memory.capacity_s", 2.0);
            let mut d = Device::new(p.snapshot().0).unwrap();
            d.set_value(
                idx("band_filter.frequency_hz"),
                ControlReading::Continuous(500.0),
            )
            .unwrap();
            d.set_value(
                idx("insert.before_limiter"),
                ControlReading::Choice(ComponentId::BandFilter as u32),
            )
            .unwrap();
            let r = d.call_readings();
            assert_eq!(r.component, ComponentId::BandFilter);
            assert_eq!(r.parameters[1], 500.0);
            assert!(r.parameters.len() >= 4);
            let delay = if codec { 180 * rate / 6000 } else { 0 };
            let want = r.record_latency - delay;
            assert!(
                r.pre_encoder_latency.abs_diff(want) <= 1,
                "{rate} {codec}: {} vs {want}",
                r.pre_encoder_latency
            );
        }
    }
}

#[test]
fn tour_follows_the_manifest() {
    let pages = tour::pages();
    assert_eq!(pages.len(), 16);
    assert_eq!(*pages.last().unwrap(), 0, "the no-stage page last");
    // The pages partition every control.
    let mut seen = vec![0usize; rr_dr60::controls::controls().len()];
    for p in &pages {
        for e in tour::controls(*p) {
            let i = idx(e.identifier) as usize;
            seen[i] += 1;
        }
    }
    assert!(seen.iter().all(|n| *n == 1), "{seen:?}");
    // Stage 4 has two switches; stages 8, 9 and 15 none.
    assert_eq!(tour::switches(4).len(), 2);
    for s in [8u8, 9, 15, 0] {
        assert!(tour::switches(s).is_empty(), "{s}");
    }
    // The tour module writes no stage text: no string literal longer than a word.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tour.rs"),
    )
    .unwrap();
    let body = &src[src.find("pub fn pages").unwrap()..src.find("impl std::fmt::Display").unwrap()];
    let code: Vec<&str> = body
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect();
    assert!(
        code.iter().all(|l| !l.contains('"')),
        "no literals in the page logic"
    );
}

#[test]
fn compare_without_stage() {
    let rate = 16_000;
    let x = voiced(rate, 1.0, 0.5);
    let mut req = request(rate, &x, |_| {});
    req.changes = vec![Scheduled {
        clock: Clock::Output,
        position: 1000,
        control: idx("output.volume_amp_enabled"),
        value: 0.0,
        tag: 1,
    }];
    let r = run(&req);
    // Stage 11: its switch off, its scheduled entry removed.
    let w = tour::without_stage(&r, &req, 11).unwrap();
    assert_eq!(
        w.turned_off,
        [(
            idx("output.volume_amp_enabled"),
            "output.volume_amp_enabled"
        )]
    );
    assert_eq!(w.removed.len(), 1);
    let b = run(&w.request);
    let d = workbench_core::analysis::compare::param_diff(&w.request, &r.request);
    let ids: Vec<_> = d.controls.iter().map(|c| c.identifier).collect();
    assert_eq!(ids, ["output.volume_amp_enabled"]);
    assert_eq!(d.b_only.len(), 1, "the removed entry");
    assert!(!tour::no_effect(&r, &b));
    // Stage 4's two switches.
    let w4 = tour::without_stage(&r, &req, 4).unwrap();
    assert_eq!(w4.turned_off.len(), 2);
}

#[test]
fn compare_without_stage_no_effect_and_refused() {
    let rate = 8000;
    let x = voiced(rate, 0.6, 0.5);
    let req = request(rate, &x, |_| {});
    let r = run(&req);
    // Stage 13 (the earphone) on the speaker route: no effect.
    let w = tour::without_stage(&r, &req, 13).unwrap();
    assert!(tour::no_effect(&r, &run(&w.request)));
    // A stale render is refused; a stage with no switch is not offered.
    let other = request(rate, &x, |p| set_num(p, "agc.knee_dbfs", -6.0));
    assert_eq!(
        tour::without_stage(&r, &other, 3).unwrap_err(),
        tour::NotOffered::Stale
    );
    assert_eq!(
        tour::without_stage(&r, &req, 9).unwrap_err(),
        tour::NotOffered::NoSwitch
    );
    let off = request(rate, &x, |p| set_switch(p, "agc.enabled", false));
    let ro = run(&off);
    assert_eq!(
        tour::without_stage(&ro, &off, 3).unwrap_err(),
        tour::NotOffered::AlreadyOff
    );
}
