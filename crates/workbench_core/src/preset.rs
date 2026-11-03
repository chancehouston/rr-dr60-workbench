//! Settings presets (spec 005b D6): the panel's values as a file, restored by the sidecar's
//! control rules ([`sidecar::restore_controls`]).

use std::fmt::Write as _;

use crate::panel::Panel;
use crate::sidecar::{self, RestoreReport};

/// The first line.
pub const MAGIC: &str = "rr-dr60-workbench preset";
/// The format version.
pub const VERSION: u32 = 1;

const KEYS: [&str; 3] = ["workbench.version", "engine.version", "engine.commit"];

/// A refused preset, in words by `Display`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetError {
    /// The first line is not a preset's (a sidecar included).
    NotAPreset,
    /// Another version.
    UnsupportedVersion(String),
    /// A malformed line: its 1-based number and the reason.
    Malformed(usize, String),
    /// A required key is missing.
    Missing(&'static str),
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAPreset => write!(f, "This file is not a workbench preset"),
            Self::UnsupportedVersion(v) => write!(f, "Preset version {v} is not supported"),
            Self::Malformed(n, why) => write!(f, "Preset line {n} is malformed: {why}"),
            Self::Missing(k) => write!(f, "The preset lacks the required field {k}"),
        }
    }
}

impl std::error::Error for PresetError {}

/// A parsed preset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    /// The engine commit it was saved with.
    pub engine_commit: String,
    /// The control lines in file order: identifier and raw value.
    pub controls: Vec<(String, String)>,
}

/// Writes the panel's values as a preset: every manifest control, as the sidecar writes it (the
/// seed and the tap included; the host rate written, never applied).
pub fn write(panel: &Panel) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{MAGIC} {VERSION}");
    let _ = writeln!(s, "workbench.version {}", crate::workbench_version());
    let _ = writeln!(s, "engine.version {}", crate::engine_version());
    let _ = writeln!(s, "engine.commit {}", crate::engine_commit());
    for r in panel.rows() {
        let _ = writeln!(
            s,
            "control {} {}",
            r.entry.identifier,
            sidecar::fmt_reading(r.value)
        );
    }
    s
}

/// Parses a preset.
///
/// # Errors
///
/// Not a preset (a sidecar included), another version, a malformed or repeated line, an unknown
/// or missing key; in words.
pub fn read(text: &str) -> Result<Preset, PresetError> {
    let mut lines = text.lines().enumerate();
    let first = lines
        .by_ref()
        .find(|(_, l)| !l.trim().is_empty())
        .map(|(_, l)| l)
        .ok_or(PresetError::NotAPreset)?;
    let v = first
        .strip_prefix(MAGIC)
        .and_then(|r| r.strip_prefix(' '))
        .ok_or(PresetError::NotAPreset)?;
    if v != VERSION.to_string() {
        return Err(PresetError::UnsupportedVersion(v.to_owned()));
    }
    let mut commit = None;
    let mut seen: Vec<&str> = Vec::new();
    let mut controls: Vec<(String, String)> = Vec::new();
    for (n, line) in lines {
        let n = n + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once(' ')
            .ok_or_else(|| PresetError::Malformed(n, "no value".into()))?;
        if k == "control" {
            let (id, val) = v
                .split_once(' ')
                .ok_or_else(|| PresetError::Malformed(n, "a control line needs a value".into()))?;
            if controls.iter().any(|(c, _)| c == id) {
                return Err(PresetError::Malformed(n, format!("control {id} repeated")));
            }
            controls.push((id.to_owned(), val.to_owned()));
            continue;
        }
        if !KEYS.contains(&k) {
            return Err(PresetError::Malformed(n, format!("unknown key {k}")));
        }
        if seen.contains(&k) {
            return Err(PresetError::Malformed(n, format!("key {k} repeated")));
        }
        seen.push(k);
        if k == "engine.commit" {
            commit = Some(v.to_owned());
        }
    }
    for k in KEYS {
        if !seen.contains(&k) {
            return Err(PresetError::Missing(k));
        }
    }
    Ok(Preset {
        engine_commit: commit.unwrap_or_default(),
        controls,
    })
}

/// Applies a preset to the panel by the sidecar's control rules (D6).
pub fn restore(p: &Preset, panel: &mut Panel) -> RestoreReport {
    sidecar::restore_controls(&p.controls, &p.engine_commit, panel)
}
