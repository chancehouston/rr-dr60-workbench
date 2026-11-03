//! Spec 001 T025 [TR: FR-013; FR-014, SC-002, US3 AS2 – AS5]: the sidecar's grammar, values,
//! reproduction and differences.

mod support;

use rr_dr60::control_map::ControlReading;
use rr_dr60::controls::ControlValue;
use support::*;
use workbench_core::panel::Panel;
use workbench_core::sidecar::{self, SidecarError};

#[test]
fn values_roundtrip() {
    // Every control, at its range ends where it has a range, survives write → read → restore.
    let rate = 48000;
    let x = tone(rate, 0.1, 0.5);
    let inp = input_of(rate, &x);
    let mut ends: Vec<(String, ControlReading)> = Vec::new();
    let p0 = Panel::new(rate).unwrap();
    for row in p0.rows() {
        match row.entry.value {
            ControlValue::Continuous { min, max, .. } => {
                ends.push((row.entry.identifier.into(), ControlReading::Continuous(min)));
                ends.push((row.entry.identifier.into(), ControlReading::Continuous(max)));
            }
            ControlValue::Integer { min, max, .. } => {
                ends.push((row.entry.identifier.into(), ControlReading::Integer(min)));
                ends.push((row.entry.identifier.into(), ControlReading::Integer(max)));
            }
            _ => {}
        }
    }
    let total = ends.len();
    let mut tested = 0;
    let mut skipped: Vec<String> = Vec::new();
    for (id, v) in ends {
        let mut p = Panel::new(rate).unwrap();
        let i = p.index_of(&id).unwrap();
        if !p.set(i, v) || !p.renderable() {
            skipped.push(id.clone());
            continue; // read-only, not on the one-pass, or an end the whole check refuses
        }
        let r = run(&request_for(&inp, &p, true));
        let text = sidecar::write(&r);
        let s = sidecar::read(&text).unwrap();
        let mut q = Panel::new(rate).unwrap();
        let rep = sidecar::restore(&s, &mut q, &inp);
        assert!(rep.refused.is_empty(), "{id} {v:?}: {:?}", rep.refused);
        assert_eq!(
            q.row(&id).unwrap().value,
            p.row(&id).unwrap().value,
            "{id} {v:?}"
        );
        tested += 1;
    }
    println!("range ends round-tripped: {tested} of {total}; skipped {skipped:?}");
    skipped.sort();
    skipped.dedup();
    // The ends not round-tripped are listed exactly, so a new skip fails here: the controls not
    // on the one-pass (the memory and the playback speed).
    println!("skipped controls: {skipped:?}");
    assert_eq!(
        skipped,
        [
            "memory.capacity_s",
            "memory.max_recordings",
            "playback_speed"
        ],
        "the skipped range ends"
    );
    assert_eq!(tested + 6, total);
    // A non-default switch and choice round-trip too (coach 11).
    let mut p = Panel::new(rate).unwrap();
    set_switch(&mut p, "speech_codec_enabled", false);
    set_choice(&mut p, "vas.mode", "mute");
    let r = run(&request_for(&inp, &p, true));
    let mut q = Panel::new(rate).unwrap();
    let rep = sidecar::restore(&sidecar::read(&sidecar::write(&r)).unwrap(), &mut q, &inp);
    assert!(rep.refused.is_empty());
    for id in ["speech_codec_enabled", "vas.mode"] {
        assert_eq!(q.row(id).unwrap().value, p.row(id).unwrap().value, "{id}");
    }
    // −0.0 reads back as 0.0 (the engine stores it so).
    let mut p = Panel::new(rate).unwrap();
    set(
        &mut p,
        "mic.hiss_offset_db",
        ControlReading::Continuous(-0.0),
    );
    let r = run(&request_for(&inp, &p, true));
    let text = sidecar::write(&r);
    let line = text
        .lines()
        .find(|l| l.starts_with("control mic.hiss_offset_db "))
        .unwrap();
    assert_eq!(line, "control mic.hiss_offset_db 0 0x00000000");
}

fn sample_text() -> (String, workbench_core::input::Input) {
    let rate = 16000;
    let inp = input_of(rate, &voiced(rate, 0.6, 0.5));
    let p = Panel::new(rate).unwrap();
    let r = run(&request_for(&inp, &p, true));
    (sidecar::write(&r), inp)
}

#[test]
fn grammar() {
    let (text, _) = sample_text();
    assert!(sidecar::read(&text).is_ok());
    assert!(text.starts_with("rr-dr60-workbench sidecar 2\n"));
    // Version 1 is still read: drop the version-2 keys and change the magic line.
    let v1: String = text
        .replacen("sidecar 2", "sidecar 1", 1)
        .lines()
        .filter(|l| !l.starts_with("live ") && !l.starts_with("start ") && !l.starts_with("fed "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(sidecar::read(&v1).unwrap().version, 1, "v1_still_read");
    assert_eq!(
        sidecar::read("hello\n").unwrap_err(),
        SidecarError::NotASidecar
    );
    let v3 = text.replacen("sidecar 2", "sidecar 3", 1);
    assert_eq!(
        sidecar::read(&v3).unwrap_err(),
        SidecarError::UnsupportedVersion("3".into())
    );
    let dup = format!("{text}block 4096\n");
    assert!(matches!(
        sidecar::read(&dup),
        Err(SidecarError::Malformed(_, _))
    ));
    let unknown = format!("{text}colour blue\n");
    assert!(matches!(
        sidecar::read(&unknown),
        Err(SidecarError::Malformed(_, _))
    ));
    let dup_control = format!("{text}control seed 1\n");
    assert!(matches!(
        sidecar::read(&dup_control),
        Err(SidecarError::Malformed(_, _))
    ));
    // A decimal that disagrees with its bits.
    let bad = text.replace("control volume 0.13345174 ", "control volume 0.5 ");
    assert_ne!(bad, text, "the fixture has the default volume line");
    assert!(matches!(
        sidecar::read(&bad),
        Err(SidecarError::Malformed(_, _))
    ));
    let missing: String = text
        .lines()
        .filter(|l| !l.starts_with("engine.commit "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        sidecar::read(&missing).unwrap_err(),
        SidecarError::Missing("engine.commit")
    );
    // Comments and blank lines are ignored.
    let commented = text.replacen("\n", "\n# a note\n\n", 1);
    assert!(sidecar::read(&commented).is_ok());
}

#[test]
fn reproduces() {
    for rate in RATES {
        let x = voiced(rate, 1.5, 0.5);
        let inp = input_of(rate, &x);
        let mut p = Panel::new(rate).unwrap();
        set(&mut p, "vas.sensitivity", ControlReading::Integer(4));
        set(&mut p, "volume", ControlReading::Continuous(0.3));
        set_choice(&mut p, "insert.before_limiter", "band_filter");
        set_num(&mut p, "band_filter.frequency_hz", 1200.0);
        set(&mut p, "seed", ControlReading::Integer(0xDEAD_BEEF));
        let r = run(&request_for(&inp, &p, true));
        let text = sidecar::write(&r);
        // A fresh session: open the same input, restore, render.
        let inp2 = input_of(rate, &x);
        let mut q = Panel::new(rate).unwrap();
        let rep = sidecar::restore(&sidecar::read(&text).unwrap(), &mut q, &inp2);
        assert!(
            rep.refused.is_empty() && rep.unknown_ids.is_empty() && !rep.input_differs,
            "{rate}: {rep:?}"
        );
        assert!(
            rep.missing_controls.is_empty(),
            "{rate}: every control is written"
        );
        assert!(rep.commit_differs.is_none());
        let r2 = run(&request_for(&inp2, &q, rep.tail));
        assert_eq!(&r2.samples[..], &r.samples[..], "{rate}: bit for bit");
        assert_eq!(r2.output_sha256, r.output_sha256);
    }
}

#[test]
fn differences_listed() {
    let (text, inp) = sample_text();
    let other_commit = text
        .lines()
        .map(|l| {
            if l.starts_with("engine.commit ") {
                "engine.commit 0000000000000000000000000000000000000000".to_owned()
            } else {
                l.to_owned()
            }
        })
        .filter(|l| !l.starts_with("control vas.hang_ms "))
        .chain([
            "control future.knob 3".to_owned(),
            "control vas.sensitivity 9".to_owned(),
        ])
        .filter(|l| l != "control vas.sensitivity 3")
        .map(|l| format!("{l}\n"))
        .collect::<String>()
        .replace("control host_rate_hz 16000", "control host_rate_hz 48000");
    let s = sidecar::read(&other_commit).unwrap();
    let mut p = Panel::new(16000).unwrap();
    let rep = sidecar::restore(&s, &mut p, &inp);
    assert!(rep.commit_differs.is_some());
    assert_eq!(rep.unknown_ids, vec!["future.knob".to_owned()]);
    assert_eq!(rep.missing_controls, vec!["vas.hang_ms".to_owned()]);
    assert_eq!(rep.refused.len(), 1, "{:?}", rep.refused);
    assert_eq!(rep.refused[0].0, "vas.sensitivity");
    assert_eq!(
        p.row("vas.sensitivity").unwrap().value,
        ControlReading::Integer(3),
        "left at its default"
    );
    assert_eq!(rep.rate_differs.as_deref(), Some("48000"));
    assert!(p.renderable());
    // A different input.
    let other = input_of(16000, &tone(16000, 0.3, 0.4));
    let rep = sidecar::restore(
        &sidecar::read(&text).unwrap(),
        &mut Panel::new(16000).unwrap(),
        &other,
    );
    assert!(rep.input_differs);
}
