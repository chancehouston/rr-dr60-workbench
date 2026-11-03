//! Spec 005b [TR: FR-006, US3]: settings presets.

mod support;

use rr_dr60::control_map::ControlReading;
use support::*;
use workbench_core::panel::Panel;
use workbench_core::preset;

fn edited(rate: u32) -> Panel {
    let mut p = Panel::new(rate).unwrap();
    set_num(&mut p, "agc.knee_dbfs", -6.0);
    set_num(&mut p, "vas.hang_ms", 300.0);
    set_choice(&mut p, "vas.mode", "mute");
    set_switch(&mut p, "speech_codec_enabled", false);
    set_num(&mut p, "seed", 42.0);
    set_choice(&mut p, "tap", "after_vas");
    set_choice(&mut p, "route", "earphone");
    set_num(&mut p, "volume", 0.5);
    assert!(p.renderable());
    p
}

#[test]
fn preset_roundtrip() {
    for rate in [6000, 48_000] {
        let a = edited(rate);
        let text = preset::write(&a);
        let mut b = Panel::new(rate).unwrap();
        let rep = preset::restore(&preset::read(&text).unwrap(), &mut b);
        assert!(
            rep.unknown_ids.is_empty() && rep.refused.is_empty(),
            "{rep:?}"
        );
        assert!(rep.missing_controls.is_empty() && rep.commit_differs.is_none());
        assert_eq!(rep.rate_differs, None);
        for (x, y) in a.rows().iter().zip(b.rows()) {
            assert_eq!(x.value, y.value, "{rate} {}", x.entry.identifier);
        }
        // Written again, the same text.
        assert_eq!(preset::write(&b), text);
    }
}

#[test]
fn preset_reports() {
    let rate = 16_000;
    let text = preset::write(&edited(48_000));
    // Another rate, an unknown identifier, a missing control and a refused value.
    let text = text
        .lines()
        .filter(|l| !l.starts_with("control vas.onset_ms "))
        .map(|l| {
            if l.starts_with("control agc.attack_ms ") {
                "control agc.attack_ms 99999 0x47c34f80".to_owned()
            } else if l.starts_with("engine.commit ") {
                "engine.commit 0000000000000000000000000000000000000000".to_owned()
            } else {
                l.to_owned()
            }
        })
        .chain(["control no.such_control 1".to_owned()])
        .map(|l| format!("{l}\n"))
        .collect::<String>();
    let mut p = Panel::new(rate).unwrap();
    let rep = preset::restore(&preset::read(&text).unwrap(), &mut p);
    assert_eq!(rep.unknown_ids, ["no.such_control"]);
    assert_eq!(rep.missing_controls, ["vas.onset_ms"]);
    assert!(
        rep.refused.iter().any(|(id, _)| id == "agc.attack_ms"),
        "{rep:?}"
    );
    assert!(rep.commit_differs.is_some());
    assert!(rep.rate_differs.is_some(), "the preset's 48 kHz is noted");
    assert_eq!(p.rate_hz(), rate, "the rate is not applied");
    assert_eq!(
        p.row("host_rate_hz").unwrap().value,
        ControlReading::Choice(rate)
    );
    assert_eq!(
        p.row("agc.knee_dbfs").unwrap().value,
        ControlReading::Continuous(-6.0)
    );
}

#[test]
fn preset_is_not_a_sidecar() {
    let text = preset::write(&edited(8000));
    assert_eq!(
        workbench_core::sidecar::read(&text).unwrap_err(),
        workbench_core::sidecar::SidecarError::NotASidecar
    );
    let x = tone(8000, 0.3, 0.1);
    let r = run(&request(8000, &x, |_| {}));
    let side = workbench_core::sidecar::write(&r);
    assert!(preset::read(&side).is_err());
    assert!(preset::read("rr-dr60-workbench preset 2\n").is_err());
    assert!(preset::read(&text.replacen("engine.commit ", "engine.kommit ", 1)).is_err());
}
