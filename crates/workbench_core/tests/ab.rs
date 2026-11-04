//! Spec 001 T011 [TR: FR-010, SC-005]: the A/B map and its inverse.

mod support;

use rr_dr60::VasEvent;
use support::*;
use workbench_core::ab::{self, AbMode};

#[test]
fn hand_built_exact_values() {
    // Latency 10; a splice at output 50 that removed 30 input samples.
    let ev = [VasEvent::new(50, 30)];
    let f = |i| ab::to_render_raw(i, 10, 0, &ev, true, 200);
    assert_eq!(f(0), 10, "input 0 maps to the latency");
    assert_eq!(f(39), 49);
    // Inputs 40..=69 were removed: they land on the splice (output 50).
    assert_eq!(f(40), 50);
    assert_eq!(f(69), 50);
    assert_eq!(f(70), 50);
    assert_eq!(f(71), 51);
    // Past the end clamps to the render's end.
    assert_eq!(f(10_000), 200);
    // Mute mode: o − L.
    assert_eq!(ab::to_render_raw(5, 10, 0, &ev, false, 200), 15);
}

#[test]
fn round_trips_at_every_rate() {
    for rate in RATES {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
        for mode in ["drop", "mute"] {
            let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", mode)));
            let n_in = x.len() as u64;
            let n_out = r.samples.len() as u64;
            for i in (0..n_in).step_by(97) {
                let o = ab::to_render(i, &r, AbMode::EngineMap);
                assert!(o <= n_out);
                if o < n_out {
                    assert!(
                        ab::to_original(o, &r, AbMode::EngineMap, n_in) >= i,
                        "{rate} {mode} i={i}"
                    );
                }
            }
            for o in (0..n_out).step_by(97) {
                // Outputs before the latency align with "before input 0" (the engine clamps them).
                let (_, clamped) = workbench_core::render::output_to_input(o, &r);
                if clamped {
                    continue;
                }
                let i = ab::to_original(o, &r, AbMode::EngineMap, n_in);
                assert!(
                    ab::to_render(i, &r, AbMode::EngineMap) <= o,
                    "{rate} {mode} o={o}"
                );
            }
            // Same seconds: the identity, clamped.
            assert_eq!(ab::to_render(123, &r, AbMode::SameSeconds), 123);
            assert_eq!(
                ab::to_render(n_in + 5, &r, AbMode::SameSeconds),
                n_out.min(n_in + 5)
            );
            assert_eq!(
                ab::to_original(n_out + 5, &r, AbMode::SameSeconds, n_in),
                n_in.min(n_out + 5)
            );
        }
    }
}

#[test]
fn the_map_is_exact() {
    // SC-005 "exactly": every input outside a removed pause maps to the output the engine's map
    // takes back to it; a removed input maps to its splice's output position.
    for rate in [6000, 48000] {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
        let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
        assert!(!r.events.is_empty());
        let n_out = r.samples.len() as u64;
        // The reports' clock (engine 0.95, spec 028): a splice is heard at its position plus the
        // heard shift H (the coder's delay plus the playback half's latency).
        let c = u64::from(r.splice_shift_samples);
        let l = u64::from(r.latency_samples) - c;
        // The removed input ranges, in input positions.
        let mut removed = Vec::new();
        let mut before = 0u64;
        for e in &r.events {
            let start = e.output_position + before - l;
            removed.push((start, start + e.input_length, e.output_position + c));
            before += e.input_length;
        }
        for i in (0..x.len() as u64).step_by(31) {
            let o = ab::to_render(i, &r, AbMode::EngineMap);
            if o >= n_out {
                continue; // a final pause
            }
            if let Some(&(_, _, splice)) = removed.iter().find(|(s, e, _)| (*s..*e).contains(&i)) {
                assert_eq!(o, splice, "{rate}: removed input {i} lands on its splice");
            } else {
                let (back, _) = workbench_core::render::output_to_input(o, &r);
                assert_eq!(back, i, "{rate}: input {i} → output {o} → input {back}");
            }
        }
    }
}

#[test]
fn final_pause_clamps_to_the_end() {
    let rate = 8000;
    let mut x = tone(rate, 0.3, 0.5);
    x.resize(x.len() + 2 * rate as usize, 0.0);
    let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
    assert!(r.paused_at_end);
    let last = x.len() as u64 - 1;
    assert_eq!(
        ab::to_render(last, &r, AbMode::EngineMap),
        r.samples.len() as u64
    );
}

/// Spec 008 D6 (the 005b follow-up): with the coder on in drop mode, the outputs just before a
/// splice reaches the output are the hang's silence; the reports'-clock map takes them to silent
/// input, while the full-latency map counted the splice the coder's delay early and took them
/// into the loud content after the removed pause.
#[test]
fn the_map_counts_a_splice_at_the_output() {
    for rate in [8000, 48000] {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap, b].concat();
        let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
        let c = u64::from(r.splice_shift_samples);
        assert!(
            r.codec_delay_samples > 0,
            "{rate}: the coder runs by default"
        );
        let e = r.events[0];
        let at_output = e.output_position + c;
        let (mut new_loud, mut old_loud) = (0, 0);
        for o in at_output - c / 2..at_output {
            let (i, _) = workbench_core::render::output_to_input(o, &r);
            let (j, _) =
                rr_dr60::timeline::onepass_output_to_input(o, r.latency_samples, &r.events, true);
            new_loud += usize::from(x.get(i as usize).is_some_and(|v| v.abs() > 0.0));
            old_loud += usize::from(x.get(j as usize).is_some_and(|v| v.abs() > 0.0));
        }
        assert_eq!(new_loud, 0, "{rate}: the hang's outputs map to the silence");
        assert!(
            old_loud > 0,
            "{rate}: the full-latency map lands in the content"
        );
    }
}

/// The RMS of `y[lo..hi)` in dBFS (−200 for silence).
fn level_db(y: &[f32], lo: u64, hi: u64) -> f64 {
    let s = &y[lo as usize..hi.min(y.len() as u64) as usize];
    let p = s.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>() / s.len().max(1) as f64;
    if p > 0.0 { 10.0 * p.log10() } else { -200.0 }
}

/// The first output sample at or after `from` whose magnitude exceeds −45 dBFS (above the
/// hang's hiss, which reads about −72 dBFS RMS), or `None` within `to`.
fn onset(y: &[f32], from: u64, to: u64) -> Option<u64> {
    let t = 10f32.powf(-45.0 / 20.0);
    (from..to.min(y.len() as u64)).find(|&i| y[i as usize].abs() > t)
}

/// W1 (the independent review of 008): an oracle on the output itself, not on the map's own
/// arithmetic. With the coder on in drop mode, the gap's splice mark sits on the output's onset
/// (the hang's silence, then the next burst): the first sample above −45 dBFS lies within C/4 of
/// the mark (the voice's attack delays it by up to 0.17 C), and the old map's position (the mark without the coder's delay C) is a whole C away.
#[test]
fn the_marks_sit_on_the_output_step() {
    for rate in [6000, 44_100, 48_000] {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap, b].concat();
        let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "drop")));
        let c = u64::from(r.codec_delay_samples);
        assert_eq!(c, 180 * u64::from(rate) / 6000, "{rate}: the coder runs");
        let marks = ab::marks(&r);
        let splice = marks
            .iter()
            .find(|m| m.input_span.1 - m.input_span.0 > u64::from(rate))
            .expect("the gap's splice");
        let m = splice.output_position;
        // The burst's onset in the output, searched from a whole C before the mark.
        let s = onset(&r.samples, m - c, m + c).expect("the next burst reaches the output");
        // Within C/4 (the voice's own attack delays the first loud sample by up to 0.17 C).
        assert!(
            s.abs_diff(m) <= c / 4,
            "{rate}: the onset {s} within C/4 of the mark {m}"
        );
        assert!(
            s.abs_diff(m - c) > c / 2,
            "{rate}: the old map's position {} is not",
            m - c
        );
        assert!(
            level_db(&r.samples, m - c / 2, m - c / 6) < -60.0,
            "{rate}: silence before"
        );
    }
}

/// W2: with the coder on in mute mode, a muted region keeps its length, so its end in the
/// output is its mark plus its input length: the next burst's onset lies within C/4 of it, and
/// the old map's end (C earlier) does not. (The region's start is the hang's hiss turning into
/// the decoded muted zeros, a step too gradual through the coder to read sharply.)
#[test]
fn the_mute_marks_sit_on_the_output_step() {
    for rate in [6000, 44_100, 48_000] {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap, b].concat();
        let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", "mute")));
        let c = u64::from(r.codec_delay_samples);
        let marks = ab::marks(&r);
        let region = marks
            .iter()
            .find(|m| m.input_span.1 - m.input_span.0 > u64::from(rate))
            .expect("the gap's muted region");
        let end = region.output_position + (region.input_span.1 - region.input_span.0);
        let s = onset(&r.samples, end - c, end + c).expect("the next burst reaches the output");
        assert!(
            s.abs_diff(end) <= c / 4,
            "{rate}: the onset {s} within C/4 of the region's end {end}"
        );
        assert!(
            s.abs_diff(end - c) > c / 2,
            "{rate}: the old map's end {} is not",
            end - c
        );
    }
}

/// Engine 0.95 (spec 028, workbench #27): the workbench's per-frame map is the engine's one-pass
/// output map. On every output of drop- and mute-mode renders, coder on and off, at 6, 8 and
/// 48 kHz, `render::output_to_input` equals `one_pass_input_at_output`'s centre, each
/// mark sits on `one_pass_splice_output_position`, and the heard shift is the coder's delay plus
/// the playback half's latency (0 at 6 kHz).
#[test]
fn the_map_is_the_engines() {
    for rate in [6000, 8000, 48_000] {
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 2 * rate as usize];
        let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
        for mode in ["drop", "mute"] {
            for codec in [true, false] {
                let r = run(&request(rate, &x, |p| {
                    set_choice(p, "vas.mode", mode);
                    set_switch(p, "speech_codec_enabled", codec);
                }));
                assert!(!r.events.is_empty(), "{rate} {mode} {codec}");
                let p = workbench_core::render::fresh_engine(&r.request).unwrap();
                let lpost = if rate == 6000 {
                    0
                } else {
                    p.playback_latency_samples()
                };
                assert_eq!(
                    r.splice_shift_samples,
                    r.codec_delay_samples + lpost,
                    "{rate} {mode} {codec}"
                );
                let mut differs_from_coder_only = 0u32;
                for o in 0..r.samples.len() as u64 {
                    let a = p.one_pass_input_at_output(o, &r.events);
                    // The centre (the engine's `clamped` is its window's flag at the stream's
                    // start, a different reading from the earlier map's).
                    let (i, _) = workbench_core::render::output_to_input(o, &r);
                    assert_eq!(i, a.centre, "{rate} {mode} {codec} {o}");
                    let c = u64::from(r.codec_delay_samples);
                    let old = if o < c {
                        0
                    } else {
                        rr_dr60::timeline::onepass_output_to_input(
                            o - c,
                            r.latency_samples - r.codec_delay_samples,
                            &r.events,
                            r.drop_mode,
                        )
                        .0
                    };
                    differs_from_coder_only += u32::from(old != i);
                }
                let marks = ab::marks(&r);
                for (m, e) in marks.iter().zip(&r.events) {
                    assert_eq!(m.output_position, p.one_pass_splice_output_position(e));
                }
                // The old coder-only rule was off on L_post outputs per splice in drop mode.
                let want = if mode == "drop" {
                    r.events.len() as u32 * lpost
                } else {
                    0
                };
                println!(
                    "{rate} {mode} codec {codec}: coder-only rule off on {differs_from_coder_only} outputs"
                );
                assert_eq!(differs_from_coder_only, want, "{rate} {mode} {codec}");
            }
        }
    }
}
