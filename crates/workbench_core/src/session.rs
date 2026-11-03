//! The session: one file, its panel, the tail switch and the current render (spec 001 D10;
//! data-model "Session"). The rules that decide what the view shows live here so they are tested
//! without a window (post-implementation review, coach 1 – 3, 6).

use std::path::{Path, PathBuf};

use crate::input::{self, Channel, Input, OpenError};
use crate::panel::Panel;
use crate::render::{Render, RenderRequest, Scheduled};
use crate::sidecar::{self, RestoreReport, SidecarError};

/// The session's state.
#[derive(Debug, Default)]
pub struct Session {
    path: Option<PathBuf>,
    input: Option<Input>,
    panel: Option<Panel>,
    tail: bool,
    render: Option<Render>,
    /// A restored live take's range and changes (spec 003 D5): `(start, fed, changes)`.
    range: Option<(u64, u64, Vec<Scheduled>)>,
    /// A live take's values at the stop: its staleness is judged against them (plan review B2).
    live_end: Option<RenderRequest>,
}

/// A comparable summary of a session (the "session unchanged" tests).
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The input's converted digest and channel.
    pub input: Option<([u8; 32], Channel)>,
    /// Every panel row's value.
    pub values: Vec<rr_dr60::control_map::ControlReading>,
    /// The tail switch.
    pub tail: bool,
    /// The render's output digest.
    pub render: Option<[u8; 32]>,
}

impl Session {
    /// An empty session with the tail on (D7).
    pub fn new() -> Self {
        Self {
            tail: true,
            ..Self::default()
        }
    }

    /// The open input.
    pub fn input(&self) -> Option<&Input> {
        self.input.as_ref()
    }

    /// The panel.
    pub fn panel(&self) -> Option<&Panel> {
        self.panel.as_ref()
    }

    /// The panel, to edit.
    pub fn panel_mut(&mut self) -> Option<&mut Panel> {
        self.panel.as_mut()
    }

    /// The current render.
    pub fn render(&self) -> Option<&Render> {
        self.render.as_ref()
    }

    /// The tail switch.
    pub fn tail(&self) -> bool {
        self.tail
    }

    /// Sets the tail switch.
    pub fn set_tail(&mut self, on: bool) {
        self.tail = on;
    }

    /// The open file's path.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Opens `path` with `channel`. A refusal leaves the session unchanged. A new file (or a new
    /// rate) gets a fresh panel and drops the render; the same file with another channel keeps
    /// the panel and the render, which is then stale (its request records its own input).
    ///
    /// # Errors
    ///
    /// The input's refusal.
    pub fn open(&mut self, path: &Path, channel: Channel) -> Result<(), OpenError> {
        let inp = input::open(path, channel)?;
        let same = self.path.as_deref() == Some(path)
            && self
                .panel
                .as_ref()
                .is_some_and(|p| p.rate_hz() == inp.rate_hz);
        if !same {
            self.panel =
                Some(Panel::new(inp.rate_hz).map_err(|e| OpenError::Decode(e.to_string()))?);
            self.render = None;
            self.range = None;
            self.live_end = None;
        }
        self.path = Some(path.to_path_buf());
        self.input = Some(inp);
        Ok(())
    }

    /// The render request for the current input and panel.
    pub fn request(&self) -> Option<RenderRequest> {
        let mut r = self.plain_request()?;
        if let Some((start, fed, changes)) = &self.range {
            r.start = *start;
            r.len = Some(*fed);
            r.changes = changes.clone();
        }
        Some(r)
    }

    /// The request without a restored range (a live run's start, or a whole-file render).
    pub fn plain_request(&self) -> Option<RenderRequest> {
        Some(RenderRequest::new(
            self.input.as_ref()?,
            self.panel.as_ref()?,
            self.tail,
        ))
    }

    /// Drops a restored live range, so renders cover the whole file again.
    pub fn clear_range(&mut self) {
        self.range = None;
    }

    /// The restored live range, if any.
    pub fn range(&self) -> Option<&(u64, u64, Vec<Scheduled>)> {
        self.range.as_ref()
    }

    /// Accepts a finished live take (spec 003 D5): the panel takes the take's end values, and its
    /// staleness is judged against them (post-implementation review B6).
    pub fn accept_live_take(&mut self, take: Render) -> bool {
        let end = crate::live::end_values(&take);
        if let Some(p) = self.panel.as_mut() {
            p.set_many(&end);
        }
        let Some(at_stop) = self.plain_request() else {
            return false;
        };
        self.accept_live(take, at_stop)
    }

    /// Accepts a finished live take with a given request at the stop, against which its staleness
    /// is judged.
    pub fn accept_live(&mut self, take: Render, at_stop: RenderRequest) -> bool {
        let ok = self.accept_render(take);
        if ok {
            self.live_end = Some(at_stop);
        }
        ok
    }

    /// True when the current render was made from other values or another input (D5).
    pub fn is_stale(&self) -> bool {
        match (&self.render, self.request()) {
            (Some(_), Some(q)) if self.live_end.is_some() => {
                self.live_end.as_ref().is_some_and(|e| e.differs(&q))
            }
            (Some(r), Some(q)) => r.is_stale(&q),
            _ => false,
        }
    }

    /// Accepts a finished render only if it was made from the current file and rate (coach 1).
    /// A render of another file is discarded and `false` returned.
    pub fn accept_render(&mut self, r: Render) -> bool {
        let Some(inp) = &self.input else { return false };
        let same_file = r.request.raw_sha256 == inp.raw_sha256 && r.request.rate_hz == inp.rate_hz;
        if same_file {
            self.render = Some(r);
            self.live_end = None;
        }
        same_file
    }

    /// Restores a sidecar (D9). The sidecar's channel is applied first, so its converted-input
    /// digest is compared with the right channel (coach 3; hardware review B4). A refusal leaves
    /// the session unchanged.
    ///
    /// # Errors
    ///
    /// The sidecar's refusal, or no input open.
    pub fn restore(&mut self, text: &str) -> Result<RestoreReport, RestoreError> {
        let s = sidecar::read(text).map_err(RestoreError::Sidecar)?;
        let (Some(path), Some(inp)) = (self.path.clone(), self.input.as_ref()) else {
            return Err(RestoreError::NoInput);
        };
        let channel = match s.fields.get("channel").map(String::as_str) {
            Some("mix") => Channel::Mix,
            Some(c) => c
                .parse::<u16>()
                .ok()
                .filter(|&n| n >= 1)
                .map_or(inp.channel, |n| Channel::Index(n - 1)),
            None => inp.channel,
        };
        if channel != inp.channel {
            let reopened = input::open(&path, channel).map_err(RestoreError::Channel)?;
            self.input = Some(reopened);
        }
        let inp = self.input.as_ref().expect("input");
        let panel = self.panel.as_mut().expect("a panel with an input");
        let rep = sidecar::restore(&s, panel, inp);
        self.tail = rep.tail;
        // A live take's range and changes (version 2, `live yes`).
        self.range = None;
        if s.version >= 2 && s.fields.get("live").map(String::as_str) == Some("yes") {
            let start = s
                .fields
                .get("start")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let fed = s
                .fields
                .get("fed")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            // An unknown change identifier means the take cannot be reproduced: listed, and the
            // range is not set (post-implementation review B14).
            let unknown: Vec<String> = s
                .changes
                .iter()
                .filter(|(_, _, id, _, _)| {
                    !rr_dr60::controls::controls()
                        .iter()
                        .any(|e| e.identifier == id)
                })
                .map(|(_, _, id, _, _)| id.clone())
                .collect();
            let changes = s
                .changes
                .iter()
                .filter_map(|(clock, position, id, value, tag)| {
                    let control = rr_dr60::controls::controls()
                        .iter()
                        .position(|e| e.identifier == id)? as u32;
                    Some(Scheduled {
                        clock: *clock,
                        position: *position,
                        control,
                        value: *value,
                        tag: *tag,
                    })
                })
                .collect();
            if unknown.is_empty() {
                self.range = Some((start, fed, changes));
            } else {
                let mut rep = rep;
                rep.unknown_ids.extend(unknown);
                return Ok(rep);
            }
        }
        Ok(rep)
    }

    /// The session's comparable state.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            input: self.input.as_ref().map(|i| (i.input_sha256, i.channel)),
            values: self
                .panel
                .as_ref()
                .map(|p| p.rows().iter().map(|r| r.value).collect())
                .unwrap_or_default(),
            tail: self.tail,
            render: self.render.as_ref().map(|r| r.output_sha256),
        }
    }
}

/// A refused restore.
#[derive(Debug)]
pub enum RestoreError {
    /// The sidecar itself is refused.
    Sidecar(SidecarError),
    /// No input is open.
    NoInput,
    /// The sidecar's channel cannot be opened from this file.
    Channel(OpenError),
}

impl std::fmt::Display for RestoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sidecar(e) => e.fmt(f),
            Self::NoInput => write!(f, "{}", crate::text::OPEN_INPUT_FIRST),
            Self::Channel(e) => write!(f, "The sidecar's channel cannot be opened: {e}"),
        }
    }
}

impl std::error::Error for RestoreError {}
