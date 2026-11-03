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
                for &v in &x[s as usize..end as usize] {
                    assert!(lo <= v && v <= hi, "cols {cols} range {range:?} column {i}");
                }
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
