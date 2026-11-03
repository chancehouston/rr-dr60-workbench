//! Processing records (spec 005): the run a recording keeps (D1), the record and its
//! record-input file (D2), the read-out's flags and the settings diff (D3), and the re-render, a
//! mirror of the engine's recipe (D4; the engine's Swift `ReRender.reRender` and `fill`, engine
//! issue #76, which waits for engine spec 027; the engine's 0.93 fixed its Swift restore, #77).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rr_dr60::changes::ChangeEntry;
use rr_dr60::components::{self, ComponentId, InsertionPointId};
use rr_dr60::control_map::ControlReading;
use rr_dr60::record::{self, Origin, RecordReadout, StopFields};
use rr_dr60::{Encoding, Error, Pipeline, Settings, StopResult, Stream, VasEvent};
use sha2::{Digest, Sha256};

use crate::input::{Channel, hex};
use crate::render::block_for;

/// The record-input file's first line (contracts/record-input-v1.md).
pub const MAGIC: &str = "rr-dr60-workbench record-input";
/// The record-input file's version.
pub const VERSION: u32 = 1;

/// Where a recording's input came from (D1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSource {
    /// The input file's name (empty: not named).
    pub file_name: String,
    /// The converted input's SHA-256 (001's digest), when named.
    pub input_sha256: Option<[u8; 32]>,
    /// The channel the input was converted from, when named.
    pub channel: Option<Channel>,
    /// 001's conversion rule's version.
    pub conversion: u32,
    /// The start offset in the converted input, host samples.
    pub start: u64,
}

impl RunSource {
    /// A source that names no file (the core's own callers).
    pub fn unnamed(start: u64) -> Self {
        Self {
            file_name: String::new(),
            input_sha256: None,
            channel: None,
            conversion: crate::sidecar::CONVERSION,
            start,
        }
    }

    /// The session input's source at `start`.
    pub fn of(input: &crate::input::Input, start: u64) -> Self {
        Self {
            file_name: input.file_name.clone(),
            input_sha256: Some(input.input_sha256),
            channel: Some(input.channel),
            conversion: crate::sidecar::CONVERSION,
            start,
        }
    }
}

/// A recording's run: everything the engine's processing record needs (D1).
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The recorder's gate reports, the stop flush's included.
    pub events: Vec<VasEvent>,
    /// The record stream's change log, the stop's entries included.
    pub changes: Vec<ChangeEntry>,
    /// The stop's fields, by the engine's conversions.
    pub stop: StopFields,
    /// Where the input came from.
    pub source: RunSource,
}

/// The fed span is exact on the 24-bit grid: every sample round-trips through the engine's
/// feed-point rule bit for bit (D2; plan R-03).
pub fn feed_point_i24(x: &[f32]) -> bool {
    let mut back = [0.0f32];
    x.iter().all(|&s| {
        rr_dr60::input::samples_from_i24(&[rr_dr60::input::i24_from_sample(s)], &mut back);
        back[0].to_bits() == s.to_bits()
    })
}

/// The fed span as little-endian `i32` values holding 24-bit samples (D2).
pub fn i24_bytes(x: &[f32]) -> Vec<u8> {
    x.iter()
        .flat_map(|&s| rr_dr60::input::i24_from_sample(s).to_le_bytes())
        .collect()
}

/// SHA-256 of bytes.
pub fn sha256(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn channel_text(c: Channel) -> String {
    match c {
        Channel::Index(i) => (u32::from(i) + 1).to_string(),
        Channel::Mix => "mix".to_owned(),
    }
}

fn parse_channel(t: &str) -> Option<Channel> {
    if t == "mix" {
        return Some(Channel::Mix);
    }
    t.parse::<u16>()
        .ok()
        .filter(|&n| n >= 1)
        .map(|n| Channel::Index(n - 1))
}

/// How a record's input is fed at a re-render (D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedPoint {
    /// The fed span is on the 24-bit grid; the `.i24` file holds it.
    I24,
    /// The workbench's own f32 samples: the record re-renders in the workbench only.
    F32,
    /// No input (an imported item).
    None,
}

/// A record-input file (contracts/record-input-v1.md).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordInput {
    /// The record file's name.
    pub record_file: String,
    /// The record file's SHA-256.
    pub record_sha256: [u8; 32],
    /// The record's origin.
    pub imported: bool,
    /// The input file's name (`None`: none).
    pub input_file: Option<String>,
    /// The converted input's SHA-256.
    pub input_sha256: Option<[u8; 32]>,
    /// The channel.
    pub channel: Option<Channel>,
    /// 001's conversion rule's version.
    pub conversion: Option<u32>,
    /// The host rate of the recording.
    pub host_rate: u32,
    /// The start offset (`None`: an imported item).
    pub start: Option<u64>,
    /// The fed count (`None`: an imported item).
    pub fed: Option<u64>,
    /// The feed point.
    pub feed_point: FeedPoint,
}

/// The files a saved record consists of (D2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFiles {
    /// The engine's processing record, byte for byte.
    pub record: Vec<u8>,
    /// The record-input file's text.
    pub input_text: String,
    /// The fed span as `.i24` bytes, when the feed point is `i24`.
    pub i24: Option<Vec<u8>>,
}

/// Writes the record-input file for `ri`.
pub fn write_record_input(ri: &RecordInput) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{MAGIC} {VERSION}");
    let mut kv = |k: &str, v: &str| {
        let _ = writeln!(s, "{k} {v}");
    };
    kv("workbench.version", crate::workbench_version());
    kv("engine.commit", crate::engine_commit());
    kv("host_rate", &ri.host_rate.to_string());
    kv("record.file", &ri.record_file.replace('\n', " "));
    kv("record.sha256", &hex(&ri.record_sha256));
    kv("origin", if ri.imported { "imported" } else { "engine" });
    kv(
        "input.file",
        &ri.input_file
            .as_deref()
            .map_or_else(|| "none".to_owned(), |f| f.replace('\n', " ")),
    );
    kv(
        "input.sha256",
        &ri.input_sha256
            .map_or_else(|| "none".to_owned(), |h| hex(&h)),
    );
    kv(
        "channel",
        &ri.channel.map_or_else(|| "none".to_owned(), channel_text),
    );
    kv(
        "conversion",
        &ri.conversion
            .map_or_else(|| "none".to_owned(), |c| c.to_string()),
    );
    let opt = |v: Option<u64>| v.map_or_else(|| "none".to_owned(), |v| v.to_string());
    kv("start", &opt(ri.start));
    kv("fed", &opt(ri.fed));
    kv(
        "feed_point",
        match ri.feed_point {
            FeedPoint::I24 => "i24",
            FeedPoint::F32 => "f32",
            FeedPoint::None => "none",
        },
    );
    s
}

fn parse_hex(t: &str) -> Option<[u8; 32]> {
    if t.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(t.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

const INPUT_KEYS: [&str; 13] = [
    "workbench.version",
    "engine.commit",
    "host_rate",
    "record.file",
    "record.sha256",
    "origin",
    "input.file",
    "input.sha256",
    "channel",
    "conversion",
    "start",
    "fed",
    "feed_point",
];

/// Parses a record-input file.
///
/// # Errors
///
/// Not a record-input file, another version, a malformed or repeated line, an unknown or a
/// missing key; in words.
pub fn read_record_input(text: &str) -> Result<RecordInput, String> {
    let mut lines = text.lines().enumerate();
    let first = lines
        .by_ref()
        .find(|(_, l)| !l.trim().is_empty())
        .map(|(_, l)| l)
        .ok_or_else(|| "This file is not a record-input file".to_owned())?;
    let v = first
        .strip_prefix(MAGIC)
        .and_then(|r| r.strip_prefix(' '))
        .ok_or_else(|| "This file is not a record-input file".to_owned())?;
    if v != VERSION.to_string() {
        return Err(format!("Record-input version {v} is not supported"));
    }
    let mut f: BTreeMap<&str, &str> = BTreeMap::new();
    for (n, line) in lines {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once(' ')
            .ok_or_else(|| format!("Record-input line {} has no value", n + 1))?;
        if !INPUT_KEYS.contains(&k) {
            return Err(format!("Record-input line {}: unknown key {k}", n + 1));
        }
        if f.insert(k, v).is_some() {
            return Err(format!("Record-input line {}: key {k} repeated", n + 1));
        }
    }
    for k in INPUT_KEYS {
        if !f.contains_key(k) {
            return Err(format!("The record-input file lacks {k}"));
        }
    }
    let bad = |k: &str| format!("The record-input file's {k} is malformed");
    let none_or = |k: &str| (f[k] != "none").then(|| f[k]);
    Ok(RecordInput {
        record_file: f["record.file"].to_owned(),
        record_sha256: parse_hex(f["record.sha256"]).ok_or_else(|| bad("record.sha256"))?,
        imported: match f["origin"] {
            "engine" => false,
            "imported" => true,
            _ => return Err(bad("origin")),
        },
        input_file: none_or("input.file").map(str::to_owned),
        input_sha256: match none_or("input.sha256") {
            Some(h) => Some(parse_hex(h).ok_or_else(|| bad("input.sha256"))?),
            None => None,
        },
        channel: match none_or("channel") {
            Some(c) => Some(parse_channel(c).ok_or_else(|| bad("channel"))?),
            None => None,
        },
        conversion: match none_or("conversion") {
            Some(c) => Some(c.parse().map_err(|_| bad("conversion"))?),
            None => None,
        },
        host_rate: f["host_rate"].parse().map_err(|_| bad("host_rate"))?,
        start: match none_or("start") {
            Some(v) => Some(v.parse().map_err(|_| bad("start"))?),
            None => None,
        },
        fed: match none_or("fed") {
            Some(v) => Some(v.parse().map_err(|_| bad("fed"))?),
            None => None,
        },
        feed_point: match f["feed_point"] {
            "i24" => FeedPoint::I24,
            "f32" => FeedPoint::F32,
            "none" => FeedPoint::None,
            _ => return Err(bad("feed_point")),
        },
    })
}

/// The engine's eight read-out flags by their constants' names (D3; plan R-05).
pub const FLAG_NAMES: [(u32, &str); 8] = [
    (record::flags::LIBRARY_DIFFERS, "LIBRARY_DIFFERS"),
    (record::flags::STAGE_MODEL_DIFFERS, "STAGE_MODEL_DIFFERS"),
    (record::flags::HOST_RATE_DIFFERS, "HOST_RATE_DIFFERS"),
    (record::flags::UNKNOWN_SETTINGS, "UNKNOWN_SETTINGS"),
    (record::flags::INCOMPLETE_RUN, "INCOMPLETE_RUN"),
    (record::flags::IMPORTED_ORIGIN, "IMPORTED_ORIGIN"),
    (record::flags::DIGEST_MISMATCH, "DIGEST_MISMATCH"),
    (record::flags::UNKNOWN_COMPONENT, "UNKNOWN_COMPONENT"),
];

/// The names of the flags set in `flags`, in the engine's bit order; an unnamed bit as hex.
pub fn flag_names(flags: u32) -> Vec<String> {
    let mut out: Vec<String> = FLAG_NAMES
        .iter()
        .filter(|(b, _)| flags & b != 0)
        .map(|(_, n)| (*n).to_owned())
        .collect();
    let known = FLAG_NAMES.iter().fold(0, |a, (b, _)| a | b);
    if flags & !known != 0 {
        out.push(format!("0x{:x}", flags & !known));
    }
    out
}

/// The flags a re-render shows before it runs, under which a "differs" is expected (D4, C10).
pub const EXPECTED_DIFFER: u32 = record::flags::LIBRARY_DIFFERS
    | record::flags::STAGE_MODEL_DIFFERS
    | record::flags::HOST_RATE_DIFFERS
    | record::flags::UNKNOWN_SETTINGS;

/// The record's change rows (`record::record_changes`).
pub fn change_rows(bytes: &[u8], r: &RecordReadout) -> Vec<ChangeEntry> {
    let mut v = vec![ChangeEntry::default(); r.change_count as usize];
    let n = record::record_changes(bytes, 0, &mut v);
    v.truncate(n);
    v
}

/// The record's unknown setting names (`record::record_unknown_setting`).
pub fn unknown_settings(bytes: &[u8], r: &RecordReadout) -> Vec<String> {
    (0..r.unknown_setting_count)
        .filter_map(|i| record::record_unknown_setting(bytes, i).map(str::to_owned))
        .collect()
}

/// What a re-render needs, from a read-out and its change rows (plan R-07). The tests apply the
/// engine's deltas to it (spec US1 AS3).
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// The record's settings.
    pub settings: Settings,
    /// The recording counter.
    pub counter: u64,
    /// The remaining capacity at the start, device samples.
    pub remaining_at_start: u64,
    /// The slot's component.
    pub component: ComponentId,
    /// The slot's parameter store.
    pub store: [f32; components::COMPONENT_PARAMETER_BUDGET],
    /// The record's change entries.
    pub changes: Vec<ChangeEntry>,
    /// The stop's fields.
    pub stop: StopFields,
    /// Fill by import (always in the window; the "no fill" delta clears it).
    pub fill: bool,
}

/// A re-render's refusal or failure (D4), in words by `Display`.
#[derive(Clone, Debug, PartialEq)]
pub enum ReRenderError {
    /// An imported-origin record has no run.
    Imported,
    /// The record names a component this engine does not know.
    UnknownComponent,
    /// The record-input file is not this record's (its SHA-256).
    NotThisRecord,
    /// The record-input file does not parse (its reason).
    RecordInput(String),
    /// The session's input is not the record's: its file name.
    OtherInput(String),
    /// The filler did not bring the remaining capacity to the record's.
    Fill(String),
    /// The re-render's stop kept nothing.
    NothingKept,
    /// Cancelled.
    Cancelled,
    /// The engine refused a step.
    Engine(Error),
}

impl std::fmt::Display for ReRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Imported => write!(
                f,
                "An imported item's record names its stored form only: it has no run to re-render"
            ),
            Self::UnknownComponent => {
                write!(f, "The record names a component this engine does not know")
            }
            Self::NotThisRecord => write!(
                f,
                "The record-input file belongs to another record (its record digest differs)"
            ),
            Self::OtherInput(name) => write!(
                f,
                "The session's input is not the record's: open {name} with the same channel"
            ),
            Self::RecordInput(why) => write!(f, "{why}"),
            Self::Fill(why) => write!(f, "The memory could not be filled as the record's: {why}"),
            Self::NothingKept => write!(f, "The re-render kept nothing: the record differs"),
            Self::Cancelled => write!(f, "Re-render cancelled"),
            Self::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl From<Error> for ReRenderError {
    fn from(e: Error) -> Self {
        Self::Engine(e)
    }
}

impl Recipe {
    /// The recipe of a read record.
    ///
    /// # Errors
    ///
    /// An imported-origin record; a component this engine does not know.
    pub fn of(bytes: &[u8], r: &RecordReadout) -> Result<Self, ReRenderError> {
        if r.origin == Origin::Imported {
            return Err(ReRenderError::Imported);
        }
        if r.flags & record::flags::UNKNOWN_COMPONENT != 0 {
            return Err(ReRenderError::UnknownComponent);
        }
        Ok(Self {
            settings: r.settings,
            counter: r.recording_counter,
            remaining_at_start: r.remaining_samples_at_start,
            component: r.component,
            store: r.store,
            changes: change_rows(bytes, r),
            stop: r.stop,
            fill: true,
        })
    }
}

/// A re-render prepared (plan R-07b): the recipe, the read-out and the span of the session's
/// input to feed.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    /// The recipe.
    pub recipe: Recipe,
    /// The record's read-out (the flags shown before the run).
    pub readout: RecordReadout,
    /// The span of the session's input, from the record's start.
    pub span: std::ops::Range<usize>,
}

/// Prepares a re-render of `record` with its record-input file against the session's input
/// (D4's refusals, one variant each).
///
/// # Errors
///
/// A record-input file that does not parse or belongs to another record; the engine's refusal
/// of the record; an imported origin; an unknown component; another input or channel.
pub fn prepare(
    record: &[u8],
    record_input: &str,
    input: &crate::input::Input,
) -> Result<Prepared, ReRenderError> {
    let ri = read_record_input(record_input).map_err(ReRenderError::RecordInput)?;
    if ri.record_sha256 != sha256(record) {
        return Err(ReRenderError::NotThisRecord);
    }
    let mut readout = RecordReadout::default();
    record::read(record, &mut readout)?;
    let recipe = Recipe::of(record, &readout)?;
    let name = ri.input_file.clone().unwrap_or_default();
    if ri.input_sha256 != Some(input.input_sha256) || ri.channel != Some(input.channel) {
        return Err(ReRenderError::OtherInput(name));
    }
    let len = input.samples.len();
    let start = usize::try_from(ri.start.unwrap_or(0))
        .unwrap_or(len)
        .min(len);
    let end = usize::try_from(ri.fed.unwrap_or(0))
        .map_or(len, |f| start.saturating_add(f))
        .min(len);
    Ok(Prepared {
        recipe,
        readout,
        span: start..end,
    })
}

/// A re-render's result (D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The new stored form's digest.
    pub digest: u64,
    /// The record's digest.
    pub expected: u64,
}

impl Outcome {
    /// Whether the record and the recording belong together.
    pub fn reproduces(&self) -> bool {
        self.digest == self.expected
    }
}

/// A filler: `samples` device samples of zeros recorded on a throwaway 6 kHz engine with every
/// record stage off and the codec as `encoding`'s, exported (Swift's `fill`; never by recording
/// on the re-render engine, which would move its counter).
///
/// # Errors
///
/// The throwaway engine's refusal, or a filler not kept at exactly `samples`.
pub fn filler(encoding: Encoding, samples: u64) -> Result<Vec<u8>, ReRenderError> {
    let mut s = Settings::new(6000);
    s.record_stage_enabled = false;
    s.adc_clamp_enabled = false;
    s.agc.enabled = false;
    s.vas.enabled = false;
    s.mic.capsule_enabled = false;
    s.mic.amp_enabled = false;
    s.speech_codec_enabled = encoding == Encoding::Frames;
    s.memory.capacity_s = u32::try_from(samples / 6000 + 1)
        .map_err(|_| ReRenderError::Fill("the filler is too long".into()))?;
    let mut q = Pipeline::new(s)?;
    q.start_recording()?;
    let _ = Pipeline::record(&mut q, &vec![0.0f32; samples as usize], &mut [])?;
    let out = q.stop_recording(&mut [])?;
    let StopResult::Kept { number, length } = out.result else {
        return Err(ReRenderError::Fill("the filler kept nothing".into()));
    };
    if length != samples {
        return Err(ReRenderError::Fill(format!(
            "the filler kept {length} samples of {samples}"
        )));
    }
    let mut bytes = vec![0u8; q.export_len(number)?];
    let _ = q.export(number, &mut bytes)?;
    Ok(bytes)
}

/// Restores the slot (plan R-07 step 3): every component's parameters at their published
/// offsets (`components::components()`, `parameter_offset + i`; post-implementation review B4),
/// whatever the component, as the engine's harness restores the store; then the component.
///
/// # Errors
///
/// The engine's refusal of a value or of the component.
pub fn restore_slot(
    p: &mut Pipeline,
    component: ComponentId,
    store: &[f32; components::COMPONENT_PARAMETER_BUDGET],
) -> Result<(), Error> {
    let point = InsertionPointId::BeforeLimiter;
    for c in components::components() {
        for k in 0..c.parameters.len() {
            let at = c.parameter_offset as usize + k;
            if let Some(v) = store.get(at) {
                p.set_component_parameter(point, at as u32, f64::from(*v))?;
            }
        }
    }
    p.set_component(point, component)
}

/// The note a re-render's result carries when the record's flags make a difference expected
/// (D4, C10): `None` when no such flag is set.
pub fn expected_note(flags: u32) -> Option<String> {
    let names = flag_names(flags & EXPECTED_DIFFER);
    (!names.is_empty()).then(|| format!("expected under {}", names.join(", ")))
}

/// Re-renders `recipe` from `fed`, the input from the record's start (D4), in 10 ms calls;
/// `progress` counts the host samples fed, `cancel` stops between calls. Returns the new stored
/// form's digest and the record's.
///
/// # Errors
///
/// The engine's refusal of a step; a fill that did not bring the remaining capacity to the
/// record's; a cancel.
pub fn re_render(
    recipe: &Recipe,
    expected: u64,
    fed: &[f32],
    progress: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<Outcome, ReRenderError> {
    let mut p = Pipeline::new_at_counter(recipe.settings, recipe.counter)?;
    // 2. The fill by import.
    let st = p.memory_status();
    if recipe.fill && st.remaining_samples > recipe.remaining_at_start {
        let missing = st.remaining_samples - recipe.remaining_at_start;
        let bytes = filler(Pipeline::encoding(&p), missing)?;
        let _ = p.import(&bytes)?;
        let now = p.memory_status().remaining_samples;
        if now != recipe.remaining_at_start {
            return Err(ReRenderError::Fill(format!(
                "{now} samples remain, the record's {}",
                recipe.remaining_at_start
            )));
        }
    }
    // 3. The slot: the parameters the engine accepts, then the component.
    restore_slot(&mut p, recipe.component, &recipe.store)?;
    // 4. The schedule, refilled below 64 pending.
    let mut next = 0usize;
    let schedule = |p: &mut Pipeline, next: &mut usize| -> Result<(), Error> {
        while *next < recipe.changes.len() && p.pending_changes(Stream::Record)? < 64 {
            let c = recipe.changes[*next];
            p.schedule_change(Stream::Record, c.position, c.control, c.after, c.tag)?;
            *next += 1;
        }
        Ok(())
    };
    schedule(&mut p, &mut next)?;
    // 5. The feed up to the stop position.
    p.start_recording()?;
    let rate = recipe.settings.host_rate_hz;
    let block = block_for(rate).max(1);
    let end = usize::try_from(recipe.stop.stop_input_position)
        .unwrap_or(usize::MAX)
        .min(fed.len());
    // The reports do not enter the digest, so this buffer is not regrown after a hang landing
    // (as the engine's harness; review B6). The calls are 10 ms, as the recording's were, so
    // 64 pending is always enough between two refills (review B4's note).
    let mut ev = vec![VasEvent::new(0, 0); p.max_record_events(block).max(1)];
    let mut pos = 0usize;
    let mut stopped = false;
    while pos < end {
        if cancel.load(Ordering::Relaxed) {
            return Err(ReRenderError::Cancelled);
        }
        let n = block.min(end - pos);
        let info = Pipeline::record(&mut p, &fed[pos..pos + n], &mut ev)?;
        pos += n;
        progress.store(pos as u64, Ordering::Relaxed);
        schedule(&mut p, &mut next)?;
        if info.stopped.is_some() {
            stopped = true;
            break;
        }
    }
    // 6. No stop call after a self-stop; otherwise a stop call (also when a self-stop was
    // expected and did not come, so the result honestly differs).
    let number = if stopped {
        p.memory_status().count
    } else {
        let mut sev = vec![VasEvent::new(0, 0); p.max_record_events(rate as usize).max(1)];
        match p.stop_recording(&mut sev)?.result {
            StopResult::Kept { number, .. } => number,
            _ => return Err(ReRenderError::NothingKept),
        }
    };
    Ok(Outcome {
        digest: p.recording_digest(number)?,
        expected,
    })
}

/// A settings difference between a record and the panel (D3).
#[derive(Clone, Debug, PartialEq)]
pub struct DiffRow {
    /// The control's index.
    pub index: u32,
    /// Its identifier.
    pub identifier: &'static str,
    /// The record's reading.
    pub record: ControlReading,
    /// The panel's reading.
    pub panel: ControlReading,
}

/// The controls a record does not carry: the five live playback values (D3, C8).
pub fn not_in_record() -> Vec<&'static str> {
    crate::device::PLAY_VALUES.to_vec()
}

/// The settings diff of a record against the panel's values (D3; plan review Q1), as engine
/// readings with −0.0 equal to 0.0: the settings fields by the pure
/// `control_map::settings_value` of the record's settings; the slot by `control_value` on a
/// small check engine with the record's slot restored (none when the record names an unknown
/// component). The live playback values are skipped (see [`not_in_record`]). An imported record
/// carries no settings: `None`.
///
/// # Errors
///
/// The check engine's refusal of the record's slot.
pub fn settings_diff(
    r: &RecordReadout,
    panel: &[(u32, ControlReading)],
) -> Result<Option<Vec<DiffRow>>, Error> {
    if r.origin == Origin::Imported {
        return Ok(None);
    }
    let slot = if r.flags & record::flags::UNKNOWN_COMPONENT == 0 {
        let mut s = Settings::new(r.settings.host_rate_hz);
        s.memory.capacity_s = 1;
        let mut p = Pipeline::new(s)?;
        restore_slot(&mut p, r.component, &r.store)?;
        Some(p)
    } else {
        None
    };
    let skip = not_in_record();
    let mut out = Vec::new();
    for (i, e) in rr_dr60::controls::controls().iter().enumerate() {
        if skip.contains(&e.identifier) {
            continue;
        }
        let i = i as u32;
        let rec = match rr_dr60::control_map::settings_value(&r.settings, i) {
            Ok(v) => Some(v),
            Err(_) => slot.as_ref().and_then(|p| p.control_value(i).ok()),
        };
        let (Some(rec), Some((_, pan))) = (rec, panel.iter().find(|(j, _)| *j == i)) else {
            continue;
        };
        // `ControlReading` compares f32 values, so −0.0 equals 0.0 (C8).
        if rec != *pan {
            out.push(DiffRow {
                index: i,
                identifier: e.identifier,
                record: rec,
                panel: *pan,
            });
        }
    }
    Ok(Some(out))
}

/// The files **Save processing record…** writes for item `n` (D2): the engine's record, the
/// record-input file naming `record_file`, and the fed span as `.i24` when it is exact. The fed
/// span is read from `input`, which must be the run's input.
///
/// # Errors
///
/// The engine's refusal; a session input that is not the run's (in words).
pub fn files(
    device: &crate::device::Device,
    n: u32,
    input: Option<&crate::input::Input>,
    record_file: &str,
) -> Result<RecordFiles, String> {
    let record = device.processing_record(n)?;
    let host_rate = device.settings().host_rate_hz;
    let base = RecordInput {
        record_file: record_file.to_owned(),
        record_sha256: sha256(&record),
        imported: true,
        input_file: None,
        input_sha256: None,
        channel: None,
        conversion: None,
        host_rate,
        start: None,
        fed: None,
        feed_point: FeedPoint::None,
    };
    let Some(run) = device.run_of(n) else {
        return Ok(RecordFiles {
            input_text: write_record_input(&base),
            record,
            i24: None,
        });
    };
    let src = &run.source;
    let fed = run.stop.stop_input_position;
    let matching = input
        .filter(|i| src.input_sha256 == Some(i.input_sha256) && src.channel == Some(i.channel));
    let Some(inp) = matching else {
        return Err(format!(
            "The item's input is not the session's: open {} to save its record",
            if src.file_name.is_empty() {
                "the file it was recorded from"
            } else {
                &src.file_name
            }
        ));
    };
    let len = inp.samples.len();
    let start = usize::try_from(src.start).unwrap_or(len).min(len);
    let end = usize::try_from(fed)
        .map_or(len, |f| start.saturating_add(f))
        .min(len);
    let span = &inp.samples[start..end];
    let exact = feed_point_i24(span);
    let ri = RecordInput {
        imported: false,
        input_file: Some(src.file_name.clone()),
        input_sha256: src.input_sha256,
        channel: src.channel,
        conversion: Some(src.conversion),
        start: Some(src.start),
        fed: Some(fed),
        feed_point: if exact {
            FeedPoint::I24
        } else {
            FeedPoint::F32
        },
        ..base
    };
    Ok(RecordFiles {
        input_text: write_record_input(&ri),
        record,
        i24: exact.then(|| i24_bytes(span)),
    })
}

/// The two companion paths of a record path (D2): `<stem>.record-input.txt` and
/// `<stem>.record-input.i24`, where `<stem>` is the record's path without `.record.txt` (or
/// without its last extension).
pub fn companion_paths(record: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let s = record.to_string_lossy();
    let stem = s.strip_suffix(".record.txt").map_or_else(
        || record.with_extension("").to_string_lossy().into_owned(),
        str::to_owned,
    );
    (
        std::path::PathBuf::from(format!("{stem}.record-input.txt")),
        std::path::PathBuf::from(format!("{stem}.record-input.i24")),
    )
}
