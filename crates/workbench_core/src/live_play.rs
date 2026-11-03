//! Live playback (spec 004b): a clone of the device engine plays an item inside the output
//! callback, with the playback controls changed while it plays.
//!
//! [`LivePlayer::fill`] is the callback's body. It plays in whole 10 ms calls (004's block) into
//! a FIFO, feeds the schedule list just in time through the same [`Due`] the offline take uses,
//! schedules the user's commands at the stream's current position, and sends every sample, log
//! entry, meter row and scheduled change to the UI. It allocates nothing, locks nothing and does
//! no I/O.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rr_dr60::changes::{ChangeEntry, Clock, MAX_CHANGES_PER_CALL};
use rr_dr60::control_map::ControlReading;
use rr_dr60::{Pipeline, Settings, Stream};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::device::PlayTake;
use crate::live::{Command, MAX_PART, Reply};
use crate::meters::MeterRow;
use crate::render::{Due, Scheduled, block_for};

/// The state read before `start_playback` (D1), carried to the take.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    /// The item played.
    pub number: u32,
    /// Its digest at the start.
    pub digest: u64,
    /// The device's settings.
    pub settings: Settings,
    /// The five playback values.
    pub values: Vec<(u32, ControlReading)>,
}

/// The counters the UI reads (atomics only).
#[derive(Debug, Default)]
pub struct PlayState {
    /// Samples the player produced.
    pub produced: AtomicU64,
    /// The speech coder's frames decoded.
    pub frames: AtomicU64,
    /// The place reached.
    pub place: AtomicU64,
    /// The playback finished (the drain included).
    pub finished: AtomicBool,
    /// Finished, and the FIFO is empty: every sample has been played.
    pub drained: AtomicBool,
    /// A capture queue overflowed: the take does not reproduce (D6).
    pub incomplete: AtomicBool,
    /// An engine call failed: the run stopped.
    pub failed: AtomicBool,
    /// Replies lost to a full reply queue (they do not affect the take).
    pub replies_lost: AtomicU64,
}

/// The UI's ends of the queues.
pub struct PlayQueues {
    /// Commands to the callback.
    pub commands: Producer<Command>,
    /// Their results.
    pub replies: Consumer<Reply>,
    /// The player's output samples.
    pub samples: Consumer<f32>,
    /// Change-log entries.
    pub log: Consumer<ChangeEntry>,
    /// Meter rows.
    pub rows: Consumer<MeterRow>,
    /// Every change the engine accepted, in the order it received it (D4).
    pub scheduled: Consumer<Scheduled>,
    /// The counters.
    pub state: Arc<PlayState>,
    /// The start state.
    pub start: Start,
}

/// The audio thread's player.
pub struct LivePlayer {
    p: Pipeline,
    block: usize,
    fifo: VecDeque<f32>,
    obuf: Vec<f32>,
    cbuf: Vec<ChangeEntry>,
    list: Vec<Scheduled>,
    due: Due,
    next_tag: u64,
    commands: Consumer<Command>,
    replies: Producer<Reply>,
    samples: Producer<f32>,
    log: Producer<ChangeEntry>,
    rows: Producer<MeterRow>,
    scheduled: Producer<Scheduled>,
    state: Arc<PlayState>,
    produced: u64,
    frames: u64,
    finished: bool,
    failed: bool,
}

impl LivePlayer {
    /// The player for `p`, whose playback has started, with the schedule list `list` (sorted
    /// by position, tagged 1 … k; commands continue from k + 1).
    pub fn new(p: Pipeline, start: Start, list: &[Scheduled]) -> (Self, PlayQueues) {
        let rate = start.settings.host_rate_hz;
        let block = block_for(rate).max(1);
        // About 10 s of UI stall before a capture queue overflows (post-implementation review B1).
        let chunks = 10 * (rate as usize).div_ceil(block) + 2 * MAX_PART.div_ceil(block) + 2;
        let (cp, cc) = RingBuffer::new(256);
        let (rp, rc) = RingBuffer::new(256);
        let (sp, sc) = RingBuffer::new(10 * rate as usize + 2 * MAX_PART + 2 * block);
        let (lp, lc) = RingBuffer::new(chunks * 8 + MAX_CHANGES_PER_CALL);
        let (mp, mc) = RingBuffer::new(chunks + 1);
        let (kp, kc) = RingBuffer::new(list.len() + 4096);
        let state = Arc::new(PlayState::default());
        let next_tag = list.iter().map(|c| c.tag).max().unwrap_or(0) + 1;
        let rest = MeterRow {
            meters: p.rest_meters(),
            in_span: (0, 0),
            out_span: (0, 0),
            tail: false,
        };
        let mut player = Self {
            due: Due::new(list),
            p,
            block,
            fifo: VecDeque::with_capacity(MAX_PART + 2 * block),
            obuf: vec![0.0; block],
            cbuf: vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL],
            list: list.to_vec(),
            next_tag,
            commands: cc,
            replies: rp,
            samples: sp,
            log: lp,
            rows: mp,
            scheduled: kp,
            state: state.clone(),
            produced: 0,
            frames: 0,
            finished: false,
            failed: false,
        };
        if player.rows.push(rest).is_err() {
            player.mark_incomplete();
        }
        (
            player,
            PlayQueues {
                commands: cp,
                replies: rc,
                samples: sc,
                log: lc,
                rows: mc,
                scheduled: kc,
                state,
                start,
            },
        )
    }

    fn mark_incomplete(&self) {
        self.state.incomplete.store(true, Ordering::Relaxed);
    }

    fn reply(&mut self, tag: u64, control: u32, error: Option<rr_dr60::Error>) {
        if self
            .replies
            .push(Reply {
                tag,
                control,
                error,
            })
            .is_err()
        {
            self.state.replies_lost.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Schedules the waiting commands at the stream's current position (D2).
    fn take_commands(&mut self) {
        while let Ok(c) = self.commands.pop() {
            let tag = self.next_tag;
            self.next_tag += 1;
            if self.finished {
                // The playback has finished: a change could never land (B3).
                self.reply(tag, c.control, Some(rr_dr60::Error::NotPlaying));
                continue;
            }
            let r = self.p.stream_position(Stream::Playback).and_then(|at| {
                self.p
                    .schedule_change(Stream::Playback, at, c.control, c.value, tag)
                    .map(|()| at)
            });
            match r {
                Ok(position) => {
                    let s = Scheduled {
                        clock: Clock::Output,
                        position,
                        control: c.control,
                        value: c.value,
                        tag,
                    };
                    if self.scheduled.push(s).is_err() {
                        self.mark_incomplete();
                    }
                    self.reply(tag, c.control, None);
                }
                Err(e) => self.reply(tag, c.control, Some(e)),
            }
        }
    }

    /// Plays one 10 ms call and sends its results.
    fn chunk(&mut self) {
        let mut overflow = false;
        {
            let scheduled = &mut self.scheduled;
            let replies = &mut self.replies;
            let state = &self.state;
            self.due.feed_on_with(
                &mut self.p,
                Stream::Playback,
                &self.list,
                self.block as u64,
                &mut |c, r| match r {
                    Ok(()) => {
                        if scheduled.push(*c).is_err() {
                            overflow = true;
                        }
                    }
                    Err(e) => {
                        // A list entry the engine refuses: reported, each one (B8); the take
                        // keeps only what was accepted.
                        let r = Reply {
                            tag: c.tag,
                            control: c.control,
                            error: Some(e),
                        };
                        if replies.push(r).is_err() {
                            state.replies_lost.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
            );
        }
        if overflow {
            self.mark_incomplete();
        }
        let info = match self
            .p
            .play_with_changes(&mut self.obuf[..self.block], &mut self.cbuf)
        {
            Ok(i) => i,
            Err(e) => {
                self.mark_incomplete();
                self.failed = true;
                self.state.failed.store(true, Ordering::Relaxed);
                self.reply(0, 0, Some(e));
                return;
            }
        };
        let produced = info.produced.min(self.block);
        if info.changes > self.cbuf.len() {
            self.mark_incomplete();
        }
        let out0 = self.produced;
        for &v in &self.obuf[..produced] {
            self.fifo.push_back(v);
            if self.samples.push(v).is_err() {
                self.mark_incomplete();
            }
        }
        self.produced += produced as u64;
        self.frames += info.frames as u64;
        for &c in &self.cbuf[..info.changes.min(self.cbuf.len())] {
            if self.log.push(c).is_err() {
                self.mark_incomplete();
            }
        }
        let row = MeterRow {
            meters: Pipeline::meters(&self.p),
            in_span: (0, 0),
            out_span: (out0, self.produced),
            tail: false,
        };
        if self.rows.push(row).is_err() {
            self.mark_incomplete();
        }
        self.state.produced.store(self.produced, Ordering::Relaxed);
        self.state.frames.store(self.frames, Ordering::Relaxed);
        self.state.place.store(info.place, Ordering::Relaxed);
        if info.finished {
            self.finished = true;
            self.state.finished.store(true, Ordering::Relaxed);
        }
    }

    /// The callback's body: fills `out` (mono) from the player (D1).
    pub fn fill(&mut self, out: &mut [f32]) {
        for part in out.chunks_mut(MAX_PART) {
            self.take_commands();
            while self.fifo.len() < part.len() && !self.finished && !self.failed {
                self.chunk();
            }
            let have = self.fifo.len().min(part.len());
            for v in part.iter_mut().take(have) {
                *v = self.fifo.pop_front().unwrap_or(0.0);
            }
            part[have..].fill(0.0);
            if (self.finished || self.failed) && self.fifo.is_empty() {
                self.state.drained.store(true, Ordering::Relaxed);
            }
        }
    }

    /// The FIFO's samples (the tests' bound check).
    pub fn fifo_len(&self) -> usize {
        self.fifo.len()
    }
}

/// The take, assembled on the UI thread from the queues (D4).
#[derive(Debug, Default)]
pub struct PlayTakeBuilder {
    /// The player's output.
    pub samples: Vec<f32>,
    /// The change log.
    pub log: Vec<ChangeEntry>,
    /// The meter rows.
    pub rows: Vec<MeterRow>,
    /// Every accepted change, in scheduling order.
    pub scheduled: Vec<Scheduled>,
    /// The commands' results.
    pub replies: Vec<Reply>,
}

impl PlayTakeBuilder {
    /// Moves everything the queues hold into the take.
    pub fn drain(&mut self, q: &mut PlayQueues) {
        while let Ok(v) = q.samples.pop() {
            self.samples.push(v);
        }
        while let Ok(v) = q.log.pop() {
            self.log.push(v);
        }
        while let Ok(v) = q.rows.pop() {
            self.rows.push(v);
        }
        while let Ok(v) = q.scheduled.pop() {
            self.scheduled.push(v);
        }
    }

    /// Moves the waiting replies into the take and returns them (B2: the UI shows the
    /// refusals as they arrive; the drain leaves them to this call).
    pub fn take_replies(&mut self, q: &mut PlayQueues) -> &[Reply] {
        let n = self.replies.len();
        while let Ok(v) = q.replies.pop() {
            self.replies.push(v);
        }
        &self.replies[n..]
    }

    /// The playback take (D4): the start state, the changes in scheduling order (sorted by
    /// position, stable, so a replay's `Due` feeds same-position changes in the order the live
    /// engine received them), and "stopped early" unless the playback finished.
    pub fn finish(self, q: &PlayQueues) -> PlayTake {
        let mut changes = self.scheduled;
        // The engine receives commands at a call's boundary before that call's list entries,
        // so the scheduling order is already position order (B6); the stable sort is a guard.
        debug_assert!(changes.is_sorted_by_key(|c| c.position));
        changes.sort_by_key(|c| c.position);
        PlayTake {
            number: q.start.number,
            digest: q.start.digest,
            settings: q.start.settings,
            values: q.start.values.clone(),
            changes,
            samples: self.samples,
            log: self.log,
            frames: q.state.frames.load(Ordering::Relaxed),
            place: q.state.place.load(Ordering::Relaxed),
            meters: self.rows,
            stopped_early: !q.state.finished.load(Ordering::Relaxed),
            incomplete: q.state.incomplete.load(Ordering::Relaxed),
        }
    }
}

/// The controls a live playback changes at once (D2): the five plain live values and the
/// stage switches 10 – 13.
pub const LIVE_PLAY_CONTROLS: [&str; 9] = [
    "route",
    "volume",
    "battery_volts",
    "playback_speed",
    "playback_speed_mode",
    "playback_stage_enabled",
    "output.volume_amp_enabled",
    "output.speaker_enabled",
    "output.earphone_enabled",
];

/// Each live playback control's reading at the start (D2): the five values from the start
/// state, the stage switches from its settings. A control the engine does not name is left out.
pub fn readings_at_start(start: &Start) -> Vec<(u32, ControlReading)> {
    LIVE_PLAY_CONTROLS
        .iter()
        .filter_map(|id| {
            let i = rr_dr60::controls::controls()
                .iter()
                .position(|e| e.identifier == *id)? as u32;
            start
                .values
                .iter()
                .find(|(j, _)| *j == i)
                .map(|&(_, r)| r)
                .or_else(|| rr_dr60::control_map::settings_value(&start.settings, i).ok())
                .map(|r| (i, r))
        })
        .collect()
}

/// A change entry's `after` as the control's reading (B9: the widgets follow the log).
pub fn reading_of(control: u32, after: f64) -> ControlReading {
    use rr_dr60::controls::ControlValue;
    match rr_dr60::controls::controls()
        .get(control as usize)
        .map(|e| e.value)
    {
        Some(ControlValue::Switch { .. }) => ControlReading::Switch(after != 0.0),
        Some(ControlValue::Integer { .. }) => ControlReading::Integer(after as u64),
        Some(ControlValue::Continuous { .. }) => ControlReading::Continuous(after as f32),
        _ => ControlReading::Choice(after as u32),
    }
}
