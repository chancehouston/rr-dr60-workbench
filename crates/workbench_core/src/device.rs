//! The device: one persistent engine with its memory (spec 004 D1 – D6). Recording, the memory,
//! export and import, and offline playback to a take, with a playback sidecar that replays it.

use std::fmt::Write as _;

use rr_dr60::changes::{ChangeEntry, MAX_CHANGES_PER_CALL};
use rr_dr60::control_map::{self, ControlReading};
use rr_dr60::{Error, MemoryStatus, Pipeline, Settings, StopResult, Stream, VasEvent};

use crate::meters::MeterRow;
use crate::render::{Due, Scheduled, block_for};
use crate::sidecar::{fmt_reading, parse_change, parse_reading};

/// The five live playback controls whose values a playback starts from (D5).
pub const PLAY_VALUES: [&str; 5] = [
    "route",
    "volume",
    "battery_volts",
    "playback_speed",
    "playback_speed_mode",
];

fn index(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .map_or(u32::MAX, |i| i as u32)
}

/// One item of the memory (D3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Its number (1 to the count).
    pub number: u32,
    /// Its length in device samples.
    pub length: u64,
    /// Its recording counter (`None`: imported).
    pub counter: Option<u64>,
    /// Its digest (`recording_digest`).
    pub digest: u64,
}

/// How a recording ended (D2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordEnd {
    /// Stopped at the input's end and kept.
    Kept {
        /// The item's number.
        number: u32,
        /// Its length in device samples.
        length: u64,
    },
    /// The recorder stopped by itself when the memory filled; the item is kept.
    SelfStopped {
        /// The item's number.
        number: u32,
    },
    /// Nothing was kept: no item was added.
    DiscardedEmpty,
}

/// A recording's outcome: its end, the recorder's reports and log, and the input fed.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordOutcome {
    /// How it ended.
    pub end: RecordEnd,
    /// The recorder's gate reports.
    pub events: Vec<VasEvent>,
    /// The record stream's change log.
    pub log: Vec<ChangeEntry>,
    /// Host input samples fed.
    pub fed: u64,
}

/// A playback take (D4, D5).
#[derive(Clone, Debug, PartialEq)]
pub struct PlayTake {
    /// The item played.
    pub number: u32,
    /// Its digest when played.
    pub digest: u64,
    /// The device's settings immediately before `start_playback`.
    pub settings: Settings,
    /// The five playback values immediately before `start_playback` (index, reading).
    pub values: Vec<(u32, ControlReading)>,
    /// The requested changes in the playback clock (tags from 1).
    pub changes: Vec<Scheduled>,
    /// The output (the drain included).
    pub samples: Vec<f32>,
    /// The playback stream's change log.
    pub log: Vec<ChangeEntry>,
    /// The speech coder's frames decoded.
    pub frames: u64,
    /// The meter rows (one per 10 ms call).
    pub meters: Vec<MeterRow>,
}

/// The persistent device engine (D1).
pub struct Device {
    p: Pipeline,
}

impl std::fmt::Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Device")
            .field("status", &self.p.memory_status())
            .finish_non_exhaustive()
    }
}

impl Device {
    /// A device with an empty memory.
    ///
    /// # Errors
    ///
    /// The engine's refusal of the settings.
    pub fn new(settings: Settings) -> Result<Self, Error> {
        Ok(Self {
            p: Pipeline::new(settings)?,
        })
    }

    /// The engine's settings.
    pub fn settings(&self) -> Settings {
        *Pipeline::settings(&self.p)
    }

    /// Applies new settings with the engine's `reconfigure` (D1); the memory is kept or the
    /// engine refuses. Unchanged settings are a no-op.
    ///
    /// # Errors
    ///
    /// The engine's refusal (e.g. a memory that does not fit, `StoredExceedsCapacity`).
    pub fn sync(&mut self, settings: Settings) -> Result<(), Error> {
        if settings == *Pipeline::settings(&self.p) {
            return Ok(());
        }
        self.p.reconfigure(settings)
    }

    /// The memory's status.
    pub fn status(&self) -> MemoryStatus {
        self.p.memory_status()
    }

    /// The items, in order.
    pub fn items(&self) -> Vec<Item> {
        (1..=self.p.memory_status().count)
            .filter_map(|n| {
                Some(Item {
                    number: n,
                    length: self.p.recording_length(n).ok()?,
                    counter: self.p.recording_counter_of(n).ok().flatten(),
                    digest: self.p.recording_digest(n).ok()?,
                })
            })
            .collect()
    }

    /// Records `input[start .. start + len]` (D2) with record-side changes in the input clock.
    ///
    /// # Errors
    ///
    /// The engine's refusal at the start (`MemoryFull`, `NoRecordingSlots`) or of a call.
    pub fn record(
        &mut self,
        input: &[f32],
        start: usize,
        len: Option<usize>,
        changes: &[Scheduled],
    ) -> Result<RecordOutcome, Error> {
        let rate = Pipeline::settings(&self.p).host_rate_hz;
        let block = block_for(rate).max(1);
        let start = start.min(input.len());
        let end = len.map_or(input.len(), |l| (start + l).min(input.len()));
        self.p.start_recording()?;
        let mut ebuf = vec![VasEvent::new(0, 0); self.p.max_record_events(block).max(1)];
        let mut cbuf = vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL];
        let mut due = Due::new(changes);
        let mut events = Vec::new();
        let mut log = Vec::new();
        let mut fed = 0u64;
        let mut refusal = None;
        let mut pos = start;
        while pos < end {
            let n = (end - pos).min(block);
            due.feed_on(
                &mut self.p,
                Stream::Record,
                changes,
                n as u64,
                &mut |_, e| {
                    refusal.get_or_insert(e);
                },
            );
            if let Some(e) = refusal {
                let _ = self.p.stop_recording(&mut ebuf);
                return Err(e);
            }
            let info = self
                .p
                .record_with_changes(&input[pos..pos + n], &mut ebuf, &mut cbuf)?;
            events.extend_from_slice(&ebuf[..info.events.min(ebuf.len())]);
            log.extend_from_slice(&cbuf[..info.changes.min(cbuf.len())]);
            fed += n as u64;
            pos += n;
            if info.stopped.is_some() {
                // The recorder's own self-stop: the item is kept; no stop call (D2).
                return Ok(RecordOutcome {
                    end: RecordEnd::SelfStopped {
                        number: self.p.memory_status().count,
                    },
                    events,
                    log,
                    fed,
                });
            }
        }
        let out = self.p.stop_recording_with_changes(&mut ebuf, &mut cbuf)?;
        events.extend_from_slice(&ebuf[..out.flush.events.min(ebuf.len())]);
        log.extend_from_slice(&cbuf[..out.changes.min(cbuf.len())]);
        let end = match out.result {
            StopResult::Kept { number, length } => RecordEnd::Kept { number, length },
            _ => RecordEnd::DiscardedEmpty,
        };
        Ok(RecordOutcome {
            end,
            events,
            log,
            fed,
        })
    }

    /// Erases item `n` (later items renumber).
    ///
    /// # Errors
    ///
    /// The engine's refusal.
    pub fn erase(&mut self, n: u32) -> Result<(), Error> {
        Pipeline::erase(&mut self.p, n)
    }

    /// Erases every item.
    ///
    /// # Errors
    ///
    /// The engine's refusal.
    pub fn erase_all(&mut self) -> Result<(), Error> {
        Pipeline::erase_all(&mut self.p)
    }

    /// Item `n`'s stored form (D3).
    ///
    /// # Errors
    ///
    /// The engine's refusal.
    pub fn export(&self, n: u32) -> Result<Vec<u8>, Error> {
        let mut out = vec![0u8; self.p.export_len(n)?];
        let len = Pipeline::export(&self.p, n, &mut out)?;
        out.truncate(len);
        Ok(out)
    }

    /// Imports a stored form; returns its new number.
    ///
    /// # Errors
    ///
    /// The engine's refusal (version, encoding, fit).
    pub fn import(&mut self, bytes: &[u8]) -> Result<u32, Error> {
        Pipeline::import(&mut self.p, bytes)
    }

    /// The engine's figures for item `n` (D6): length, playback length per level in both modes,
    /// and the record and playback latencies.
    pub fn readings(&self, n: u32) -> Vec<(String, String)> {
        let mut v = Vec::new();
        if let Ok(l) = self.p.recording_length(n) {
            v.push((
                "length".into(),
                format!("{l} device samples ({:.3} s)", l as f64 / 6000.0),
            ));
        }
        for level in 1..=5u8 {
            let a = self
                .p
                .playback_length_samples(n, level)
                .map_or_else(|e| e.to_string(), |x| x.to_string());
            let b = self
                .p
                .playback_length_samples_in_mode(n, level, rr_dr60::speed::SpeedMode::PitchKept)
                .map_or_else(|e| e.to_string(), |x| x.to_string());
            v.push((
                format!("playback length, level {level}"),
                format!("{a} (pitch with speed), {b} (pitch kept) host samples"),
            ));
        }
        v.push((
            "record latency".into(),
            format!("{} host samples", self.p.record_latency_samples()),
        ));
        v.push((
            "playback latency".into(),
            format!("{} host samples", self.p.playback_latency_samples()),
        ));
        v.push(("state".into(), format!("{:?}", self.p.state())));
        v
    }

    /// Plays item `n` to a take (D4): the values set first by index (spec 025's plain setters),
    /// the start state read, then the playback with the changes fed just in time.
    ///
    /// # Errors
    ///
    /// The engine's refusal of a value, of the start, or of a change.
    pub fn play_to_take(
        &mut self,
        n: u32,
        values: &[(u32, ControlReading)],
        changes: &[Scheduled],
    ) -> Result<PlayTake, Error> {
        for &(i, v) in values {
            self.p.set_control_value(i, v)?;
        }
        let settings = *Pipeline::settings(&self.p);
        let start_values: Vec<(u32, ControlReading)> = PLAY_VALUES
            .iter()
            .filter_map(|id| {
                let i = index(id);
                self.p.control_value(i).ok().map(|r| (i, r))
            })
            .collect();
        let digest = self.p.recording_digest(n)?;
        self.p.start_playback(n)?;
        let block = block_for(settings.host_rate_hz).max(1);
        let mut obuf = vec![0.0f32; block];
        let mut cbuf = vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL];
        let mut due = Due::new(changes);
        let mut samples = Vec::new();
        let mut log = Vec::new();
        let mut meters = vec![MeterRow {
            meters: self.p.rest_meters(),
            in_span: (0, 0),
            out_span: (0, 0),
            tail: false,
        }];
        let mut frames = 0u64;
        let mut refusal = None;
        loop {
            due.feed_on(
                &mut self.p,
                Stream::Playback,
                changes,
                block as u64,
                &mut |_, e| {
                    refusal.get_or_insert(e);
                },
            );
            if let Some(e) = refusal {
                let _ = Pipeline::stop_playback(&mut self.p);
                return Err(e);
            }
            let info = self.p.play_with_changes(&mut obuf, &mut cbuf)?;
            let o0 = samples.len() as u64;
            samples.extend_from_slice(&obuf[..info.produced]);
            log.extend_from_slice(&cbuf[..info.changes.min(cbuf.len())]);
            frames += info.frames as u64;
            meters.push(MeterRow {
                meters: Pipeline::meters(&self.p),
                in_span: (0, 0),
                out_span: (o0, samples.len() as u64),
                tail: false,
            });
            if info.finished {
                break;
            }
        }
        Ok(PlayTake {
            number: n,
            digest,
            settings,
            values: start_values,
            changes: changes.to_vec(),
            samples,
            log,
            frames,
            meters,
        })
    }
}

/// The playback sidecar's first line (D5).
pub const PLAYBACK_MAGIC: &str = "rr-dr60-workbench playback 1";

/// Writes a playback take's sidecar (D5): the identity, the item's digest and stored form, the
/// settings and the five values at the start, the requested changes, the produced count.
pub fn playback_sidecar(take: &PlayTake, stored_file: &str, stored_sha256: &[u8; 32]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{PLAYBACK_MAGIC}");
    let _ = writeln!(s, "workbench.version {}", crate::workbench_version());
    let _ = writeln!(s, "engine.version {}", crate::engine_version());
    let _ = writeln!(s, "engine.commit {}", crate::engine_commit());
    let _ = writeln!(s, "item.number {}", take.number);
    let _ = writeln!(s, "item.digest {:016x}", take.digest);
    let _ = writeln!(s, "stored.file {}", stored_file.replace('\n', " "));
    let _ = writeln!(s, "stored.sha256 {}", crate::input::hex(stored_sha256));
    let _ = writeln!(s, "produced {}", take.samples.len());
    let _ = writeln!(
        s,
        "output.sha256 {}",
        crate::input::hex(&crate::input::samples_sha256(&take.samples))
    );
    for (i, e) in rr_dr60::controls::controls().iter().enumerate() {
        if let Ok(r) = control_map::settings_value(&take.settings, i as u32) {
            let _ = writeln!(s, "setting {} {}", e.identifier, fmt_reading(r));
        }
    }
    for &(i, r) in &take.values {
        let id = rr_dr60::controls::controls()[i as usize].identifier;
        let _ = writeln!(s, "value {id} {}", fmt_reading(r));
    }
    for c in &take.changes {
        let id = rr_dr60::controls::controls()
            .get(c.control as usize)
            .map_or("unknown", |e| e.identifier);
        let _ = writeln!(
            s,
            "change out {} {id} {} 0x{:016x} {}",
            c.position,
            c.value,
            c.value.to_bits(),
            c.tag
        );
    }
    s
}

/// A parsed playback sidecar.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackSidecar {
    /// The settings at the start.
    pub settings: Settings,
    /// The five values at the start.
    pub values: Vec<(u32, ControlReading)>,
    /// The requested changes.
    pub changes: Vec<Scheduled>,
    /// The item's digest.
    pub digest: u64,
    /// The stored form's SHA-256 (hex).
    pub stored_sha256: String,
    /// The produced count.
    pub produced: u64,
}

/// Reads a playback sidecar.
///
/// # Errors
///
/// A malformed line (its number and the reason) or an unknown first line.
pub fn read_playback(text: &str) -> Result<PlaybackSidecar, String> {
    let mut lines = text.lines();
    if lines.next() != Some(PLAYBACK_MAGIC) {
        return Err("not a playback sidecar".into());
    }
    let mut setting_lines: Vec<(String, String)> = Vec::new();
    let mut values = Vec::new();
    let mut changes = Vec::new();
    let (mut digest, mut stored, mut produced, mut rate) = (None, None, None, None);
    for (n, line) in lines.enumerate() {
        let n = n + 2;
        let Some((key, rest)) = line.split_once(' ') else {
            continue;
        };
        match key {
            "setting" => {
                let (id, v) = rest
                    .split_once(' ')
                    .ok_or(format!("line {n}: a setting needs a value"))?;
                if id == "host_rate_hz" {
                    rate = v.parse::<u32>().ok();
                }
                setting_lines.push((id.to_owned(), v.to_owned()));
            }
            "value" => {
                let (id, v) = rest
                    .split_once(' ')
                    .ok_or(format!("line {n}: a value needs a value"))?;
                let i = index(id);
                let e = rr_dr60::controls::controls()
                    .get(i as usize)
                    .ok_or(format!("line {n}: unknown control {id}"))?;
                values.push((
                    i,
                    parse_reading(v, &e.value).map_err(|m| format!("line {n}: {m}"))?,
                ));
            }
            "change" => {
                let (clock, position, id, value, tag) =
                    parse_change(rest).map_err(|m| format!("line {n}: {m}"))?;
                changes.push(Scheduled {
                    clock,
                    position,
                    control: index(&id),
                    value,
                    tag,
                });
            }
            "item.digest" => digest = u64::from_str_radix(rest, 16).ok(),
            "stored.sha256" => stored = Some(rest.to_owned()),
            "produced" => produced = rest.parse().ok(),
            _ => {}
        }
    }
    let rate = rate.ok_or("no host rate")?;
    let mut settings = Settings::new(rate);
    for (id, v) in setting_lines {
        let i = index(&id);
        let e = rr_dr60::controls::controls()
            .get(i as usize)
            .ok_or(format!("unknown setting {id}"))?;
        let r = parse_reading(&v, &e.value)?;
        control_map::set_settings_value(&mut settings, i, r).map_err(|e| e.to_string())?;
    }
    Ok(PlaybackSidecar {
        settings,
        values,
        changes,
        digest: digest.ok_or("no item digest")?,
        stored_sha256: stored.ok_or("no stored form digest")?,
        produced: produced.ok_or("no produced count")?,
    })
}

/// Replays a playback sidecar against its stored form (D5): a fresh device from the recorded
/// settings, the import, the digest check, the values, the playback with its changes.
///
/// # Errors
///
/// The engine's refusals, or a digest that does not match.
pub fn replay(s: &PlaybackSidecar, stored: &[u8]) -> Result<PlayTake, String> {
    let mut d = Device::new(s.settings).map_err(|e| e.to_string())?;
    let n = d.import(stored).map_err(|e| e.to_string())?;
    let digest = d.p.recording_digest(n).map_err(|e| e.to_string())?;
    if digest != s.digest {
        return Err(format!(
            "the stored form's digest {digest:016x} is not the sidecar's {:016x}",
            s.digest
        ));
    }
    let take = d
        .play_to_take(n, &s.values, &s.changes)
        .map_err(|e| e.to_string())?;
    Ok(take)
}
