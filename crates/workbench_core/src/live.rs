//! The live engine (spec 003): the core the output callback runs, its queues, and the take it
//! leaves behind (research R-01 – R-04).
//!
//! [`LiveCore::fill`] is the callback's body. It calls the engine in 10 ms chunks through the
//! same [`render::run_chunk`] the offline render uses, into a FIFO, schedules the user's "now"
//! commands at the stream's current position, and sends every sample, report, log entry and
//! meter row to the UI. It allocates nothing, locks nothing and does no I/O.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use rr_dr60::changes::{ChangeEntry, Clock, MAX_CHANGES_PER_CALL};
use rr_dr60::control_map::ControlReading;
use rr_dr60::controls::{ControlTiming, ControlValue};
use rr_dr60::{Error, Pipeline, Stream, VasEvent};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::meters::MeterRow;
use crate::render::{self, Due, LiveInfo, Render, RenderRequest, Scheduled};

/// The largest device buffer served in one part (D8).
pub const MAX_PART: usize = 4096;
/// The budget: input fed per callback, in chunks, as a multiple of the buffer (D4).
pub const BUDGET_FACTOR: usize = 4;

/// A control change sent to the callback (D2, D3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Command {
    /// The control's manifest index.
    pub control: u32,
    /// The value, as `schedule_change` takes it.
    pub value: f64,
}

/// A command's result, returned by the callback (D2: the engine's own words, on the UI).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reply {
    /// The tag the command was scheduled with.
    pub tag: u64,
    /// The control.
    pub control: u32,
    /// The engine's refusal, if any.
    pub error: Option<Error>,
}

/// The counters the UI reads (atomics only).
#[derive(Debug, Default)]
pub struct LiveState {
    /// Input fed from the start (the tail's silence included).
    pub fed: AtomicU64,
    /// Engine output produced.
    pub produced: AtomicU64,
    /// The input (and its tail) is exhausted.
    pub finished: AtomicBool,
    /// A queue or report buffer overflowed: the take does not reproduce.
    pub incomplete: AtomicBool,
    /// Callbacks that ended in a budget skip.
    pub skips: AtomicU64,
}

/// The UI's ends of the queues.
pub struct LiveQueues {
    /// Commands to the callback.
    pub commands: Producer<Command>,
    /// Their results.
    pub replies: Consumer<Reply>,
    /// Engine output samples.
    pub samples: Consumer<f32>,
    /// Gate reports.
    pub events: Consumer<VasEvent>,
    /// Change-log entries.
    pub log: Consumer<ChangeEntry>,
    /// Meter rows.
    pub rows: Consumer<MeterRow>,
    /// Budget skips: (take position, samples).
    pub skips: Consumer<(u64, u64)>,
    /// The counters.
    pub state: Arc<LiveState>,
}

/// The audio thread's core.
pub struct LiveCore {
    p: Pipeline,
    input: Arc<[f32]>,
    base: usize,
    pos: u64,
    end: u64,
    block: usize,
    fifo: VecDeque<f32>,
    ibuf: Vec<f32>,
    obuf: Vec<f32>,
    ebuf: Vec<VasEvent>,
    cbuf: Vec<ChangeEntry>,
    list: Vec<Scheduled>,
    due: Due,
    next_tag: u64,
    commands: Consumer<Command>,
    replies: Producer<Reply>,
    samples: Producer<f32>,
    events: Producer<VasEvent>,
    log: Producer<ChangeEntry>,
    rows: Producer<MeterRow>,
    skips: Producer<(u64, u64)>,
    state: Arc<LiveState>,
    produced: u64,
    failed: bool,
}

/// A converted control value for `schedule_change`.
pub fn value_of(r: ControlReading) -> f64 {
    match r {
        ControlReading::Switch(b) => f64::from(u8::from(b)),
        ControlReading::Integer(i) => i as f64,
        ControlReading::Continuous(f) => f64::from(f),
        ControlReading::Choice(c) => f64::from(c),
    }
}

/// Queue sizes for a 1 s UI stall plus one worst-case callback (plan review B5): a callback of
/// up to `MAX_PART` frames per part may run `BUDGET_FACTOR · MAX_PART / block` chunks, so the
/// queues hold a second of chunks plus two parts' worth.
fn sizes(rate: u32, block: usize, max_events: usize) -> (usize, usize, usize, usize) {
    let per_second = (rate as usize).div_ceil(block.max(1));
    let worst = 2 * (BUDGET_FACTOR * MAX_PART).div_ceil(block.max(1)) + 2;
    let chunks = per_second + worst;
    (
        rate as usize + 2 * BUDGET_FACTOR * MAX_PART,
        chunks * MAX_CHANGES_PER_CALL.min(8) + MAX_CHANGES_PER_CALL,
        chunks * max_events.max(1),
        chunks + 1,
    )
}

impl LiveCore {
    /// Builds the core for `req` (the whole file, its start S, the panel's values at the start)
    /// and the user's list (positions from S, in their clocks). The list is tagged 1 … k in its
    /// order; "now" commands continue from k + 1.
    ///
    /// # Errors
    ///
    /// The engine's refusal of the settings or values.
    pub fn new(
        req: &RenderRequest,
        list: &[(Clock, u64, u32, f64)],
    ) -> Result<(Self, LiveQueues), Error> {
        let p = render::fresh_engine(req)?;
        let base = (req.start as usize).min(req.input.len());
        let avail = (req.input.len() - base) as u64;
        let end = avail + if req.tail { render::tail_for(&p) } else { 0 };
        let block = render::block_for(req.rate_hz).max(1);
        let list: Vec<Scheduled> = list
            .iter()
            .enumerate()
            .map(|(i, &(clock, position, control, value))| Scheduled {
                clock,
                position,
                control,
                value,
                tag: i as u64 + 1,
            })
            .collect();
        let next_tag = list.len() as u64 + 1;
        // The events buffer for a chunk at the hang's 50 ms minimum (plan review B4): set the
        // minimum on a scratch copy of the settings to size it, the engine keeps its own.
        let mut s = req.settings;
        s.vas.hang_ms = 50.0;
        let max_ev = Pipeline::new(s)
            .map_or(block, |q| q.max_events(block))
            .max(p.max_events(block))
            .max(1);
        let (ns, nl, ne, nr) = sizes(req.rate_hz, block, max_ev);
        let (cp, cc) = RingBuffer::new(256);
        let (rp, rc) = RingBuffer::new(256);
        let (sp, sc) = RingBuffer::new(ns);
        let (ep, ec) = RingBuffer::new(ne);
        let (lp, lc) = RingBuffer::new(nl);
        let (mp, mc) = RingBuffer::new(nr);
        let (kp, kc) = RingBuffer::new(1024);
        let state = Arc::new(LiveState::default());
        // The rest row first, as the render's timeline (002 D1).
        let rest = MeterRow {
            meters: p.rest_meters(),
            in_span: (0, 0),
            out_span: (0, 0),
            tail: false,
        };
        let mut core = Self {
            due: Due::new(&list),
            p,
            input: req.input.clone(),
            base,
            pos: 0,
            end,
            block,
            fifo: VecDeque::with_capacity(MAX_PART + 2 * block),
            ibuf: vec![0.0; block],
            obuf: vec![0.0; block],
            ebuf: vec![VasEvent::new(0, 0); max_ev],
            cbuf: vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL],
            list,
            next_tag,
            commands: cc,
            replies: rp,
            samples: sp,
            events: ep,
            log: lp,
            rows: mp,
            skips: kp,
            state: state.clone(),
            produced: 0,
            failed: false,
        };
        if core.rows.push(rest).is_err() {
            core.state.incomplete.store(true, Ordering::Relaxed);
        }
        Ok((
            core,
            LiveQueues {
                commands: cp,
                replies: rc,
                samples: sc,
                events: ec,
                log: lc,
                rows: mc,
                skips: kc,
                state,
            },
        ))
    }

    fn mark_incomplete(&self) {
        self.state.incomplete.store(true, Ordering::Relaxed);
    }

    /// Whether a chunk failed in the engine (the run stopped).
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Schedules the waiting "now" commands at the stream's current position in each control's
    /// clock (D2).
    fn take_commands(&mut self) {
        while let Ok(c) = self.commands.pop() {
            let tag = self.next_tag;
            self.next_tag += 1;
            let clock = rr_dr60::changes::clock_of(c.control);
            let position = match clock {
                Ok(Clock::Input) => self.p.input_position(Stream::OnePass),
                Ok(_) => self.p.stream_position(Stream::OnePass),
                Err(e) => Err(e),
            };
            let error = position
                .and_then(|at| {
                    self.p
                        .schedule_change(Stream::OnePass, at, c.control, c.value, tag)
                })
                .err();
            if self
                .replies
                .push(Reply {
                    tag,
                    control: c.control,
                    error,
                })
                .is_err()
            {
                self.mark_incomplete();
            }
        }
    }

    /// Runs one chunk and sends its results.
    fn chunk(&mut self) {
        let n = (self.end - self.pos).min(self.block as u64) as usize;
        let start = self.base + self.pos as usize;
        let have = self.input.len().saturating_sub(start).min(n);
        if have > 0 {
            self.ibuf[..have].copy_from_slice(&self.input[start..start + have]);
        }
        self.ibuf[have..n].fill(0.0);
        let replies = &mut self.replies;
        let state = &self.state;
        let list = &self.list;
        let res = render::run_chunk(
            &mut self.p,
            &self.ibuf[..n],
            &mut self.obuf,
            &mut self.ebuf,
            &mut self.cbuf,
            &mut self.due,
            list,
            &mut |tag, e| {
                let control = list.iter().find(|c| c.tag == tag).map_or(0, |c| c.control);
                if replies
                    .push(Reply {
                        tag,
                        control,
                        error: Some(e),
                    })
                    .is_err()
                {
                    state.incomplete.store(true, Ordering::Relaxed);
                }
            },
        );
        let info = match res {
            Ok(i) => i,
            Err(_) => {
                // A refusal in the callback: the take cannot be reproduced; stop the run.
                self.mark_incomplete();
                self.failed = true;
                return;
            }
        };
        assert!(
            info.produced <= n,
            "the engine emits at most one sample per input sample"
        );
        if info.changes > self.cbuf.len() || info.events > self.ebuf.len() {
            self.mark_incomplete();
        }
        let out0 = self.produced;
        for &v in &self.obuf[..info.produced] {
            self.fifo.push_back(v);
            if self.samples.push(v).is_err() {
                self.mark_incomplete();
            }
        }
        self.produced += info.produced as u64;
        for &e in &self.ebuf[..info.events.min(self.ebuf.len())] {
            if self.events.push(e).is_err() {
                self.mark_incomplete();
            }
        }
        for &c in &self.cbuf[..info.changes.min(self.cbuf.len())] {
            if self.log.push(c).is_err() {
                self.mark_incomplete();
            }
        }
        let avail = (self.input.len() - self.base) as u64;
        let row = MeterRow {
            meters: Pipeline::meters(&self.p),
            in_span: (self.pos, self.pos + n as u64),
            out_span: (out0, self.produced),
            tail: self.pos >= avail && n > 0,
        };
        if self.rows.push(row).is_err() {
            self.mark_incomplete();
        }
        self.pos += n as u64;
        self.state.fed.store(self.pos, Ordering::Relaxed);
        self.state.produced.store(self.produced, Ordering::Relaxed);
        if self.pos >= self.end {
            self.state.finished.store(true, Ordering::Relaxed);
        }
    }

    /// The callback's body: fills `out` (mono) from the engine (D1, D4).
    pub fn fill(&mut self, out: &mut [f32]) {
        for part in out.chunks_mut(MAX_PART) {
            self.fill_part(part);
        }
    }

    fn fill_part(&mut self, out: &mut [f32]) {
        self.take_commands();
        let budget_chunks = (BUDGET_FACTOR * out.len()).div_ceil(self.block).max(1);
        let mut chunks = 0usize;
        while self.fifo.len() < out.len()
            && chunks < budget_chunks
            && self.pos < self.end
            && !self.failed
        {
            self.chunk();
            chunks += 1;
        }
        let have = self.fifo.len().min(out.len());
        for v in out.iter_mut().take(have) {
            *v = self.fifo.pop_front().unwrap_or(0.0);
        }
        out[have..].fill(0.0);
        if have < out.len() && self.pos < self.end && !self.failed {
            // The budget ran out with the gate paused: labelled silence, not engine output.
            self.state.skips.fetch_add(1, Ordering::Relaxed);
            if self
                .skips
                .push((self.produced, (out.len() - have) as u64))
                .is_err()
            {
                self.mark_incomplete();
            }
        }
    }

    /// The FIFO's samples (the tests' bound check).
    pub fn fifo_len(&self) -> usize {
        self.fifo.len()
    }

    /// The engine's pending changes (both clocks of the one-pass).
    pub fn pending(&self) -> u32 {
        self.p.pending_changes(Stream::OnePass).unwrap_or(0)
    }
}

/// The take, assembled on the UI thread from the queues (D5).
#[derive(Debug, Default)]
pub struct TakeBuilder {
    /// The engine's output.
    pub samples: Vec<f32>,
    /// The gate's reports.
    pub events: Vec<VasEvent>,
    /// The change log.
    pub log: Vec<ChangeEntry>,
    /// The meter rows.
    pub rows: Vec<MeterRow>,
    /// The budget's skips.
    pub skips: Vec<(u64, u64)>,
    /// The commands' results.
    pub replies: Vec<Reply>,
}

impl TakeBuilder {
    /// Moves everything the queues hold into the take.
    pub fn drain(&mut self, q: &mut LiveQueues) {
        while let Ok(v) = q.samples.pop() {
            self.samples.push(v);
        }
        while let Ok(v) = q.events.pop() {
            self.events.push(v);
        }
        while let Ok(v) = q.log.pop() {
            self.log.push(v);
        }
        while let Ok(v) = q.rows.pop() {
            self.rows.push(v);
        }
        while let Ok(v) = q.skips.pop() {
            self.skips.push(v);
        }
        while let Ok(v) = q.replies.pop() {
            self.replies.push(v);
        }
    }

    /// The take as the session's render (D5): its request is the start's request with the
    /// range fed and the log's scheduled entries (tag ≥ 1) as its changes, so a re-render of it
    /// reproduces the take.
    ///
    /// # Errors
    ///
    /// The engine's refusal of the start values (not expected: the run used them).
    pub fn finish(self, start_req: &RenderRequest, state: &LiveState) -> Result<Render, Error> {
        let fed = state.fed.load(Ordering::Relaxed);
        let base = (start_req.start as usize).min(start_req.input.len());
        let avail = (start_req.input.len() - base) as u64;
        let changes: Vec<Scheduled> = self
            .log
            .iter()
            .filter(|e| e.tag >= 1)
            .map(|e| Scheduled {
                clock: rr_dr60::changes::clock_of(e.control).unwrap_or(Clock::Output),
                position: e.position,
                control: e.control,
                value: e.after,
                tag: e.tag,
            })
            .collect();
        let mut request = start_req.clone();
        request.len = Some(fed);
        request.changes = changes;
        let p = render::fresh_engine(&request)?;
        let peak = self.samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let output_sha256 = crate::input::samples_sha256(&self.samples);
        let splices = self.events.len() as u64;
        Ok(Render {
            tail_samples: fed.saturating_sub(avail),
            latency_samples: p.latency_samples(),
            drain_samples: p.drain_samples(),
            playback_latency_samples: p.playback_latency_samples(),
            splices_or_regions: splices,
            drop_mode: request.settings.vas.mode == rr_dr60::VasMode::Drop,
            gate_reached: render::gate_reached(&request.settings),
            paused_at_end: self.rows.last().is_some_and(|r| r.meters.vas_paused),
            not_emitted: fed - self.samples.len() as u64,
            peak,
            output_sha256,
            footprint_bytes: p.footprint_bytes(),
            meters: self.rows,
            events: self.events,
            log: self.log,
            live: Some(LiveInfo {
                start: start_req.start,
                fed,
                skips: self.skips,
                incomplete: state.incomplete.load(Ordering::Relaxed),
                tail_switch: start_req.tail && fed > avail,
            }),
            samples: self.samples.into(),
            request,
        })
    }
}

/// Why a control is disabled during a live run, or `None` when it can change live (D2;
/// research R-04, plan review B1): its timing, then a dry run of every option (switches and
/// choices) or the current value (continuous) on a check engine built like the live one.
pub fn live_reason(req: &RenderRequest, index: u32) -> Option<&'static str> {
    let e = rr_dr60::controls::controls().get(index as usize)?;
    if e.timing != ControlTiming::Live {
        return Some(crate::text::LIVE_IDLE);
    }
    let Ok(mut p) = render::fresh_engine(req) else {
        return Some(crate::text::LIVE_NOT_ONE_PASS);
    };
    if e.identifier.starts_with("band_filter.") {
        let slot = rr_dr60::controls::controls()
            .iter()
            .position(|c| c.identifier == "insert.before_limiter");
        if let Some(si) = slot
            && p.control_value(si as u32).ok() == Some(ControlReading::Choice(0))
        {
            return Some(crate::text::LIVE_SLOT_EMPTY);
        }
    }
    let values: Vec<f64> = match e.value {
        ControlValue::Switch { .. } => vec![0.0, 1.0],
        ControlValue::Choice { options, .. } => {
            options.iter().map(|o| f64::from(o.value)).collect()
        }
        _ => p
            .control_value(index)
            .ok()
            .map(value_of)
            .into_iter()
            .collect(),
    };
    for v in values {
        let r = p.schedule_change(Stream::OnePass, 0, index, v, 1);
        let _ = p.clear_schedule(Stream::OnePass);
        match r {
            Ok(()) => {}
            Err(Error::ControlNotOnStream { .. }) => return Some(crate::text::LIVE_NOT_ONE_PASS),
            Err(Error::StageNotConfigured { .. }) => return Some(crate::text::LIVE_STAGE_OFF),
            Err(Error::TapNotOnRoute { .. }) => return Some(crate::text::LIVE_TAP_ROUTE),
            Err(_) => {}
        }
    }
    None
}
