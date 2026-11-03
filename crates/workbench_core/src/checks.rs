//! The entry-point checks (spec 006 D1): the session's request through each of the engine's six
//! one-pass call forms, and through an engine reused after `reset` against a fresh engine built
//! from its state. Each form runs the render's block, tail and just-in-time schedule (`Due`).

use std::sync::atomic::{AtomicBool, Ordering};

use rr_dr60::changes::{ChangeEntry, MAX_CHANGES_PER_CALL};
use rr_dr60::{Error, Meters, Pipeline, Stream, VasEvent};

use crate::render::{self, Due, RenderRequest};

/// A one-pass call form (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// `process_with_events_and_changes`: the render's.
    WithEventsAndChanges,
    /// `process`.
    Process,
    /// `process_with_events`.
    WithEvents,
    /// `process_in_place`.
    InPlace,
    /// `process_in_place_with_events`.
    InPlaceWithEvents,
    /// `process_in_place_with_events_and_changes`.
    InPlaceWithEventsAndChanges,
}

/// The six forms, the render's first.
pub const FORMS: [Form; 6] = [
    Form::WithEventsAndChanges,
    Form::Process,
    Form::WithEvents,
    Form::InPlace,
    Form::InPlaceWithEvents,
    Form::InPlaceWithEventsAndChanges,
];

impl Form {
    /// The engine method's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::WithEventsAndChanges => "process_with_events_and_changes",
            Self::Process => "process",
            Self::WithEvents => "process_with_events",
            Self::InPlace => "process_in_place",
            Self::InPlaceWithEvents => "process_in_place_with_events",
            Self::InPlaceWithEventsAndChanges => "process_in_place_with_events_and_changes",
        }
    }

    /// Whether the form reports the gate's events.
    pub fn has_events(self) -> bool {
        !matches!(self, Self::Process | Self::InPlace)
    }

    /// Whether the form reports the change log.
    pub fn has_changes(self) -> bool {
        matches!(
            self,
            Self::WithEventsAndChanges | Self::InPlaceWithEventsAndChanges
        )
    }
}

/// One call's report (D1: compared per call).
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    /// `BlockInfo.produced`.
    pub produced: usize,
    /// `BlockInfo.events`.
    pub events: usize,
    /// `BlockInfo.paused`.
    pub paused: bool,
    /// `BlockInfo.changes`.
    pub changes: usize,
    /// `Pipeline::meters` after the call.
    pub meters: Meters,
}

/// A form's run.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The output.
    pub samples: Vec<f32>,
    /// The gate's events (forms with events).
    pub events: Vec<VasEvent>,
    /// The change log (forms with changes).
    pub log: Vec<ChangeEntry>,
    /// Per call.
    pub calls: Vec<Call>,
}

/// A form that did not run (D1, hardware review B2).
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// A returning form's refusal, in the engine's words.
    Engine(Error),
    /// An in-place call refused: neither clock advanced.
    InPlaceRefused,
    /// A scheduled change was refused.
    Change(Error),
    /// Cancelled.
    Cancelled,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Engine(e) => write!(f, "refused: {e}"),
            Self::InPlaceRefused => write!(
                f,
                "refused: the in-place call returned with neither clock advancing"
            ),
            Self::Change(e) => write!(f, "a scheduled change was refused: {e}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Runs `req` on `p` through `form` (the render's block, tail and schedule).
///
/// # Errors
///
/// The form's refusal, a refused change, or a cancel.
pub fn run_on(
    p: &mut Pipeline,
    req: &RenderRequest,
    form: Form,
    cancel: &AtomicBool,
) -> Result<Run, Refusal> {
    let base = (req.start as usize).min(req.input.len());
    let avail = (req.input.len() - base) as u64;
    let total = match req.len {
        Some(n) => n,
        None => avail + if req.tail { render::tail_for(p) } else { 0 },
    };
    let block = req.block.max(1);
    let mut ibuf = vec![0.0f32; block];
    let mut obuf = vec![0.0f32; block];
    let mut ebuf = vec![VasEvent::new(0, 0); p.max_events(block).max(1)];
    let mut cbuf = vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL];
    let mut due = Due::new(&req.changes);
    let mut run = Run {
        samples: Vec::with_capacity(total as usize),
        events: Vec::new(),
        log: Vec::new(),
        calls: Vec::new(),
    };
    let mut fed = 0u64;
    while fed < total {
        if cancel.load(Ordering::Relaxed) {
            return Err(Refusal::Cancelled);
        }
        let n = (total - fed).min(block as u64) as usize;
        let start = base + fed as usize;
        let have = req.input.len().saturating_sub(start).min(n);
        if have > 0 {
            ibuf[..have].copy_from_slice(&req.input[start..start + have]);
        }
        ibuf[have..n].fill(0.0);
        let mut refusal = None;
        due.feed(p, &req.changes, n as u64, &mut |_, e| {
            refusal.get_or_insert(e);
        });
        if let Some(e) = refusal {
            return Err(Refusal::Change(e));
        }
        let before = (
            p.input_position(Stream::OnePass).ok(),
            p.stream_position(Stream::OnePass).ok(),
        );
        let input = &ibuf[..n];
        let info = match form {
            Form::WithEventsAndChanges => {
                p.process_with_events_and_changes(input, &mut obuf[..n], &mut ebuf, &mut cbuf)
            }
            Form::Process => Pipeline::process(p, input, &mut obuf[..n]),
            Form::WithEvents => Pipeline::process_with_events(p, input, &mut obuf[..n], &mut ebuf),
            Form::InPlace | Form::InPlaceWithEvents | Form::InPlaceWithEventsAndChanges => {
                obuf[..n].copy_from_slice(input);
                let info = match form {
                    Form::InPlace => Pipeline::process_in_place(p, &mut obuf[..n]),
                    Form::InPlaceWithEvents => {
                        Pipeline::process_in_place_with_events(p, &mut obuf[..n], &mut ebuf)
                    }
                    _ => Pipeline::process_in_place_with_events_and_changes(
                        p,
                        &mut obuf[..n],
                        &mut ebuf,
                        &mut cbuf,
                    ),
                };
                let after = (
                    p.input_position(Stream::OnePass).ok(),
                    p.stream_position(Stream::OnePass).ok(),
                );
                if n > 0 && after == before {
                    return Err(Refusal::InPlaceRefused);
                }
                Ok(info)
            }
        }
        .map_err(Refusal::Engine)?;
        run.samples.extend_from_slice(&obuf[..info.produced]);
        if form.has_events() {
            run.events
                .extend_from_slice(&ebuf[..info.events.min(ebuf.len())]);
        }
        if form.has_changes() {
            run.log
                .extend_from_slice(&cbuf[..info.changes.min(cbuf.len())]);
        }
        run.calls.push(Call {
            produced: info.produced,
            events: info.events,
            paused: info.paused,
            changes: info.changes,
            meters: Pipeline::meters(p),
        });
        fed += n as u64;
    }
    Ok(run)
}

/// One form's result against the render's form (D1).
#[derive(Clone, Debug, PartialEq)]
pub struct FormResult {
    /// The form.
    pub form: Form,
    /// The run, or its refusal.
    pub run: Result<Run, Refusal>,
    /// The output agrees bit for bit, per call (output, `BlockInfo`, meters), and so do its
    /// events and log where the form reports them.
    pub agrees: bool,
}

/// Whether `x` agrees with the reference `r` for `form` (D1).
pub fn agrees(r: &Run, x: &Run, form: Form) -> bool {
    let bits = r.samples.len() == x.samples.len()
        && r.samples
            .iter()
            .zip(&x.samples)
            .all(|(a, b)| a.to_bits() == b.to_bits());
    let calls = r.calls.len() == x.calls.len()
        && r.calls.iter().zip(&x.calls).all(|(a, b)| {
            a.produced == b.produced
                && a.events == b.events
                && a.paused == b.paused
                && a.changes == b.changes
                && a.meters == b.meters
        });
    bits && calls
        && (!form.has_events() || r.events == x.events)
        && (!form.has_changes() || r.log == x.log)
}

/// The log without the tag-0 entries at position 0 (a fresh engine's plain setters log them; a
/// reset engine's start values do not: hardware review B1).
pub fn without_start_entries(log: &[ChangeEntry]) -> Vec<ChangeEntry> {
    log.iter()
        .filter(|e| !(e.tag == 0 && e.position == 0))
        .copied()
        .collect()
}

/// The reset check (D1; hardware review B1): one engine runs the request, is `reset`, runs it
/// again; its oracle is a fresh engine built from the first engine's state after its first run
/// (`settings()`, the controls that are not settings: the live values and the slot), with the
/// same schedule.
#[derive(Clone, Debug, PartialEq)]
pub struct ResetCheck {
    /// The reset engine's second run.
    pub reset_run: Run,
    /// The oracle's run.
    pub oracle: Run,
    /// The audio agrees bit for bit, and the logs without their start entries.
    pub agrees: bool,
}

/// Runs the reset check.
///
/// # Errors
///
/// A refusal of a run or of the oracle's construction.
pub fn reset_check(req: &RenderRequest, cancel: &AtomicBool) -> Result<ResetCheck, Refusal> {
    let mut p = render::fresh_engine(req).map_err(Refusal::Engine)?;
    let _first = run_on(&mut p, req, Form::WithEventsAndChanges, cancel)?;
    let settings = *Pipeline::settings(&p);
    let others: Vec<(u32, rr_dr60::control_map::ControlReading)> = (0
        ..rr_dr60::controls::controls().len() as u32)
        .filter(|&i| rr_dr60::control_map::settings_value(&settings, i).is_err())
        .filter_map(|i| p.control_value(i).ok().map(|r| (i, r)))
        .collect();
    Pipeline::reset(&mut p);
    let reset_run = run_on(&mut p, req, Form::WithEventsAndChanges, cancel)?;
    // A playback-side switch that landed during the first run is "landed off, still running",
    // not "configured off" (engine spec 021 D11; hardware review B1): the oracle is built with
    // the request's own switches and the landed values scheduled at position 0, tag 0.
    let switches = [
        "output.volume_amp_enabled",
        "output.speaker_enabled",
        "output.earphone_enabled",
    ];
    let mut oracle_settings = settings;
    let mut landed = Vec::new();
    for id in switches {
        let Some(i) = rr_dr60::controls::controls()
            .iter()
            .position(|e| e.identifier == id)
            .map(|i| i as u32)
        else {
            continue;
        };
        let (now, then) = (
            rr_dr60::control_map::settings_value(&settings, i),
            rr_dr60::control_map::settings_value(&req.settings, i),
        );
        if let (Ok(now), Ok(then)) = (now, then)
            && now != then
        {
            let _ = rr_dr60::control_map::set_settings_value(&mut oracle_settings, i, then);
            landed.push((i, now));
        }
    }
    let mut q = Pipeline::new(oracle_settings).map_err(Refusal::Engine)?;
    for (i, r) in others {
        q.set_control_value(i, r).map_err(Refusal::Engine)?;
    }
    for (i, r) in landed {
        let v = match r {
            rr_dr60::control_map::ControlReading::Switch(b) => f64::from(u8::from(b)),
            _ => continue,
        };
        q.schedule_change(Stream::OnePass, 0, i, v, 0)
            .map_err(Refusal::Change)?;
    }
    let oracle = run_on(&mut q, req, Form::WithEventsAndChanges, cancel)?;
    let bits = reset_run.samples.len() == oracle.samples.len()
        && reset_run
            .samples
            .iter()
            .zip(&oracle.samples)
            .all(|(a, b)| a.to_bits() == b.to_bits());
    let agrees = bits
        && reset_run.events == oracle.events
        && without_start_entries(&reset_run.log) == without_start_entries(&oracle.log);
    Ok(ResetCheck {
        reset_run,
        oracle,
        agrees,
    })
}

/// The one-pass check (D1): every form against the render's form on fresh engines, and the
/// reset check.
pub struct OnePassCheck {
    /// One per form, the render's first.
    pub forms: Vec<FormResult>,
    /// The reset check.
    pub reset: Result<ResetCheck, Refusal>,
}

/// Runs the one-pass check for `req`.
pub fn one_pass_check(req: &RenderRequest, cancel: &AtomicBool) -> OnePassCheck {
    let runs: Vec<(Form, Result<Run, Refusal>)> = FORMS
        .iter()
        .map(|&f| {
            let run = render::fresh_engine(req)
                .map_err(Refusal::Engine)
                .and_then(|mut p| run_on(&mut p, req, f, cancel));
            (f, run)
        })
        .collect();
    let reference = runs[0].1.clone();
    let forms = runs
        .into_iter()
        .map(|(form, run)| {
            let agrees = match (&reference, &run) {
                (Ok(r), Ok(x)) => agrees(r, x, form),
                _ => false,
            };
            FormResult { form, run, agrees }
        })
        .collect();
    OnePassCheck {
        forms,
        reset: reset_check(req, cancel),
    }
}
