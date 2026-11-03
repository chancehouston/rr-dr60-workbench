//! The workbench's logic, with no UI dependency (constitution principle V).
//!
//! The bootstrap holds only the control manifest's grouping and labels; features add sessions,
//! rendering, scheduling and analysis here, each tested without a window or an audio device.

use rr_dr60::controls::{ControlEntry, ControlKind, ControlTiming};

/// The engine version this build links (the pinned commit's `rr_dr60::VERSION`).
pub fn engine_version() -> &'static str {
    rr_dr60::VERSION
}

/// The control manifest's entries for one signal-chain stage (0 = no stage), in manifest order.
pub fn controls_for_stage(stage: u8) -> impl Iterator<Item = &'static ControlEntry> {
    rr_dr60::controls::controls()
        .iter()
        .filter(move |e| e.stage == stage)
}

/// The stages that have at least one control, ascending (0 = no stage first).
pub fn stages_with_controls() -> Vec<u8> {
    let mut stages: Vec<u8> = rr_dr60::controls::controls()
        .iter()
        .map(|e| e.stage)
        .collect();
    stages.sort_unstable();
    stages.dedup();
    stages
}

/// The kind's words as the engine's control manifest uses them.
pub fn kind_label(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::DeviceControl => "device control",
        ControlKind::UnitCondition => "unit condition",
        ControlKind::ModelInternal => "model internal",
        ControlKind::EmulatorOption => "emulator option",
        ControlKind::HostParameter => "host parameter",
        _ => "unknown kind",
    }
}

/// The timing's words as the engine's control manifest uses them.
pub fn timing_label(timing: ControlTiming) -> &'static str {
    match timing {
        ControlTiming::Live => "live",
        ControlTiming::Idle => "idle",
        ControlTiming::IdleMemoryDependent => "idle, memory-dependent",
        ControlTiming::HostFixed => "host-fixed",
        _ => "unknown timing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_control_is_in_exactly_one_stage_group() {
        let grouped: usize = stages_with_controls()
            .into_iter()
            .map(|s| controls_for_stage(s).count())
            .sum();
        assert_eq!(grouped, rr_dr60::controls::controls().len());
    }

    #[test]
    fn every_kind_and_timing_has_a_known_label() {
        for e in rr_dr60::controls::controls() {
            assert_ne!(kind_label(e.kind), "unknown kind", "{}", e.identifier);
            assert_ne!(timing_label(e.timing), "unknown timing", "{}", e.identifier);
        }
    }
}
