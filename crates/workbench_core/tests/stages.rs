//! Spec 002 [TR: FR-009, SC-005, US3 AS2]: the Stages view's text is the engine's.

use workbench_core::stages;

#[test]
fn stages_verbatim() {
    let m = rr_dr60::manifest();
    assert_eq!(stages::stages().len(), m.stages().len());
    for (e, s) in m.stages().iter().zip(stages::stages()) {
        let f = stages::stage_fields(s);
        let get = |k: &str| f.iter().find(|(l, _)| *l == k).unwrap().1.clone();
        assert_eq!(get("name"), e.name);
        assert_eq!(get("identifier"), e.identifier);
        assert_eq!(get("description"), e.description);
        assert_eq!(get("not modeled here"), e.coverage);
        assert_eq!(get("controls"), e.controls);
        assert_eq!(get("model version"), e.model_version);
        assert_eq!(get("sources"), e.sources.join(", "));
        assert_eq!(get("assumptions"), e.assumptions.join(", "));
        // Spec 008 D7: the engine's labels, never the enum's Debug name.
        assert_eq!(get("status"), e.status.label());
        assert_eq!(get("path"), e.path.label());
        assert_eq!(get("confidence"), e.confidence.label());
        for c in stages::controls_of(e.number) {
            assert_eq!(c.stage, e.number);
        }
    }
    let h = stages::header_lines();
    assert!(
        h.iter()
            .any(|(l, v)| *l == "basis" && v == m.header().basis)
    );
    assert!(
        h.iter()
            .any(|(l, v)| *l == "library version" && v == m.header().library_version)
    );
    // Every control is under its stage or "no stage".
    let total: usize = (0..=15u8).map(|n| stages::controls_of(n).len()).sum();
    assert_eq!(total, rr_dr60::controls::controls().len());
}
