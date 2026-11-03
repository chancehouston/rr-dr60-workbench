//! The guided tour (spec 006 D4): the stage manifest stage by stage, each page with the control
//! manifest's entries for that stage, and "compare without this stage". Every word on a page is
//! the engine's (the stage manifest's fields, the control manifest's entries); this module writes
//! no stage text.

use rr_dr60::control_map::{self, ControlReading};
use rr_dr60::controls::{ControlEntry, ControlValue};

use crate::render::{Render, RenderRequest, Scheduled};

/// The tour's pages: the manifest's stages in order, then the "no stage" page (stage 0).
pub fn pages() -> Vec<u8> {
    let mut v: Vec<u8> = crate::stages::stages().iter().map(|e| e.number).collect();
    v.push(0);
    v
}

/// A page's controls: the control manifest's entries with that stage number.
pub fn controls(stage: u8) -> Vec<&'static ControlEntry> {
    crate::stages::controls_of(stage)
}

/// A stage's switches: its controls that are switches (stage 4 has two).
pub fn switches(stage: u8) -> Vec<(u32, &'static ControlEntry)> {
    if stage == 0 {
        return Vec::new();
    }
    rr_dr60::controls::controls()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.stage == stage && matches!(e.value, ControlValue::Switch { .. }))
        .map(|(i, e)| (i as u32, e))
        .collect()
}

/// The request of "compare without this stage" (D4): `render`'s request with the stage's
/// switches off and its own scheduled entries of them removed.
#[derive(Clone, Debug)]
pub struct WithoutStage {
    /// The request.
    pub request: RenderRequest,
    /// The switches turned off (index, identifier).
    pub turned_off: Vec<(u32, &'static str)>,
    /// The scheduled entries removed (they would be refused with the stage off).
    pub removed: Vec<Scheduled>,
}

/// Why "compare without this stage" is not offered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotOffered {
    /// The stage has no switch.
    NoSwitch,
    /// Every switch of the stage is already off.
    AlreadyOff,
    /// The render is stale: render first.
    Stale,
}

impl std::fmt::Display for NotOffered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSwitch => write!(f, "This stage has no switch in the control manifest"),
            Self::AlreadyOff => write!(f, "This stage's switches are already off"),
            Self::Stale => write!(f, "The render is stale: render first"),
        }
    }
}

/// Builds the request (D4).
///
/// # Errors
///
/// No switch, every switch off, or a stale render.
pub fn without_stage(
    render: &Render,
    current: &RenderRequest,
    stage: u8,
) -> Result<WithoutStage, NotOffered> {
    if render.is_stale(current) {
        return Err(NotOffered::Stale);
    }
    let sw = switches(stage);
    if sw.is_empty() {
        return Err(NotOffered::NoSwitch);
    }
    let mut request = render.request.clone();
    let mut turned_off = Vec::new();
    for (i, e) in &sw {
        if control_map::settings_value(&request.settings, *i) == Ok(ControlReading::Switch(true))
            && control_map::set_settings_value(
                &mut request.settings,
                *i,
                ControlReading::Switch(false),
            )
            .is_ok()
        {
            turned_off.push((*i, e.identifier));
        }
    }
    if turned_off.is_empty() {
        return Err(NotOffered::AlreadyOff);
    }
    let (removed, kept): (Vec<Scheduled>, Vec<Scheduled>) = request
        .changes
        .iter()
        .partition(|c| sw.iter().any(|(i, _)| *i == c.control));
    request.changes = kept;
    Ok(WithoutStage {
        request,
        turned_off,
        removed,
    })
}

/// Whether "without the stage" changed nothing at this tap and route (D4): the two outputs are
/// bit-identical.
pub fn no_effect(with: &Render, without: &Render) -> bool {
    with.samples.len() == without.samples.len()
        && with
            .samples
            .iter()
            .zip(without.samples.iter())
            .all(|(a, b)| a.to_bits() == b.to_bits())
}
