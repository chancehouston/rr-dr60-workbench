//! Spec 001 T020 [TR: FR-004, SC-003, US2 AS1; FR-005, US2 AS5; FR-006, US2 AS3, AS4; US2 AS2,
//! AS6; US2 AS7]: the panel presents every manifest control, classifies and checks through the
//! engine, and grows with the engine.

mod support;

use rr_dr60::control_map::ControlReading;
use rr_dr60::controls::{ControlPaths, controls};
use support::*;
use workbench_core::panel::{ControlClass, Panel, RowStatus};

#[test]
fn presents_every_control() {
    let p = Panel::new(48000).unwrap();
    let m = controls();
    assert_eq!(p.rows().len(), m.len());
    for (row, e) in p.rows().iter().zip(m) {
        assert_eq!(row.entry, *e, "the entry as the engine gives it");
        assert_ne!(row.class, ControlClass::Unknown, "{}", e.identifier);
        // The default reading is the engine's.
        let engine = rr_dr60::Pipeline::new(rr_dr60::Settings::new(48000))
            .unwrap()
            .control_value(row.index)
            .unwrap();
        assert_eq!(row.value, engine, "{}: default", e.identifier);
    }
    assert!(p.renderable());
}

#[test]
fn grows_with_the_engine() {
    // A copied entry list with one entry mutated and one appended (a #[non_exhaustive] struct can
    // be copied and its fields changed outside its crate).
    let mut entries = controls().to_vec();
    entries[15].name = "A renamed control";
    let mut extra = entries[1];
    extra.identifier = "future.control";
    extra.name = "A future control";
    entries.push(extra);
    let p = Panel::from_entries(&entries, 16000).unwrap();
    assert_eq!(p.rows().len(), controls().len() + 1);
    assert_eq!(p.rows()[15].entry.name, "A renamed control");
    let last = p.rows().last().unwrap();
    assert_eq!(last.entry.identifier, "future.control");
    assert_eq!(last.class, ControlClass::Unknown);
    assert!(
        matches!(last.status, RowStatus::Refused(_)),
        "this engine does not know it"
    );
}

#[test]
fn not_on_the_one_pass_and_read_only() {
    let mut p = Panel::new(44100).unwrap();
    for row in p.rows().to_vec() {
        if row.entry.identifier == "host_rate_hz" {
            assert_eq!(row.status, RowStatus::ReadOnly);
            assert_eq!(row.value, ControlReading::Choice(44100));
            assert!(!p.set(row.index, ControlReading::Choice(48000)));
        } else if !row.entry.paths.contains(ControlPaths::ONE_PASS) {
            assert_eq!(
                row.status,
                RowStatus::NotOnOnePass,
                "{}",
                row.entry.identifier
            );
            // Settable since spec 004 (the device uses them); the label stays.
            assert!(p.set(row.index, row.value), "{}", row.entry.identifier);
            assert_eq!(
                p.row(row.entry.identifier).unwrap().status,
                RowStatus::NotOnOnePass
            );
        }
    }
    for id in ["memory.capacity_s", "playback_speed", "playback_speed_mode"] {
        assert_eq!(p.row(id).unwrap().status, RowStatus::NotOnOnePass, "{id}");
    }
}

#[test]
fn engine_checks_each_class() {
    let mut p = Panel::new(48000).unwrap();
    // A settings field out of range: refused in the engine's words, Render unavailable.
    set(&mut p, "vas.sensitivity", ControlReading::Integer(6));
    let r = p.row("vas.sensitivity").unwrap();
    let RowStatus::Refused(msg) = &r.status else {
        panic!("sensitivity 6 accepted")
    };
    let mut s = rr_dr60::Settings::new(48000);
    let i = p.index_of("vas.sensitivity").unwrap();
    rr_dr60::control_map::set_settings_value(&mut s, i, ControlReading::Integer(6)).unwrap();
    assert_eq!(
        *msg,
        s.validate().unwrap_err().to_string(),
        "the engine's words"
    );
    assert!(!p.renderable());
    set(&mut p, "vas.sensitivity", ControlReading::Integer(5));
    assert!(p.renderable());
    // A live control out of range, through set_control_value.
    set(&mut p, "volume", ControlReading::Continuous(1.5));
    assert!(matches!(
        p.row("volume").unwrap().status,
        RowStatus::Refused(_)
    ));
    assert!(!p.renderable());
    set(&mut p, "volume", ControlReading::Continuous(0.5));
    assert!(p.renderable());
    // The earphone tap with the speaker route: refused beside both (US2 AS4).
    set_choice(&mut p, "tap", "after_earphone");
    assert!(matches!(
        p.row("tap").unwrap().status,
        RowStatus::Refused(_)
    ));
    assert!(matches!(
        p.row("route").unwrap().status,
        RowStatus::Refused(_)
    ));
    assert!(!p.renderable());
    set_choice(&mut p, "route", "earphone");
    assert!(p.renderable(), "the earphone route fixes it");
}

#[test]
fn sensitivity_5_matches_oracle() {
    let rate = 16000;
    let x = voiced(rate, 1.5, 0.3);
    let req = request(rate, &x, |p| {
        set(p, "vas.sensitivity", ControlReading::Integer(5))
    });
    assert_eq!(req.settings.vas.sensitivity, 5);
    let r = run(&req);
    let (o, _) = oracle(
        req.settings,
        &req.engine_values,
        &with_tail(&x, r.tail_samples),
        0,
    );
    assert_eq!(&r.samples[..], &o[..]);
}

#[test]
fn band_filter_matches_oracle() {
    let rate = 48000;
    let x = noise(rate, 0.5, 0.3);
    let req = request(rate, &x, |p| {
        set_choice(p, "insert.before_limiter", "band_filter");
        set_num(p, "band_filter.frequency_hz", 800.0);
    });
    let r = run(&req);
    let (o, _) = oracle(
        req.settings,
        &req.engine_values,
        &with_tail(&x, r.tail_samples),
        1,
    );
    assert_eq!(&r.samples[..], &o[..]);
    let plain = run(&request(rate, &x, |_| {}));
    assert_ne!(
        &r.samples[..],
        &plain.samples[..],
        "the filter changes the render"
    );
}

#[test]
fn stale_and_reset() {
    let rate = 8000;
    let x = tone(rate, 0.2, 0.5);
    let req = request(rate, &x, |_| {});
    let r = run(&req);
    assert!(!r.is_stale(&req));
    let edited = request(rate, &x, |p| {
        set(p, "vas.sensitivity", ControlReading::Integer(4))
    });
    assert!(r.is_stale(&edited));
    let mut seed = req.clone();
    seed.settings.seed ^= 1;
    assert!(r.is_stale(&seed));
    let mut no_tail = req.clone();
    no_tail.tail = false;
    assert!(r.is_stale(&no_tail));
    // Reset to defaults restores every row.
    let mut p = Panel::new(rate).unwrap();
    set(&mut p, "vas.sensitivity", ControlReading::Integer(4));
    set(&mut p, "volume", ControlReading::Continuous(0.9));
    p.reset_to_defaults();
    let fresh = Panel::new(rate).unwrap();
    for (a, b) in p.rows().iter().zip(fresh.rows()) {
        assert_eq!(a.value, b.value, "{}", a.entry.identifier);
        assert_eq!(a.status, b.status);
    }
}
