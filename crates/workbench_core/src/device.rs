//! The device: one persistent engine with its memory (spec 004 D1 – D6). Recording, the memory,
//! export and import, and offline playback to a take, with a playback sidecar that replays it.

use std::fmt::Write as _;

use rr_dr60::changes::{ChangeEntry, Clock, MAX_CHANGES_PER_CALL};
use rr_dr60::control_map::{self, ControlReading};
use rr_dr60::{Error, MemoryStatus, Pipeline, Settings, StopResult, Stream, VasEvent};
use sha2::{Digest, Sha256};

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

fn index(id: &str) -> Option<u32> {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .map(|i| i as u32)
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
        /// Its length in device samples.
        length: u64,
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
    /// Host input samples the workbench fed: the stop's `stop_input_position` (its own
    /// silence excluded), or `input_samples_total` at a self-stop.
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
    /// The place reached (engine spec 013): the recording's length once the playback has
    /// reached its end.
    pub place: u64,
    /// The meter rows (one per 10 ms call; output spans only).
    pub meters: Vec<MeterRow>,
    /// A live take stopped before the playback finished (spec 004b D5): the samples are a
    /// prefix of the full playback.
    pub stopped_early: bool,
    /// A live take whose capture overflowed: it does not reproduce and is not exported
    /// (spec 004b review B1).
    pub incomplete: bool,
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

/// Refuses a change whose clock is not the stream's (post-implementation review B6).
fn check_clocks(changes: &[Scheduled], clock: Clock) -> Result<(), Error> {
    for c in changes {
        if c.clock != clock {
            return Err(Error::NoSuchClock {
                stream: if clock == Clock::Input {
                    Stream::Record
                } else {
                    Stream::Playback
                },
            });
        }
    }
    Ok(())
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

    /// The engine's state.
    pub fn state(&self) -> rr_dr60::EngineState {
        Pipeline::state(&self.p)
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

    /// Undoes a recording that failed part-way: stop, and erase the item a stop kept (B1).
    fn abandon_recording(&mut self, ebuf: &mut [VasEvent]) {
        if let Ok(out) = self.p.stop_recording(ebuf)
            && let StopResult::Kept { number, .. } = out.result
        {
            let _ = Pipeline::erase(&mut self.p, number);
        }
    }

    /// Records `input[start .. start + len]` (D2) with record-side changes in the input clock
    /// from the record's start. On any error the memory and the state are as before (B1).
    ///
    /// # Errors
    ///
    /// The engine's refusal at the start (`MemoryFull`, `NoRecordingSlots`), of a change, or of a
    /// call; a change in the output clock.
    pub fn record(
        &mut self,
        input: &[f32],
        start: usize,
        len: Option<usize>,
        changes: &[Scheduled],
    ) -> Result<RecordOutcome, Error> {
        check_clocks(changes, Clock::Input)?;
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
        let mut pos = start;
        while pos < end {
            let n = (end - pos).min(block);
            let mut refusal = None;
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
                self.abandon_recording(&mut ebuf);
                return Err(e);
            }
            let info = match self
                .p
                .record_with_changes(&input[pos..pos + n], &mut ebuf, &mut cbuf)
            {
                Ok(i) => i,
                Err(e) => {
                    self.abandon_recording(&mut ebuf);
                    return Err(e);
                }
            };
            events.extend_from_slice(&ebuf[..info.events.min(ebuf.len())]);
            log.extend_from_slice(&cbuf[..info.changes.min(cbuf.len())]);
            pos += n;
            if info.stopped.is_some() {
                // The recorder's own self-stop: the item is kept; no stop call (D2).
                let number = self.p.memory_status().count;
                return Ok(RecordOutcome {
                    end: RecordEnd::SelfStopped {
                        number,
                        length: self.p.recording_length(number).unwrap_or(0),
                    },
                    events,
                    log,
                    fed: info.input_samples_total,
                });
            }
        }
        let out = match self.p.stop_recording_with_changes(&mut ebuf, &mut cbuf) {
            Ok(o) => o,
            Err(e) => {
                self.abandon_recording(&mut ebuf);
                return Err(e);
            }
        };
        events.extend_from_slice(&ebuf[..out.flush.events.min(ebuf.len())]);
        log.extend_from_slice(&cbuf[..out.changes.min(cbuf.len())]);
        // The input the workbench fed, the stop's own silence excluded (engine 020 D9).
        let fed = out.stop_input_position;
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

    /// The stored form of the item a take played, refused when that number now holds another
    /// item (after an erase renumbered them; B4).
    ///
    /// # Errors
    ///
    /// The engine's refusal, or `NoSuchRecording` when the digest moved.
    pub fn export_take(&self, take: &PlayTake) -> Result<Vec<u8>, Error> {
        if take.incomplete {
            // An incomplete live take does not reproduce (spec 004b review B1).
            return Err(Error::NoSuchRecording {
                number: take.number,
            });
        }
        if self.p.recording_digest(take.number)? != take.digest {
            return Err(Error::NoSuchRecording {
                number: take.number,
            });
        }
        self.export(take.number)
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
        v.push(("state".into(), format!("{:?}", Pipeline::state(&self.p))));
        v
    }

    /// Plays item `n` to a take (D4): the values set first by index (spec 025's plain setters;
    /// all restored if one is refused), the start state read, then the playback with the
    /// changes fed just in time. On any error the playback is stopped (B1).
    ///
    /// # Errors
    ///
    /// The engine's refusal of a value, of the start, or of a change; a change in the input
    /// clock.
    pub fn play_to_take(
        &mut self,
        n: u32,
        values: &[(u32, ControlReading)],
        changes: &[Scheduled],
    ) -> Result<PlayTake, Error> {
        // On a clone, as the live playback (spec 004b review B5): a landed stage switch does
        // not carry into the device's settings, so both modes leave the device the same.
        self.play_limited(n, values, changes, None)
    }

    /// As [`Device::play_to_take`], stopping once `limit` samples are produced (a prefix
    /// replay, spec 004b review B7).
    fn play_limited(
        &mut self,
        n: u32,
        values: &[(u32, ControlReading)],
        changes: &[Scheduled],
        limit: Option<u64>,
    ) -> Result<PlayTake, Error> {
        check_clocks(changes, Clock::Output)?;
        let start = self.prepare_play(n, values)?;
        let mut c = self.p.clone();
        c.start_playback(n)?;
        Self::play_on(&mut c, start, changes, limit)
    }

    /// Sets the five values (all restored if one is refused) and reads the start state: the
    /// settings, the values and the item's digest (D4, D5).
    fn prepare_play(
        &mut self,
        n: u32,
        values: &[(u32, ControlReading)],
    ) -> Result<crate::live_play::Start, Error> {
        let before: Vec<(u32, ControlReading)> = values
            .iter()
            .filter_map(|&(i, _)| self.p.control_value(i).ok().map(|r| (i, r)))
            .collect();
        for &(i, v) in values {
            if let Err(e) = self.p.set_control_value(i, v) {
                for &(j, r) in &before {
                    let _ = self.p.set_control_value(j, r);
                }
                return Err(e);
            }
        }
        let settings = *Pipeline::settings(&self.p);
        let start_values: Vec<(u32, ControlReading)> = PLAY_VALUES
            .iter()
            .filter_map(|id| {
                let i = index(id)?;
                self.p.control_value(i).ok().map(|r| (i, r))
            })
            .collect();
        let digest = self.p.recording_digest(n)?;
        Ok(crate::live_play::Start {
            number: n,
            digest,
            settings,
            values: start_values,
        })
    }

    /// Plays a started playback on `p` to its end (D4).
    fn play_on(
        p: &mut Pipeline,
        start: crate::live_play::Start,
        changes: &[Scheduled],
        limit: Option<u64>,
    ) -> Result<PlayTake, Error> {
        let crate::live_play::Start {
            number: n,
            digest,
            settings,
            values: start_values,
        } = start;
        let block = block_for(settings.host_rate_hz).max(1);
        let mut obuf = vec![0.0f32; block];
        let mut cbuf = vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL];
        let mut due = Due::new(changes);
        let mut samples = Vec::new();
        let mut log = Vec::new();
        let mut meters = vec![MeterRow {
            meters: p.rest_meters(),
            in_span: (0, 0),
            out_span: (0, 0),
            tail: false,
        }];
        let mut frames = 0u64;
        let mut place;
        loop {
            let mut refusal = None;
            due.feed_on(p, Stream::Playback, changes, block as u64, &mut |_, e| {
                refusal.get_or_insert(e);
            });
            if let Some(e) = refusal {
                let _ = Pipeline::stop_playback(p);
                return Err(e);
            }
            let info = match p.play_with_changes(&mut obuf, &mut cbuf) {
                Ok(i) => i,
                Err(e) => {
                    let _ = Pipeline::stop_playback(p);
                    return Err(e);
                }
            };
            let o0 = samples.len() as u64;
            samples.extend_from_slice(&obuf[..info.produced]);
            log.extend_from_slice(&cbuf[..info.changes.min(cbuf.len())]);
            frames += info.frames as u64;
            place = info.place;
            meters.push(MeterRow {
                meters: Pipeline::meters(p),
                in_span: (0, 0),
                out_span: (o0, samples.len() as u64),
                tail: false,
            });
            if info.finished || limit.is_some_and(|k| samples.len() as u64 >= k) {
                // A prefix replay stops once it has the prefix (spec 004b review B7).
                let _ = Pipeline::stop_playback(p);
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
            place,
            meters,
            stopped_early: false,
            incomplete: false,
        })
    }
}

impl Device {
    /// A live player for item `n` (spec 004b D1): the values set on the device, the start
    /// state read, and the playback started on a clone of the engine, which the output
    /// callback then owns. The device itself does not play.
    ///
    /// # Errors
    ///
    /// The engine's refusal of a value, of the start, or a change in the input clock.
    pub fn live_player(
        &mut self,
        n: u32,
        values: &[(u32, ControlReading)],
        changes: &[Scheduled],
    ) -> Result<(crate::live_play::LivePlayer, crate::live_play::PlayQueues), Error> {
        check_clocks(changes, Clock::Output)?;
        let start = self.prepare_play(n, values)?;
        let mut clone = self.p.clone();
        clone.start_playback(n)?;
        Ok(crate::live_play::LivePlayer::new(clone, start, changes))
    }
}

/// The playback values and changes for a take, from the panel and the schedule list (B9): the
/// five playback values from the panel's rows; the list's playback-path entries in the output
/// clock at `rate`, sorted by position, tagged from 1.
pub fn play_request(
    panel: &crate::panel::Panel,
    schedule: &[(u32, f64, f64)],
    rate: u32,
) -> (Vec<(u32, ControlReading)>, Vec<Scheduled>) {
    let values = PLAY_VALUES
        .iter()
        .filter_map(|id| panel.row(id).map(|r| (r.index, r.value)))
        .collect();
    let mut changes: Vec<Scheduled> = schedule
        .iter()
        .filter(|(c, _, _)| {
            rr_dr60::controls::controls()
                .get(*c as usize)
                .is_some_and(|e| e.paths.contains(rr_dr60::controls::ControlPaths::PLAYBACK))
                && rr_dr60::changes::clock_of(*c).ok() == Some(Clock::Output)
        })
        .map(|&(control, value, t)| Scheduled {
            clock: Clock::Output,
            position: (t * f64::from(rate)).round() as u64,
            control,
            value,
            tag: 0,
        })
        .collect();
    changes.sort_by_key(|c| c.position);
    for (k, c) in changes.iter_mut().enumerate() {
        c.tag = k as u64 + 1;
    }
    (values, changes)
}

/// The record-side changes for a recording, from the schedule list (D2): the input-clock entries
/// at `rate` from the record's start, sorted, tagged from 1.
pub fn record_request(schedule: &[(u32, f64, f64)], rate: u32) -> Vec<Scheduled> {
    let mut changes: Vec<Scheduled> = schedule
        .iter()
        .filter(|(c, _, _)| rr_dr60::changes::clock_of(*c).ok() == Some(Clock::Input))
        .map(|&(control, value, t)| Scheduled {
            clock: Clock::Input,
            position: (t * f64::from(rate)).round() as u64,
            control,
            value,
            tag: 0,
        })
        .collect();
    changes.sort_by_key(|c| c.position);
    for (k, c) in changes.iter_mut().enumerate() {
        c.tag = k as u64 + 1;
    }
    changes
}

/// The playback sidecar's first line (D5).
pub const PLAYBACK_MAGIC: &str = "rr-dr60-workbench playback 1";

const KEYS: [&str; 9] = [
    "workbench.version",
    "engine.version",
    "engine.commit",
    "item.number",
    "item.digest",
    "stored.file",
    "stored.sha256",
    "produced",
    "output.sha256",
];

/// Writes a playback take's sidecar (D5): the identity, the item's digest and stored form, the
/// settings and the five values at the start, the requested changes, the produced count and
/// the output's digest.
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
    if take.stopped_early {
        // A live take stopped before the end (spec 004b D5): a replay compares the prefix.
        let _ = writeln!(s, "stopped early");
    }
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
    /// The stored form's file name, as written.
    pub stored_file: String,
    /// The stored form's SHA-256 (hex).
    pub stored_sha256: String,
    /// The produced count.
    pub produced: u64,
    /// The output's SHA-256 (hex).
    pub output_sha256: String,
    /// A live take stopped before the end: the produced samples are a prefix (spec 004b D5).
    pub stopped_early: bool,
}

/// Reads a playback sidecar strictly (B5): blank and `#` lines aside, every line is known, no
/// key repeats, every settings field and all five values are present, every control is known.
///
/// # Errors
///
/// The line and the reason.
pub fn read_playback(text: &str) -> Result<PlaybackSidecar, String> {
    let mut lines = text.lines().enumerate();
    if lines.next().map(|(_, l)| l) != Some(PLAYBACK_MAGIC) {
        return Err("not a playback sidecar".into());
    }
    let mut setting_lines: Vec<(u32, String, String)> = Vec::new();
    let mut values: Vec<(u32, ControlReading)> = Vec::new();
    let mut changes = Vec::new();
    let mut stopped_early = false;
    let mut fields: std::collections::BTreeMap<&'static str, String> =
        std::collections::BTreeMap::new();
    for (n, line) in lines {
        let n = n + 1;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = line.split_once(' ').ok_or(format!("line {n}: no value"))?;
        let control = |id: &str| index(id).ok_or(format!("line {n}: unknown control {id}"));
        match key {
            "setting" => {
                let (id, v) = rest
                    .split_once(' ')
                    .ok_or(format!("line {n}: a setting needs a value"))?;
                let i = control(id)?;
                if setting_lines.iter().any(|(j, _, _)| *j == i) {
                    return Err(format!("line {n}: setting {id} repeated"));
                }
                setting_lines.push((i, id.to_owned(), v.to_owned()));
            }
            "value" => {
                let (id, v) = rest
                    .split_once(' ')
                    .ok_or(format!("line {n}: a value needs a value"))?;
                let i = control(id)?;
                if values.iter().any(|(j, _)| *j == i) {
                    return Err(format!("line {n}: value {id} repeated"));
                }
                let e = &rr_dr60::controls::controls()[i as usize];
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
                    control: control(&id)?,
                    value,
                    tag,
                });
            }
            "stopped" => {
                if rest != "early" || stopped_early {
                    return Err(format!(
                        "line {n}: only one \"stopped early\" line is allowed"
                    ));
                }
                stopped_early = true;
            }
            k => {
                let Some(known) = KEYS.iter().find(|x| **x == k) else {
                    return Err(format!("line {n}: unknown key {k}"));
                };
                if fields.insert(known, rest.to_owned()).is_some() {
                    return Err(format!("line {n}: {k} repeated"));
                }
            }
        }
    }
    for k in KEYS {
        if !fields.contains_key(k) {
            return Err(format!("the playback sidecar lacks {k}"));
        }
    }
    for id in PLAY_VALUES {
        if !values.iter().any(|(i, _)| Some(*i) == index(id)) {
            return Err(format!("the playback sidecar lacks the value {id}"));
        }
    }
    let rate_line = setting_lines
        .iter()
        .find(|(_, id, _)| id == "host_rate_hz")
        .ok_or("no host rate")?;
    let rate: u32 = rate_line.2.parse().map_err(|_| "a bad host rate")?;
    let mut settings = Settings::new(rate);
    for i in 0..rr_dr60::controls::controls().len() as u32 {
        if control_map::settings_value(&settings, i).is_ok()
            && !setting_lines.iter().any(|(j, _, _)| *j == i)
        {
            return Err(format!(
                "the playback sidecar lacks the setting {}",
                rr_dr60::controls::controls()[i as usize].identifier
            ));
        }
    }
    for (i, _, v) in setting_lines {
        let e = &rr_dr60::controls::controls()[i as usize];
        let r = parse_reading(&v, &e.value)?;
        control_map::set_settings_value(&mut settings, i, r).map_err(|e| e.to_string())?;
    }
    Ok(PlaybackSidecar {
        settings,
        values,
        changes,
        digest: u64::from_str_radix(&fields["item.digest"], 16).map_err(|_| "a bad item digest")?,
        stored_file: fields["stored.file"].clone(),
        stored_sha256: fields["stored.sha256"].clone(),
        produced: fields["produced"]
            .parse()
            .map_err(|_| "a bad produced count")?,
        output_sha256: fields["output.sha256"].clone(),
        stopped_early,
    })
}

/// What a replay checked (B2).
#[derive(Clone, Debug, PartialEq)]
pub struct ReplayReport {
    /// The replayed take.
    pub take: PlayTake,
    /// The stored form's SHA-256 equals the sidecar's.
    pub stored_matches: bool,
    /// The produced count equals the sidecar's.
    pub produced_matches: bool,
    /// The output's SHA-256 equals the sidecar's.
    pub output_matches: bool,
}

impl ReplayReport {
    /// Every check holds.
    pub fn reproduces(&self) -> bool {
        self.stored_matches && self.produced_matches && self.output_matches
    }
}

/// Replays a playback sidecar against its stored form (D5): the stored form's digest checked, a
/// fresh device from the recorded settings, the import, the item digest checked, the values,
/// the playback with its changes, then the length and output digest compared.
///
/// # Errors
///
/// The engine's refusals, or an item digest that does not match.
pub fn replay(s: &PlaybackSidecar, stored: &[u8]) -> Result<ReplayReport, String> {
    let sha: [u8; 32] = Sha256::digest(stored).into();
    let stored_matches = crate::input::hex(&sha) == s.stored_sha256;
    let mut d = Device::new(s.settings).map_err(|e| e.to_string())?;
    let n = d.import(stored).map_err(|e| e.to_string())?;
    let digest = d.p.recording_digest(n).map_err(|e| e.to_string())?;
    if digest != s.digest {
        return Err(format!(
            "the stored form's item digest {digest:016x} is not the sidecar's {:016x}",
            s.digest
        ));
    }
    let limit = s.stopped_early.then_some(s.produced);
    let mut take = d
        .play_limited(n, &s.values, &s.changes, limit)
        .map_err(|e| e.to_string())?;
    if s.stopped_early && take.samples.len() as u64 > s.produced {
        // The prefix the live take produced (spec 004b D5).
        let k = s.produced;
        take.samples.truncate(k as usize);
        take.log.retain(|e| e.position < k);
        take.meters.retain(|r| r.out_span.1 <= k);
        take.stopped_early = true;
    }
    let produced_matches = take.samples.len() as u64 == s.produced;
    let output_matches =
        crate::input::hex(&crate::input::samples_sha256(&take.samples)) == s.output_sha256;
    Ok(ReplayReport {
        take,
        stored_matches,
        produced_matches,
        output_matches,
    })
}
