//! The sidecar, version 1 (spec 001 D8, D9, FR-013, FR-014; contracts/sidecar-v1.md).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rr_dr60::control_map::{self, ControlReading};
use rr_dr60::controls::ControlValue;

use crate::input::{Channel, Input, hex};
use crate::panel::{ControlClass, Panel, RowStatus};
use crate::render::{self, Render};

/// The first line.
pub const MAGIC: &str = "rr-dr60-workbench sidecar";
/// The format version this build writes (spec 003 D5); it reads 1 and 2.
pub const VERSION: u32 = 2;
/// Version 2's keys.
const V2_REQUIRED: [&str; 3] = ["live", "start", "fed"];
/// The conversion rule's version (research R-02).
pub const CONVERSION: u32 = 1;

const REQUIRED: [&str; 22] = [
    "workbench.version",
    "engine.version",
    "engine.commit",
    "conversion",
    "recording_counter",
    "block",
    "tail",
    "tail_samples",
    "channel",
    "input.file",
    "input.frames",
    "input.raw_sha256",
    "input.sha256",
    "render.frames",
    "render.latency_samples",
    "render.splices_or_regions",
    "render.mode",
    "render.gate_reached",
    "render.paused_at_end",
    "render.not_emitted",
    "render.peak",
    "render.sha256",
];

/// A refused sidecar (FR-014). The session is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidecarError {
    /// The first line is not a sidecar's.
    NotASidecar,
    /// A sidecar of another version.
    UnsupportedVersion(String),
    /// A malformed line (its 1-based number and the reason).
    Malformed(usize, String),
    /// A required key is missing.
    Missing(&'static str),
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotASidecar => write!(f, "This file is not a workbench sidecar"),
            Self::UnsupportedVersion(v) => write!(f, "Sidecar version {v} is not supported"),
            Self::Malformed(n, why) => write!(f, "Sidecar line {n} is malformed: {why}"),
            Self::Missing(k) => write!(f, "The sidecar lacks the required field {k}"),
        }
    }
}

impl std::error::Error for SidecarError {}

/// A parsed sidecar.
#[derive(Clone, Debug, PartialEq)]
pub struct Sidecar {
    /// The version read (1 or 2).
    pub version: u32,
    /// The header fields.
    pub fields: BTreeMap<String, String>,
    /// The control lines in file order: identifier and raw value.
    pub controls: Vec<(String, String)>,
    /// The change lines in file order (version 2): clock, position, identifier, value, tag.
    pub changes: Vec<(rr_dr60::changes::Clock, u64, String, f64, u64)>,
}

/// What a restore did (D9).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestoreReport {
    /// Controls applied.
    pub applied: usize,
    /// Identifiers this engine does not know (skipped).
    pub unknown_ids: Vec<String>,
    /// Controls the engine has that the sidecar lacks (left at their defaults).
    pub missing_controls: Vec<String>,
    /// Values the engine refused or the panel cannot take, with the reason (left unchanged).
    pub refused: Vec<(String, String)>,
    /// The sidecar's engine commit when it differs from this build's.
    pub commit_differs: Option<String>,
    /// The converted input differs from the loaded file.
    pub input_differs: bool,
    /// The sidecar's host rate when it differs from the session's (never applied).
    pub rate_differs: Option<String>,
    /// The channel the sidecar used.
    pub channel: Option<Channel>,
    /// Whether the sidecar's render had the tail switch on.
    pub tail: bool,
    /// Whether the panel is renderable after the restore.
    pub renderable: bool,
}

/// A continuous value: shortest round-trip decimal and its bits.
pub fn fmt_f32(v: f32) -> String {
    format!("{v} 0x{:08x}", v.to_bits())
}

fn fmt_reading(r: ControlReading) -> String {
    match r {
        ControlReading::Switch(b) => if b { "on" } else { "off" }.to_owned(),
        ControlReading::Integer(i) => i.to_string(),
        ControlReading::Continuous(f) => fmt_f32(f),
        ControlReading::Choice(c) => c.to_string(),
    }
}

/// Parses a change line's value: `<in|out> <position> <identifier> <decimal> 0x<f64 bits> <tag>`
/// (spec 003 D5): the bits rule, the decimal must agree, and the tag is at least 1.
fn parse_change(v: &str) -> Result<(rr_dr60::changes::Clock, u64, String, f64, u64), String> {
    let f: Vec<&str> = v.split(' ').collect();
    let [clock, pos, id, dec, bits, tag] = f.as_slice() else {
        return Err(format!("a change line has six fields, got `{v}`"));
    };
    let clock = match *clock {
        "in" => rr_dr60::changes::Clock::Input,
        "out" => rr_dr60::changes::Clock::Output,
        c => return Err(format!("clock is `in` or `out`, got `{c}`")),
    };
    let pos: u64 = pos.parse().map_err(|_| format!("bad position `{pos}`"))?;
    let b = bits
        .strip_prefix("0x")
        .filter(|h| h.len() == 16)
        .and_then(|h| u64::from_str_radix(h, 16).ok())
        .ok_or_else(|| format!("bad bits `{bits}`"))?;
    let value = f64::from_bits(b);
    let d: f64 = dec.parse().map_err(|_| format!("bad decimal `{dec}`"))?;
    if d.to_bits() != b {
        return Err(format!("the decimal {dec} disagrees with the bits {bits}"));
    }
    let tag: u64 = tag.parse().map_err(|_| format!("bad tag `{tag}`"))?;
    if tag == 0 {
        return Err("a change line's tag is at least 1 (tag 0 is the start's own values)".into());
    }
    Ok((clock, pos, (*id).to_owned(), value, tag))
}

/// Parses a `<decimal> 0x<bits>` value; the bits rule and the decimal must agree.
fn parse_f32(v: &str) -> Result<f32, String> {
    let mut it = v.split(' ');
    let (Some(dec), Some(bits), None) = (it.next(), it.next(), it.next()) else {
        return Err(format!("expected `<decimal> 0x<bits>`, got `{v}`"));
    };
    let hexs = bits
        .strip_prefix("0x")
        .filter(|h| h.len() == 8)
        .ok_or_else(|| format!("bad bits `{bits}`"))?;
    let b = u32::from_str_radix(hexs, 16).map_err(|e| e.to_string())?;
    let value = f32::from_bits(b);
    let d: f32 = dec.parse().map_err(|_| format!("bad decimal `{dec}`"))?;
    if d.to_bits() != b {
        return Err(format!("the decimal {dec} disagrees with the bits {bits}"));
    }
    Ok(value)
}

fn parse_reading(v: &str, ty: &ControlValue) -> Result<ControlReading, String> {
    let int = |s: &str| {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            Err(format!("bad integer `{s}`"))
        } else {
            s.parse::<u64>().map_err(|e| e.to_string())
        }
    };
    match ty {
        ControlValue::Switch { .. } => match v {
            "on" => Ok(ControlReading::Switch(true)),
            "off" => Ok(ControlReading::Switch(false)),
            _ => Err(format!("a switch is `on` or `off`, got `{v}`")),
        },
        ControlValue::Integer { .. } => int(v).map(ControlReading::Integer),
        ControlValue::Continuous { .. } => parse_f32(v).map(ControlReading::Continuous),
        ControlValue::Choice { .. } => int(v)
            .and_then(|i| u32::try_from(i).map_err(|e| e.to_string()))
            .map(ControlReading::Choice),
        _ => Err("unknown value type".into()),
    }
}

/// Writes the sidecar of a render (D8). Every manifest control is written as the engine reads it
/// back: settings fields from the render's settings, the others from a fresh engine with the
/// render's values applied.
pub fn write(render: &Render) -> String {
    let req = &render.request;
    let engine = render::fresh_engine(req).ok();
    let mut s = String::new();
    let _ = writeln!(s, "{MAGIC} {VERSION}");
    let live = render.live.as_ref();
    let mut kv = |k: &str, v: &str| {
        let _ = writeln!(s, "{k} {v}");
    };
    kv("workbench.version", crate::workbench_version());
    kv("engine.version", crate::engine_version());
    kv("engine.commit", crate::engine_commit());
    kv("conversion", &CONVERSION.to_string());
    kv("recording_counter", "0");
    kv("live", if live.is_some() { "yes" } else { "no" });
    kv("start", &req.start.to_string());
    let avail = (req.input.len() as u64).saturating_sub(req.start);
    kv(
        "fed",
        &req.len.unwrap_or(avail + render.tail_samples).to_string(),
    );
    kv("block", &req.block.to_string());
    kv("tail", if req.tail { "on" } else { "off" });
    kv("tail_samples", &render.tail_samples.to_string());
    let ch = match req.channel {
        Channel::Index(i) => (u32::from(i) + 1).to_string(),
        Channel::Mix => "mix".to_owned(),
    };
    kv("channel", &ch);
    kv("input.file", &req.file_name.replace('\n', " "));
    kv("input.frames", &req.frames.to_string());
    kv("input.raw_sha256", &hex(&req.raw_sha256));
    kv("input.sha256", &hex(&req.input_sha256));
    kv("render.frames", &render.samples.len().to_string());
    kv(
        "render.latency_samples",
        &render.latency_samples.to_string(),
    );
    kv(
        "render.splices_or_regions",
        &render.splices_or_regions.to_string(),
    );
    kv(
        "render.mode",
        if render.drop_mode { "drop" } else { "mute" },
    );
    kv(
        "render.gate_reached",
        if render.gate_reached { "yes" } else { "no" },
    );
    kv(
        "render.paused_at_end",
        if render.paused_at_end { "yes" } else { "no" },
    );
    kv("render.not_emitted", &render.not_emitted.to_string());
    kv("render.peak", &fmt_f32(render.peak));
    kv("render.sha256", &hex(&render.output_sha256));
    // Settings as the engine holds them after construction (`Pipeline::settings`), so a value is
    // written as the engine reads it back.
    let settings = engine
        .as_ref()
        .map_or(req.settings, |p| *rr_dr60::Pipeline::settings(p));
    for (i, e) in rr_dr60::controls::controls().iter().enumerate() {
        let i = i as u32;
        let reading = control_map::settings_value(&settings, i)
            .ok()
            .or_else(|| engine.as_ref().and_then(|p| p.control_value(i).ok()));
        if let Some(r) = reading {
            let _ = writeln!(s, "control {} {}", e.identifier, fmt_reading(r));
        }
    }
    // The scheduled changes (tag ≥ 1) in log order (spec 003 D5); the tag-0 entries come back
    // from the start values.
    for c in &req.changes {
        let clock = match c.clock {
            rr_dr60::changes::Clock::Input => "in",
            _ => "out",
        };
        let id = rr_dr60::controls::controls()
            .get(c.control as usize)
            .map_or("unknown", |e| e.identifier);
        let _ = writeln!(
            s,
            "change {clock} {} {id} {} 0x{:016x} {}",
            c.position,
            c.value,
            c.value.to_bits(),
            c.tag
        );
    }
    s
}

/// Parses a sidecar (FR-014's refusals).
///
/// # Errors
///
/// Not a sidecar, an unsupported version, a malformed line or a missing required key.
pub fn read(text: &str) -> Result<Sidecar, SidecarError> {
    let mut lines = text.lines().enumerate();
    let first = lines
        .by_ref()
        .find(|(_, l)| !l.trim().is_empty())
        .map(|(_, l)| l)
        .ok_or(SidecarError::NotASidecar)?;
    let version = first
        .strip_prefix(MAGIC)
        .and_then(|r| r.strip_prefix(' '))
        .ok_or(SidecarError::NotASidecar)?;
    let version: u32 = match version {
        "1" => 1,
        "2" => 2,
        v => return Err(SidecarError::UnsupportedVersion(v.to_owned())),
    };
    let mut fields = BTreeMap::new();
    let mut controls: Vec<(String, String)> = Vec::new();
    let mut changes = Vec::new();
    for (n, line) in lines {
        let n = n + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once(' ')
            .ok_or_else(|| SidecarError::Malformed(n, "no value".into()))?;
        if key == "change" && version >= 2 {
            changes.push(parse_change(value).map_err(|e| SidecarError::Malformed(n, e))?);
            continue;
        }
        if key == "control" {
            let (id, v) = value
                .split_once(' ')
                .ok_or_else(|| SidecarError::Malformed(n, "a control line needs a value".into()))?;
            if controls.iter().any(|(c, _)| c == id) {
                return Err(SidecarError::Malformed(n, format!("control {id} repeated")));
            }
            if v.contains(" 0x") {
                parse_f32(v).map_err(|e| SidecarError::Malformed(n, e))?;
            }
            controls.push((id.to_owned(), v.to_owned()));
            continue;
        }
        if !REQUIRED.contains(&key) && !(version >= 2 && V2_REQUIRED.contains(&key)) {
            return Err(SidecarError::Malformed(n, format!("unknown key {key}")));
        }
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(SidecarError::Malformed(n, format!("key {key} repeated")));
        }
        if key == "render.peak" {
            parse_f32(value).map_err(|e| SidecarError::Malformed(n, e))?;
        }
        if key == "tail" && value != "on" && value != "off" {
            return Err(SidecarError::Malformed(
                n,
                format!("tail is `on` or `off`, got `{value}`"),
            ));
        }
        if key == "conversion" && value != CONVERSION.to_string() {
            return Err(SidecarError::Malformed(
                n,
                format!("conversion rule {value} is not supported"),
            ));
        }
    }
    for k in REQUIRED {
        if !fields.contains_key(k) {
            return Err(SidecarError::Missing(k));
        }
    }
    if version >= 2 {
        for k in V2_REQUIRED {
            if !fields.contains_key(k) {
                return Err(SidecarError::Missing(k));
            }
        }
    }
    Ok(Sidecar {
        version,
        fields,
        controls,
        changes,
    })
}

/// Restores a sidecar's values into `panel` and lists every difference (D9). The panel starts
/// from the engine's defaults, so a control the sidecar lacks is at its default; a value the
/// engine refuses leaves its control at its default.
pub fn restore(s: &Sidecar, panel: &mut Panel, input: &Input) -> RestoreReport {
    let mut rep = RestoreReport::default();
    panel.reset_to_defaults();
    let commit = s.fields.get("engine.commit").cloned().unwrap_or_default();
    if commit != crate::engine_commit() {
        rep.commit_differs = Some(commit);
    }
    rep.input_differs =
        s.fields.get("input.sha256").map(String::as_str) != Some(hex(&input.input_sha256).as_str());
    rep.tail = s.fields.get("tail").is_some_and(|t| t == "on");
    rep.channel = s.fields.get("channel").and_then(|c| {
        if c == "mix" {
            Some(Channel::Mix)
        } else {
            c.parse::<u16>()
                .ok()
                .filter(|&n| n >= 1)
                .map(|n| Channel::Index(n - 1))
        }
    });
    let mut seen = Vec::new();
    // The slot is written before its parameters in the manifest; apply in manifest order so a
    // file in another order restores the same way.
    let mut ordered: Vec<(u32, &str, &str)> = Vec::new();
    for (id, v) in &s.controls {
        match panel.index_of(id) {
            Some(i) => ordered.push((i, id, v)),
            None => rep.unknown_ids.push(id.clone()),
        }
    }
    ordered.sort_by_key(|(i, _, _)| *i);
    // Parse and filter first; then set every value and check once (hardware review B6), so a
    // value valid only together with another is not refused against a partial state.
    let mut batch: Vec<(u32, ControlReading, String)> = Vec::new();
    for (i, id, v) in ordered {
        seen.push(i);
        let row = panel.rows()[i as usize].clone();
        let reading = match parse_reading(v, &row.entry.value) {
            Ok(r) => r,
            Err(e) => {
                rep.refused.push((id.to_owned(), e));
                continue;
            }
        };
        if row.status == RowStatus::ReadOnly {
            if reading != row.value {
                rep.rate_differs = Some(v.to_owned());
            }
            continue;
        }
        if reading == row.value {
            rep.applied += 1;
            continue;
        }
        if row.status == RowStatus::NotOnOnePass {
            rep.refused
                .push((id.to_owned(), crate::text::NOT_ON_ONE_PASS.to_owned()));
            continue;
        }
        if let ControlValue::Choice { options, .. } = row.entry.value
            && let ControlReading::Choice(c) = reading
            && !options.iter().any(|o| o.value == c)
        {
            rep.refused
                .push((id.to_owned(), format!("{c} is not one of the options")));
            continue;
        }
        batch.push((i, reading, id.to_owned()));
    }
    let values: Vec<(u32, ControlReading)> = batch.iter().map(|(i, r, _)| (*i, *r)).collect();
    panel.set_many(&values);
    // Values the engine refused, row by row: reported and returned to their defaults, then
    // checked again; repeated until the panel holds only accepted values.
    loop {
        let refused: Vec<(u32, String)> = batch
            .iter()
            .filter_map(|(i, _, _)| match &panel.rows()[*i as usize].status {
                RowStatus::Refused(why) => Some((*i, why.clone())),
                _ => None,
            })
            .collect();
        if refused.is_empty() {
            break;
        }
        let defaults = Panel::new(panel.rate_hz()).ok();
        let mut back = Vec::new();
        for (i, why) in refused {
            let (_, _, id) = batch.iter().find(|(j, _, _)| *j == i).cloned().unwrap();
            rep.refused.push((id, why));
            batch.retain(|(j, _, _)| *j != i);
            if let Some(d) = &defaults {
                back.push((i, d.rows()[i as usize].value));
            }
        }
        panel.set_many(&back);
    }
    rep.applied += batch.len();
    rep.renderable = panel.renderable();
    for r in panel.rows() {
        if !seen.contains(&r.index) && r.class != ControlClass::Unknown {
            rep.missing_controls.push(r.entry.identifier.to_owned());
        }
    }
    rep
}
