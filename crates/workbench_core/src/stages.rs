//! The stage panels (spec 002 D9; research R-08): the engine's stage manifest, verbatim.

use rr_dr60::controls::ControlEntry;

/// The manifest's header lines (label, value).
pub fn header_lines() -> Vec<(&'static str, String)> {
    let h = rr_dr60::manifest().header();
    vec![
        ("library version", h.library_version.to_owned()),
        ("device rate", format!("{} Hz", h.device_rate_hz)),
        ("stages", h.stage_count.to_string()),
        (
            "status counts",
            format!(
                "{} not modeled, {} assumed, {} sourced, {} checked against a real unit",
                h.counts.not_modeled, h.counts.assumed, h.counts.sourced, h.counts.checked
            ),
        ),
        ("basis", h.basis.to_owned()),
        ("evidence", h.evidence_link.to_owned()),
        ("assumptions", h.assumptions_link.to_owned()),
        ("sources", h.sources_link.to_owned()),
    ]
}

/// The manifest's stages.
pub fn stages() -> &'static [rr_dr60::StageEntry] {
    rr_dr60::manifest().stages()
}

/// A stage's fields (label, the engine's text), in the manifest's order.
pub fn stage_fields(e: &rr_dr60::StageEntry) -> Vec<(&'static str, String)> {
    vec![
        ("number", e.number.to_string()),
        ("identifier", e.identifier.to_owned()),
        ("name", e.name.to_owned()),
        ("status", format!("{:?}", e.status)),
        ("path", format!("{:?}", e.path)),
        ("confidence", format!("{:?}", e.confidence)),
        ("description", e.description.to_owned()),
        ("not modeled here", e.coverage.to_owned()),
        ("controls", e.controls.to_owned()),
        ("model version", e.model_version.to_owned()),
        ("sources", e.sources.join(", ")),
        ("assumptions", e.assumptions.join(", ")),
    ]
}

/// The control manifest's entries for stage `n` (0: "no stage").
pub fn controls_of(n: u8) -> Vec<&'static ControlEntry> {
    rr_dr60::controls::controls()
        .iter()
        .filter(|e| e.stage == n)
        .collect()
}
