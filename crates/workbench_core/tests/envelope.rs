//! Spec 002 [TR: FR-005, US1 AS4, AS5]: envelope bounds and the alignment marks.

mod support;

use support::*;
use workbench_core::analysis::envelope::Envelope;

#[test]
fn envelope_bounds() {
    let x = noise(48000, 3.0, 0.7);
    let e = Envelope::new(&x);
    for cols in [1usize, 7, 300, 1920, 50_000] {
        for range in [(0u64, x.len() as u64), (12_345, 98_765), (100, 117)] {
            let c = e.columns(&x, range, cols);
            assert_eq!(c.len(), cols);
            let span = (range.1 - range.0) as f64 / cols as f64;
            for (i, &(lo, hi)) in c.iter().enumerate() {
                let s = range.0 + (i as f64 * span) as u64;
                let end = (range.0 + ((i + 1) as f64 * span) as u64)
                    .min(range.1)
                    .max(s + 1);
                let span = &x[s as usize..end as usize];
                let mn = span.iter().cloned().fold(f32::INFINITY, f32::min);
                let mx = span.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                assert_eq!(
                    (lo, hi),
                    (mn, mx),
                    "cols {cols} range {range:?} column {i}: exact"
                );
            }
        }
    }
}

#[test]
fn waveform_alignment_marks() {
    for (mode, removed) in [("drop", true), ("mute", false)] {
        let rate = 8000;
        let b = voiced(rate, 0.4, 0.5);
        let gap = vec![0.0f32; 3 * rate as usize];
        let x = [b.clone(), gap.clone(), b.clone(), gap, b].concat();
        let r = run(&request(rate, &x, |p| set_choice(p, "vas.mode", mode)));
        let marks = workbench_core::ab::marks(&r);
        assert_eq!(marks.len(), r.events.len());
        assert!(!marks.is_empty(), "{mode}");
        for m in &marks {
            assert_eq!(m.removed, removed);
            assert!(m.input_span.0 < m.input_span.1);
        }
    }
}

#[test]
fn analysis_memory_below_input() {
    let x = noise(48000, 10.0, 0.5);
    let e = Envelope::new(&x);
    let bytes = e.bytes();
    println!("envelope {bytes} bytes for {} of samples", 4 * x.len());
    assert!(
        bytes * 4 < 4 * x.len(),
        "the envelope stays well below one f32 copy"
    );
}

#[test]
fn kept_spans_are_the_emitted_samples() {
    // An oracle at the 6 kHz identity rate (post-implementation review B2): with the coder off,
    // at the gate's tap and every earlier stage bypassed, the render is the kept input samples,
    // so each render run equals the original over its kept span, bit for bit, in drop and mute.
    let rate = 6000;
    let b = voiced(rate, 0.4, 0.5);
    let gap = vec![0.0f32; 3 * rate as usize];
    let x = [b.clone(), gap.clone(), b.clone(), gap, b.clone()].concat();
    for mode in ["drop", "mute"] {
        let r = run(&request(rate, &x, |p| {
            set_switch(p, "speech_codec_enabled", false);
            set_switch(p, "mic.capsule_enabled", false);
            set_switch(p, "mic.amp_enabled", false);
            set_switch(p, "agc.enabled", false);
            set_switch(p, "record_stage_enabled", false);
            set_switch(p, "adc_clamp_enabled", false);
            set_choice(p, "tap", "after_vas");
            set_choice(p, "vas.mode", mode);
        }));
        assert!(!r.events.is_empty(), "{mode}");
        let runs = workbench_core::analysis::spectrum::render_runs((0, r.samples.len() as u64), &r);
        let mut checked = 0;
        for &(o0, o1) in &runs {
            let (i0, _) = rr_dr60::timeline::onepass_output_to_input(
                o0,
                r.latency_samples,
                &r.events,
                r.drop_mode,
            );
            let n = (o1 - o0) as usize;
            if (i0 as usize) + n > x.len() {
                continue; // the tail
            }
            assert_eq!(
                &r.samples[o0 as usize..o1 as usize],
                &x[i0 as usize..i0 as usize + n],
                "{mode}: run {o0}..{o1}"
            );
            checked += n;
        }
        assert!(checked > x.len() / 4, "{mode}: {checked}");
        // Every mark lies inside a gap of the stimulus, extended at its end by the gate's onset
        // (the onset consumes the next burst's start; the engine's spec 020 lesson), with one
        // device sample of slack.
        let onset = (r.request.settings.vas.onset_ms * 6.0).round() as u64;
        let starts = [b.len() as u64, (2 * b.len()) as u64 + 3 * u64::from(rate)];
        for m in workbench_core::ab::marks(&r) {
            let inside = starts.iter().any(|&g| {
                m.input_span.0 + 1 >= g && m.input_span.1 <= g + 3 * u64::from(rate) + onset + 1
            });
            assert!(
                inside,
                "{mode}: mark {:?} outside the gaps (onset {onset})",
                m.input_span
            );
        }
    }
}
