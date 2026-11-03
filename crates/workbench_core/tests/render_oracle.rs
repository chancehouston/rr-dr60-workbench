//! Spec 001 T008 [TR: FR-007, SC-001, US1 AS1]: every render equals the engine's direct output
//! for the same calls, in one whole-input call and at block size 1, at seven rates, on three
//! stimuli, in drop and mute mode, with the gate actually pausing (hang 50 ms; coach 5). And a
//! panel-free oracle with non-default live and slot values (hardware review B1).

mod support;

use rr_dr60::components::{ComponentId, InsertionPointId};
use rr_dr60::control_map::ControlReading;
use rr_dr60::{Pipeline, Route, Settings, VasEvent, VasMode};
use support::*;

fn stimuli(rate: u32) -> Vec<(&'static str, Vec<f32>)> {
    vec![
        (
            "tone",
            [
                tone(rate, 0.3, 0.5),
                vec![0.0; rate as usize / 2],
                tone(rate, 0.3, 0.5),
            ]
            .concat(),
        ),
        (
            "noise",
            [
                noise(rate, 0.3, 0.3),
                vec![0.0; rate as usize / 2],
                noise(rate, 0.3, 0.3),
            ]
            .concat(),
        ),
        ("voiced", voiced(rate, 1.5, 0.5)),
    ]
}

#[test]
fn render_equals_the_oracles() {
    for rate in RATES {
        for (name, x) in stimuli(rate) {
            for mode in ["drop", "mute"] {
                let req = request(rate, &x, |p| {
                    set_choice(p, "vas.mode", mode);
                    set_num(p, "vas.hang_ms", 50.0);
                });
                let r = run(&req);
                assert!(
                    !r.events.is_empty(),
                    "{rate} Hz {name} {mode}: the gate must pause"
                );
                if mode == "drop" {
                    assert!(r.not_emitted > 0, "{rate} Hz {name}: drop mode removes");
                }
                let fed = with_tail(&x, r.tail_samples);
                for block in [0usize, 1, 4096] {
                    let (o, ev) = oracle(req.settings, &req.engine_values, &fed, block);
                    assert_eq!(
                        &r.samples[..],
                        &o[..],
                        "{rate} Hz {name} {mode} block {block}"
                    );
                    assert_eq!(
                        r.events, ev,
                        "{rate} Hz {name} {mode} block {block}: reports"
                    );
                }
            }
        }
    }
}

/// The engine built by name, not from the panel: settings fields assigned directly and the
/// plain setters for the live controls and the slot.
fn direct(rate: u32, input: &[f32], block: usize) -> Vec<f32> {
    let mut s = Settings::new(rate);
    s.vas.sensitivity = 4;
    s.vas.mode = VasMode::Mute;
    s.tap = rr_dr60::Tap::RouteOutput;
    let mut p = Pipeline::new(s).unwrap();
    p.set_route(Route::Earphone);
    p.set_volume(0.3).unwrap();
    p.set_battery_volts(2.4).unwrap();
    p.set_component(InsertionPointId::BeforeLimiter, ComponentId::BandFilter)
        .unwrap();
    p.set_component_parameter(InsertionPointId::BeforeLimiter, 1, 800.0)
        .unwrap();
    p.set_component_parameter(InsertionPointId::BeforeLimiter, 2, 3.0)
        .unwrap();
    let block = if block == 0 {
        input.len().max(1)
    } else {
        block
    };
    let mut out = Vec::new();
    let mut buf = vec![0.0f32; block];
    let mut ev = vec![VasEvent::new(0, 0); p.max_events(block).max(1)];
    for c in input.chunks(block) {
        let o = &mut buf[..c.len()];
        let info = p.process_with_events(c, o, &mut ev).unwrap();
        out.extend_from_slice(&o[..info.produced]);
    }
    out
}

#[test]
fn panel_free_oracle_with_live_values() {
    for rate in [6000, 44100, 48000] {
        let x = voiced(rate, 1.0, 0.5);
        let req = request(rate, &x, |p| {
            set(p, "vas.sensitivity", ControlReading::Integer(4));
            set_choice(p, "vas.mode", "mute");
            set_choice(p, "route", "earphone");
            set(p, "volume", ControlReading::Continuous(0.3));
            set(p, "battery_volts", ControlReading::Continuous(2.4));
            set_choice(p, "insert.before_limiter", "band_filter");
            set_num(p, "band_filter.frequency_hz", 800.0);
            set_num(p, "band_filter.q", 3.0);
        });
        let r = run(&req);
        let fed = with_tail(&x, r.tail_samples);
        for block in [0usize, 1] {
            let o = direct(rate, &fed, block);
            assert_eq!(
                &r.samples[..],
                &o[..],
                "{rate} Hz block {block}: panel → engine translation"
            );
        }
    }
}
