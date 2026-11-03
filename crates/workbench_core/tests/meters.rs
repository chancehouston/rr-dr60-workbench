//! Spec 002 [TR: FR-001 – FR-004, SC-001, US1 AS1 – AS3, AS6]: the meter timeline.

mod support;

use rr_dr60::{Pipeline, VasEvent};
use support::*;
use workbench_core::meters::{self, FIELDS};

/// The engine's read-outs for the same calls, read directly.
fn direct_rows(req: &workbench_core::render::RenderRequest, fed: &[f32]) -> Vec<rr_dr60::Meters> {
    let mut p = Pipeline::new(req.settings).unwrap();
    for &(i, v) in &req.engine_values {
        p.set_control_value(i, v).unwrap();
    }
    let mut rows = vec![p.rest_meters()];
    let mut out = vec![0.0f32; req.block];
    let mut ev = vec![VasEvent::new(0, 0); p.max_events(req.block).max(1)];
    for c in fed.chunks(req.block) {
        let _ = p
            .process_with_events(c, &mut out[..c.len()], &mut ev)
            .unwrap();
        rows.push(p.meters());
    }
    rows
}

#[test]
fn meters_timeline_equals_engine() {
    for rate in RATES {
        let x = voiced(rate, 1.5, 0.8);
        let req = request(rate, &x, |p| set_num(p, "vas.hang_ms", 50.0));
        assert_eq!(req.block, rate as usize / 100, "10 ms calls");
        let r = run(&req);
        let fed = with_tail(&x, r.tail_samples);
        let want = direct_rows(&req, &fed);
        let got: Vec<rr_dr60::Meters> = r.meters.iter().map(|m| m.meters).collect();
        assert_eq!(
            got, want,
            "{rate} Hz: the timeline equals the engine's read-outs"
        );
        assert_eq!(r.meters[0].in_span, (0, 0));
        assert_eq!(r.meters.last().unwrap().in_span.1, fed.len() as u64);
        assert_eq!(r.meters.last().unwrap().out_span.1, r.samples.len() as u64);
        for w in r.meters.windows(2).skip(1) {
            assert_eq!(w[0].in_span.1, w[1].in_span.0, "contiguous input spans");
            assert_eq!(w[0].out_span.1, w[1].out_span.0, "contiguous output spans");
        }
        assert!(r.meters.iter().any(|m| m.tail), "the tail calls are marked");
        assert_ne!(
            r.meters[0].meters.battery_ceiling, 0.0,
            "the rest point is the engine's, not Default"
        );
        assert_eq!(
            r.meters[0].meters,
            workbench_core::render::fresh_engine(&req)
                .unwrap()
                .rest_meters()
        );
        assert!(
            r.meters
                .iter()
                .filter(|m| m.tail)
                .all(|m| m.in_span.0 >= x.len() as u64),
            "a tail row starts at or after the input's end"
        );
    }
}

#[test]
fn meter_fields_named() {
    let m = Pipeline::new(rr_dr60::Settings::new(48000))
        .unwrap()
        .rest_meters();
    let names = meters::engine_field_names(&m);
    let ours: Vec<&str> = FIELDS.iter().map(|f| f.id).collect();
    assert_eq!(
        names, ours,
        "the engine's meter fields, in order, against the view's table"
    );
}

#[test]
fn meters_limiter_burst() {
    let rate = 48000;
    // A 1 kHz burst starting on its peak at +9 dBFS (amplitude 2.818), then quiet.
    let amp = 10f32.powf(9.0 / 20.0);
    let mut x: Vec<f32> = (0..rate as usize / 2)
        .map(|i| {
            amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32
                + std::f32::consts::FRAC_PI_2)
                .sin()
        })
        .collect();
    x.extend(vec![0.0; rate as usize / 2]);
    let r = run(&request(rate, &x, |_| {}));
    let gr = r
        .meters
        .iter()
        .map(|m| m.meters.gain_reduction_db)
        .fold(0.0f32, f32::max);
    let hold = r
        .meters
        .iter()
        .map(|m| m.meters.hold_charge)
        .fold(0.0f32, f32::max);
    println!("burst: deepest gain reduction {gr:.2} dB, highest hold {hold:.3}");
    assert!(gr > 5.0, "gain reduction {gr} dB");
    assert!(hold > 0.73, "hold {hold}");
}

/// A stimulus that drives every field (plan review B5): a 0.5 s +12 dBFS 1 kHz burst, a 0.5 s
/// ±1.0 square at 250 Hz, then 3 s of silence (longer than the 1 s default hang).
fn drive(rate: u32) -> Vec<f32> {
    let amp = 10f32.powf(12.0 / 20.0);
    let mut x: Vec<f32> = (0..rate as usize / 2)
        .map(|i| amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate as f32).sin())
        .collect();
    let half = (rate / 500) as usize;
    x.extend((0..rate as usize / 2).map(|i| {
        if (i / half).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        }
    }));
    x.extend(vec![0.0; 3 * rate as usize]);
    x
}

#[test]
fn meter_points_on_clock() {
    let rate = 16000;
    let b = voiced(rate, 0.5, 0.8);
    let gap = vec![0.0f32; 3 * rate as usize];
    let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
    let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
    let marks = workbench_core::ab::marks(&r);
    assert!(!marks.is_empty());
    let route = FIELDS.iter().find(|f| f.id == "route_peak").unwrap();
    for row in r.meters.iter().skip(1) {
        if let Some((s, e)) = meters::placed_span(row, route, &r) {
            for m in &marks {
                let (a, z) = m.input_span;
                let straddles =
                    row.out_span.0 < m.output_position && row.out_span.1 > m.output_position;
                assert!(
                    straddles || e <= a || s >= z,
                    "an output-side span {s}..{e} covers the removed pause {a}..{z}"
                );
            }
        } else {
            assert_eq!(
                row.out_span.0, row.out_span.1,
                "only empty calls have no place"
            );
        }
    }
    let pts = meters::plot_points(route, &r);
    assert_eq!(
        pts[0],
        [0.0, f64::from(r.meters[0].meters.route_peak)],
        "the rest point first"
    );
    assert!(pts.iter().any(|p| p[1].is_nan()), "a break at a pause");
    let hang = FIELDS
        .iter()
        .find(|f| f.id == "hang_remaining_samples")
        .unwrap();
    assert!(meters::display(hang, &r.meters[1], rate).contains(" ms)"));
    let clamp = FIELDS.iter().find(|f| f.id == "adc_clamp_count").unwrap();
    assert!(meters::display(clamp, &r.meters[1], rate).contains("% of the call's samples"));
    let peak = FIELDS.iter().find(|f| f.id == "input_peak").unwrap();
    assert!(meters::display(peak, &r.meters[1], rate).contains("dBFS"));
}

#[test]
fn meter_reach_pinned() {
    for rate in [6000, 16000, 48000] {
        reach_at(rate);
    }
}

fn reach_at(rate: u32) {
    let x = drive(rate);
    let e = rr_dr60::controls::control("tap").unwrap();
    let rr_dr60::controls::ControlValue::Choice { options, .. } = e.value else {
        panic!()
    };
    let mut checked = 0;
    for o in options {
        // (route, agc, clamps, volts, volume amp, gate, playback stage, record stage)
        for (route, agc, clamp, volts, amp, gate, play, rec) in [
            ("speaker", true, true, 3.0f32, true, true, true, true),
            ("earphone", true, true, 3.0, true, true, true, true),
            ("speaker", false, false, 2.4, true, true, true, true),
            ("speaker", false, true, 3.0, true, true, true, true),
            ("speaker", true, true, 3.0, false, true, true, true),
            ("speaker", true, true, 3.0, true, false, true, true),
            ("speaker", true, true, 3.0, true, true, false, false),
        ] {
            let tap = o.identifier;
            if (tap == "after_speaker" && route == "earphone")
                || (tap == "after_earphone" && route == "speaker")
            {
                continue;
            }
            let req = request(rate, &x, |p| {
                set_choice(p, "route", route);
                set_choice(p, "tap", tap);
                set_switch(p, "agc.enabled", agc);
                set_switch(p, "output.clamp_enabled", clamp);
                set_switch(p, "adc_clamp_enabled", clamp);
                set(
                    p,
                    "volume",
                    rr_dr60::control_map::ControlReading::Continuous(1.0),
                );
                set(
                    p,
                    "battery_volts",
                    rr_dr60::control_map::ControlReading::Continuous(volts),
                );
            });
            let r = run(&req);
            let rest = r.meters[0].meters;
            let rt = meters::route_of(&req.engine_values);
            let factor = rr_dr60::battery::ceiling_factor(volts) as f32;
            for f in &FIELDS {
                let moved = r
                    .meters
                    .iter()
                    .any(|m| (f.get)(&m.meters) != (f.get)(&rest));
                let want = meters::reached(f, &req.settings, rt);
                let tag = format!(
                    "{rate} {tap} {route} agc={agc} clamp={clamp} amp={amp} gate={gate} play={play} rec={rec}: {}",
                    f.id
                );
                if f.id == "battery_ceiling" {
                    // A state of a setting (plan review B1): every row reads the factor in force.
                    assert!(
                        r.meters.iter().all(|m| m.meters.battery_ceiling == factor),
                        "{tag}"
                    );
                } else if !want {
                    assert!(!moved, "{tag}: predicted unreached but moved");
                } else if !moved {
                    // The one reached field this stimulus cannot move, with its measured reason:
                    // the output clamp on the earphone route, whose route peak stays at or under
                    // the clamp's bound.
                    let peak = r
                        .meters
                        .iter()
                        .map(|m| m.meters.route_peak)
                        .fold(0.0f32, f32::max);
                    assert!(
                        f.id == "output_clamp_count" && route == "earphone" && peak <= 1.0,
                        "{tag}: reached but never moved (route peak {peak})"
                    );
                    println!("{tag}: reached; the route peak {peak:.3} never exceeds the bound");
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 900, "{checked}");
}

#[test]
fn block_independent_restore() {
    // The output does not depend on the block: digests at 4096 (001's block) and at the 10 ms
    // block agree at seven rates, and a block-4096 sidecar restores to the same digest.
    for rate in RATES {
        let inp = input_of(rate, &voiced(rate, 1.0, 0.5));
        let p = workbench_core::panel::Panel::new(rate).unwrap();
        let mut req = request_for(&inp, &p, true);
        assert_eq!(req.block, rate as usize / 100);
        let r10 = run(&req);
        req.block = 4096;
        let r4096 = run(&req);
        assert_eq!(
            r10.output_sha256, r4096.output_sha256,
            "{rate}: block-independent"
        );
        let text = workbench_core::sidecar::write(&r4096);
        assert!(text.contains("\nblock 4096\n"));
        let mut q = workbench_core::panel::Panel::new(rate).unwrap();
        let rep = workbench_core::sidecar::restore(
            &workbench_core::sidecar::read(&text).unwrap(),
            &mut q,
            &inp,
        );
        let r = run(&request_for(&inp, &q, rep.tail));
        assert_eq!(
            r.output_sha256, r4096.output_sha256,
            "{rate}: a 4096 sidecar reproduces"
        );
    }
}

#[test]
fn empty_render_rows() {
    let rate = 8000;
    let inp = workbench_core::input::open_bytes(
        "e.wav",
        wav_i16(rate, &[]),
        workbench_core::input::Channel::Index(0),
    )
    .unwrap();
    let p = workbench_core::panel::Panel::new(rate).unwrap();
    let mut req = request_for(&inp, &p, false);
    req.tail = false;
    let r = run(&req);
    assert!(r.samples.is_empty());
    assert_eq!(r.meters.len(), 1, "the rest point only");
}
