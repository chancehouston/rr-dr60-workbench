//! The settings panel's model (spec 001 FR-004 – FR-006; research R-08; data-model "ControlRow").
//!
//! Every row comes from the engine's control manifest, and the engine itself classifies and checks
//! every value. The workbench keeps no list of controls, classes or ranges.

use rr_dr60::control_map::{self, ControlReading};
use rr_dr60::controls::{ControlEntry, ControlPaths, ControlValue};
use rr_dr60::{Error, Pipeline, Settings};

/// How a control is written (research R-08), derived from the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlClass {
    /// A settings field: `control_map::set_settings_value`.
    Setting,
    /// A live or component control: `Pipeline::set_control_value`.
    EngineControl,
    /// An index this engine does not know (only from a doctored entry list).
    Unknown,
}

/// A row's engine status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowStatus {
    /// The engine accepts the value.
    Accepted,
    /// The engine refuses it, in the engine's words.
    Refused(String),
    /// The control's paths exclude the one-pass (D4).
    NotOnOnePass,
    /// The host rate: the file's rate (D4).
    ReadOnly,
}

/// One manifest control in the panel.
#[derive(Clone, Debug)]
pub struct ControlRow {
    /// The manifest index.
    pub index: u32,
    /// The manifest entry, as the engine gives it.
    pub entry: ControlEntry,
    /// How it is written.
    pub class: ControlClass,
    /// The current value.
    pub value: ControlReading,
    /// The engine's verdict.
    pub status: RowStatus,
}

/// The panel: one row per manifest entry, the settings built from the settings rows, and a kept
/// check engine (research R-08).
pub struct Panel {
    rate_hz: u32,
    rows: Vec<ControlRow>,
    check: Pipeline,
    general: Option<String>,
}

impl std::fmt::Debug for Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panel")
            .field("rate_hz", &self.rate_hz)
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

/// A stage group's title from the engine's stage manifest: "Stage 11 · <name>" (coach 10), or
/// "No stage" for 0.
pub fn stage_title(stage: u8) -> String {
    if stage == 0 {
        return "No stage".to_owned();
    }
    rr_dr60::manifest()
        .stages()
        .iter()
        .find(|e| e.number == stage)
        .map_or_else(
            || format!("Stage {stage}"),
            |e| format!("Stage {stage} · {}", e.name),
        )
}

/// An entry's default as a reading (the host rate's default is the session's rate).
pub fn default_reading(entry: &ControlEntry, rate_hz: u32) -> ControlReading {
    match entry.value {
        ControlValue::Switch { default } => ControlReading::Switch(default),
        ControlValue::Integer { default, .. } => ControlReading::Integer(default),
        ControlValue::Continuous { default, .. } => ControlReading::Continuous(default),
        ControlValue::Choice { default, .. } => ControlReading::Choice(default.unwrap_or(rate_hz)),
        _ => ControlReading::Integer(0),
    }
}

impl Panel {
    /// The panel for the engine's own manifest at `rate_hz`.
    ///
    /// # Errors
    ///
    /// The engine's refusal of the rate.
    pub fn new(rate_hz: u32) -> Result<Self, Error> {
        Self::from_entries(rr_dr60::controls::controls(), rate_hz)
    }

    /// The panel for any entry list (the seam for SC-003's "a control added to the engine").
    ///
    /// # Errors
    ///
    /// The engine's refusal of the rate.
    pub fn from_entries(entries: &[ControlEntry], rate_hz: u32) -> Result<Self, Error> {
        let defaults = Settings::new(rate_hz);
        let check = Pipeline::new(defaults)?;
        let rows = entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let index = i as u32;
                let (class, value) = match control_map::settings_value(&defaults, index) {
                    Ok(v) => (ControlClass::Setting, v),
                    Err(Error::NotASetting { .. }) => (
                        ControlClass::EngineControl,
                        check
                            .control_value(index)
                            .unwrap_or_else(|_| default_reading(e, rate_hz)),
                    ),
                    Err(_) => (ControlClass::Unknown, default_reading(e, rate_hz)),
                };
                let status = if e.identifier == "host_rate_hz" {
                    RowStatus::ReadOnly
                } else if !e.paths.contains(ControlPaths::ONE_PASS) {
                    RowStatus::NotOnOnePass
                } else {
                    RowStatus::Accepted
                };
                ControlRow {
                    index,
                    entry: *e,
                    class,
                    value,
                    status,
                }
            })
            .collect();
        let mut p = Self {
            rate_hz,
            rows,
            check,
            general: None,
        };
        p.recheck();
        Ok(p)
    }

    /// The session's rate.
    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// The rows in manifest order.
    pub fn rows(&self) -> &[ControlRow] {
        &self.rows
    }

    /// The row with `identifier`.
    pub fn row(&self, identifier: &str) -> Option<&ControlRow> {
        self.rows.iter().find(|r| r.entry.identifier == identifier)
    }

    /// The index of `identifier`.
    pub fn index_of(&self, identifier: &str) -> Option<u32> {
        self.row(identifier).map(|r| r.index)
    }

    /// Sets a value and re-checks every row through the engine (FR-006). Read-only and
    /// not-on-the-one-pass rows ignore the call and return `false`.
    pub fn set(&mut self, index: u32, value: ControlReading) -> bool {
        let Some(row) = self.rows.get_mut(index as usize) else {
            return false;
        };
        if matches!(row.status, RowStatus::ReadOnly | RowStatus::NotOnOnePass) {
            return false;
        }
        row.value = value;
        self.recheck();
        true
    }

    /// Sets several values, then checks once (a sidecar restore; hardware review B6). Read-only
    /// and not-on-the-one-pass rows are skipped.
    pub fn set_many(&mut self, values: &[(u32, ControlReading)]) {
        for &(index, value) in values {
            if let Some(row) = self.rows.get_mut(index as usize)
                && !matches!(row.status, RowStatus::ReadOnly | RowStatus::NotOnOnePass)
            {
                row.value = value;
            }
        }
        self.recheck();
    }

    /// A refusal the engine attaches to no control, shown at the panel's top (coach 12).
    pub fn general_refusal(&self) -> Option<&str> {
        self.general.as_deref()
    }

    /// Restores every control to its default (US2 AS7).
    pub fn reset_to_defaults(&mut self) {
        let defaults = Settings::new(self.rate_hz);
        let fresh = Pipeline::new(defaults).ok();
        for row in &mut self.rows {
            row.value = match row.class {
                ControlClass::Setting => control_map::settings_value(&defaults, row.index)
                    .unwrap_or_else(|_| default_reading(&row.entry, self.rate_hz)),
                ControlClass::EngineControl => fresh
                    .as_ref()
                    .and_then(|p| p.control_value(row.index).ok())
                    .unwrap_or_else(|| default_reading(&row.entry, self.rate_hz)),
                ControlClass::Unknown => default_reading(&row.entry, self.rate_hz),
            };
        }
        self.recheck();
    }

    /// True when no row is refused and no panel-level refusal stands (FR-006).
    pub fn renderable(&self) -> bool {
        self.general.is_none()
            && !self
                .rows
                .iter()
                .any(|r| matches!(r.status, RowStatus::Refused(_)))
    }

    /// The settings and the engine values, in manifest order, for a render request.
    pub fn snapshot(&self) -> (Settings, Vec<(u32, ControlReading)>) {
        let settings = self.settings();
        let values: Vec<(u32, ControlReading)> = self
            .rows
            .iter()
            .filter(|r| {
                r.class == ControlClass::EngineControl && r.status != RowStatus::NotOnOnePass
            })
            .map(|r| (r.index, r.value))
            .collect();
        // Manifest order: the slot (31) precedes its parameters (32 – 35), and the engine keeps
        // the parameter store across a slot change, so the order is the manifest's.
        (settings, values)
    }

    /// The settings built from the settings rows on the session's defaults. A value the
    /// engine cannot represent is skipped (its row is refused).
    pub fn settings(&self) -> Settings {
        let mut s = Settings::new(self.rate_hz);
        for r in &self.rows {
            if r.class == ControlClass::Setting && r.status != RowStatus::ReadOnly {
                let _ = control_map::set_settings_value(&mut s, r.index, r.value);
            }
        }
        s
    }

    /// Re-checks every row in research R-08's order and sets each status.
    fn recheck(&mut self) {
        let defaults = Settings::new(self.rate_hz);
        self.general = None;
        for r in &mut self.rows {
            if matches!(r.status, RowStatus::Refused(_)) {
                r.status = RowStatus::Accepted;
            }
        }
        // 1. Each settings value alone on the defaults.
        for r in &mut self.rows {
            if r.class != ControlClass::Setting || r.status != RowStatus::Accepted {
                continue;
            }
            let mut s = defaults;
            if let Err(e) = control_map::set_settings_value(&mut s, r.index, r.value) {
                r.status = RowStatus::Refused(e.to_string());
            } else if let Err(e) = s.validate() {
                r.status = RowStatus::Refused(e.to_string());
            }
        }
        for r in &mut self.rows {
            if r.class == ControlClass::Unknown && r.status == RowStatus::Accepted {
                r.status =
                    RowStatus::Refused(format!("the engine has no control at index {}", r.index));
            }
        }
        if !self.renderable() {
            return;
        }
        // 2. The whole settings, then 3. the check engine's reconfigure.
        let settings = self.settings();
        if let Err(e) = settings
            .validate()
            .and_then(|()| self.check.reconfigure(settings))
        {
            self.refuse_for(&e);
            return;
        }
        // 4. The live and component values through the engine.
        let (_, values) = self.snapshot();
        for (i, v) in values {
            if let Err(e) = self.check.set_control_value(i, v) {
                self.rows[i as usize].status = RowStatus::Refused(e.to_string());
            }
        }
        if !self.renderable() {
            return;
        }
        // 5. The tap against the route: a zero-length call (refused before any state changes).
        if let Err(e) = self.check.process(&[], &mut []) {
            self.refuse_for(&e);
        }
    }

    /// Puts a whole-settings or engine refusal beside the row(s) it names.
    fn refuse_for(&mut self, e: &Error) {
        let msg = e.to_string();
        let targets: Vec<u32> = match e {
            Error::InvalidSetting { setting } => vec![control_map::setting_control(*setting)],
            Error::TapNotOnRoute { .. } => ["tap", "route"]
                .iter()
                .filter_map(|id| self.index_of(id))
                .collect(),
            Error::InvalidControlValue { control, .. } => vec![*control],
            _ => Vec::new(),
        };
        if targets.is_empty() {
            self.general = Some(msg);
            return;
        }
        for t in targets {
            if let Some(r) = self.rows.get_mut(t as usize) {
                r.status = RowStatus::Refused(msg.clone());
            }
        }
    }
}
