//! rr-dr60-workbench: a desktop power tool for the rr_dr60 engine.
//!
//! Spec 001, the offline render session: open a file, edit every manifest control, render through
//! a fresh one-pass engine with a tail, play the original and the render A/B on the engine's map,
//! export the render with its sidecar and reopen a sidecar. The logic is in `workbench_core`
//! (its `Session` holds the rules); this file is the thin egui view (constitution V).

mod capture;
mod playback;
mod views;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui;
use rr_dr60::control_map::ControlReading;
use rr_dr60::controls::ControlValue;
use workbench_core::ab::{self, AbMode};
use workbench_core::input::{self, Channel, SampleType};
use workbench_core::monitor::{Shared, Take, Takes};
use workbench_core::panel::{self, Panel, RowStatus};
use workbench_core::render::{self, Render};
use workbench_core::session::Session;
use workbench_core::{export, text};

use playback::Playback;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("rr-dr60 workbench")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "rr-dr60 workbench",
        options,
        Box::new(|_cc| Ok(Box::new(Workbench::new()))),
    )
}

/// A live run (spec 003): the output unit running the live core, the UI's queue ends, the take
/// being assembled, the start's request and the reasons controls are disabled.
struct LiveRun {
    playback: Playback,
    queues: workbench_core::live::LiveQueues,
    take: workbench_core::live::TakeBuilder,
    start_req: workbench_core::render::RenderRequest,
    reasons: Vec<Option<&'static str>>,
}

/// A render running on its own thread.
struct Job {
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    total: u64,
    rx: Receiver<Result<Option<Render>, String>>,
}

/// An opened processing record (spec 005 D3).
struct OpenedRecord {
    path: PathBuf,
    bytes: Vec<u8>,
    readout: rr_dr60::record::RecordReadout,
    check: Option<(u32, bool)>,
}

/// A re-render running on its own thread (spec 005 D4).
struct ReRenderJob {
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    total: u64,
    rx: Receiver<Result<workbench_core::records::Outcome, String>>,
    /// The note a "differs" carries under the record's flags (D4, C10).
    note: Option<String>,
}

/// A capture from the default input (spec 007): the input unit with the core in its callback,
/// the monitor's output unit, the UI's queue ends and the take being assembled.
struct CaptureRun {
    capture: capture::Capture,
    monitor: Option<Playback>,
    queues: workbench_core::live::LiveQueues,
    take: workbench_core::live::TakeBuilder,
    start_req: workbench_core::render::RenderRequest,
    gaps: Arc<workbench_core::capture::GapCounter>,
    underruns: Arc<AtomicU64>,
    silence: workbench_core::capture::SilenceWatch,
    seen: usize,
    latency: u32,
    note: Option<String>,
}

/// A live playback on a clone of the device (spec 004b).
struct LivePlay {
    playback: Playback,
    queues: workbench_core::live_play::PlayQueues,
    take: workbench_core::live_play::PlayTakeBuilder,
    /// The live controls' current readings, as the widgets show them.
    readings: Vec<(u32, rr_dr60::control_map::ControlReading)>,
}

struct Workbench {
    session: Session,
    job: Option<Job>,
    shared: Arc<Shared>,
    playback: Option<Playback>,
    ab_mode: AbMode,
    monitor_volume: f32,
    int_edits: HashMap<u32, String>,
    messages: Vec<String>,
    views: views::Views,
    live: Option<LiveRun>,
    live_play: Option<LivePlay>,
    device: Option<workbench_core::device::Device>,
    device_take: Option<workbench_core::device::PlayTake>,
    record_start_s: f64,
    device_item: Option<u32>,
    live_start_s: f64,
    schedule: Vec<(u32, f64, f64)>,
    sched_control: u32,
    sched_value: String,
    sched_time: String,
    record: Option<OpenedRecord>,
    check_item: u32,
    rerender: Option<ReRenderJob>,
    capture: Option<CaptureRun>,
    capture_channel: Channel,
    capture_monitor: bool,
    tour_page: usize,
    tour_pending: Option<()>,
    forms_cancel: Arc<AtomicBool>,
    forms_job: Option<Receiver<Vec<String>>>,
    forms_result: Vec<String>,
}

impl Workbench {
    fn new() -> Self {
        let shared = Arc::new(Shared::default());
        shared.set_volume(1.0);
        Self {
            session: Session::new(),
            job: None,
            shared,
            playback: None,
            ab_mode: AbMode::EngineMap,
            monitor_volume: 1.0,
            int_edits: HashMap::new(),
            messages: Vec::new(),
            views: views::Views::default(),
            live: None,
            live_play: None,
            device: None,
            device_take: None,
            record_start_s: 0.0,
            device_item: None,
            live_start_s: 0.0,
            schedule: Vec::new(),
            sched_control: 1,
            sched_value: String::new(),
            sched_time: String::new(),
            record: None,
            check_item: 1,
            rerender: None,
            capture: None,
            capture_channel: Channel::Index(0),
            capture_monitor: false,
            tour_page: 0,
            tour_pending: None,
            forms_cancel: Arc::new(AtomicBool::new(false)),
            forms_job: None,
            forms_result: Vec::new(),
        }
    }

    /// Starts a live run at the start position with the panel's values and the schedule (D1, D3).
    fn start_live(&mut self) {
        // A capture is finished before anything else changes the session (coach review 1).
        self.stop_capture(None);
        self.stop_playback();
        let Some(mut req) = self.session.plain_request() else {
            return;
        };
        req.start = (self.live_start_s * f64::from(req.rate_hz)).max(0.0) as u64;
        let list: Vec<(rr_dr60::changes::Clock, u64, u32, f64)> = self
            .schedule
            .iter()
            .map(|&(control, value, t)| {
                let clock =
                    rr_dr60::changes::clock_of(control).unwrap_or(rr_dr60::changes::Clock::Output);
                (
                    clock,
                    (t * f64::from(req.rate_hz)).round() as u64,
                    control,
                    value,
                )
            })
            .collect();
        let reasons = (0..rr_dr60::controls::controls().len() as u32)
            .map(|i| workbench_core::live::live_reason(&req, i))
            .collect();
        match workbench_core::live::LiveCore::new(&req, &list) {
            Ok((core, queues)) => {
                let mut core = core;
                match Playback::start_live(req.rate_hz, self.shared.clone(), move |o| core.fill(o))
                {
                    Ok(playback) => {
                        self.live = Some(LiveRun {
                            playback,
                            queues,
                            take: workbench_core::live::TakeBuilder::default(),
                            start_req: req,
                            reasons,
                        });
                        self.messages.clear();
                    }
                    Err(e) => self.messages = vec![e],
                }
            }
            Err(e) => self.messages = vec![e.to_string()],
        }
    }

    /// Stops the live run and makes its take the session's render (D5).
    fn stop_live(&mut self) {
        let Some(mut run) = self.live.take() else {
            return;
        };
        drop(run.playback);
        run.take.drain(&mut run.queues);
        let refused: Vec<String> = run
            .take
            .replies
            .iter()
            .filter_map(|r| r.error.map(|e| format!("A live change was refused: {e}")))
            .collect();
        let incomplete = run.queues.state.incomplete.load(Ordering::Relaxed);
        match run.take.finish(&run.start_req, &run.queues.state) {
            Ok(take) => {
                // The panel takes the take's end values (B6).
                self.session.accept_live_take(take);
                self.int_edits.clear();
                self.messages.extend(refused);
                if incomplete {
                    self.messages.push(text::LIVE_INCOMPLETE.to_owned());
                }
            }
            Err(e) => self.messages.push(e.to_string()),
        }
    }

    /// Plays item `n` live (spec 004b D1): the panel's values and the schedule list's playback
    /// entries, on a clone of the device in the output callback.
    fn start_live_play(&mut self, n: u32) {
        self.stop_playback();
        self.stop_live();
        let Some(dev) = &mut self.device else {
            return;
        };
        let rate = dev.settings().host_rate_hz;
        let (values, changes) = match self.session.panel() {
            Some(p) => workbench_core::device::play_request(p, &self.schedule, rate),
            None => (Vec::new(), Vec::new()),
        };
        match dev.live_player(n, &values, &changes) {
            Ok((mut player, queues)) => {
                let readings = workbench_core::live_play::readings_at_start(&queues.start);
                match Playback::start_live(rate, self.shared.clone(), move |o| player.fill(o)) {
                    Ok(playback) => {
                        self.live_play = Some(LivePlay {
                            playback,
                            queues,
                            take: workbench_core::live_play::PlayTakeBuilder::default(),
                            readings,
                        });
                        self.messages.clear();
                    }
                    Err(e) => self.messages = vec![e],
                }
            }
            Err(e) => self.messages = vec![e.to_string()],
        }
    }

    /// Stops a live playback; its take becomes the Device tab's playback take (D4).
    fn stop_live_play(&mut self) {
        let Some(mut run) = self.live_play.take() else {
            return;
        };
        drop(run.playback);
        run.take.drain(&mut run.queues);
        let n0 = run.take.replies.len();
        let _ = run.take.take_replies(&mut run.queues);
        let mut m: Vec<String> = run.take.replies[n0..]
            .iter()
            .filter_map(|r| r.error.map(|e| format!("A live change was refused: {e}")))
            .collect();
        let incomplete = run.queues.state.incomplete.load(Ordering::Relaxed);
        let take = std::mem::take(&mut run.take).finish(&run.queues);
        m.insert(
            0,
            format!(
                "Live playback of item {}: {} samples{}",
                take.number,
                take.samples.len(),
                if take.stopped_early {
                    " (stopped early: a replay compares this prefix)"
                } else {
                    ""
                }
            ),
        );
        if incomplete {
            m.push(text::LIVE_INCOMPLETE.to_owned());
        }
        self.messages = m;
        self.device_take = Some(take);
    }

    /// Stops every output stream: a live playback and the A/B (spec 004b D7), before another
    /// stream starts or the input changes.
    fn stop_playback(&mut self) {
        self.stop_live_play();
        self.stop_ab();
    }

    /// Starts a capture from the default input (spec 007 D1 – D4).
    fn start_capture(&mut self) {
        self.stop_playback();
        self.stop_live();
        let dev = match capture::default_input() {
            Ok(d) => d,
            Err(e) => {
                self.messages = vec![e];
                return;
            }
        };
        if let Some(why) = workbench_core::capture::rate_refusal(dev.rate) {
            self.messages = vec![format!(
                "{why} (some Bluetooth headset microphones run at 24 kHz, which the engine does not take)"
            )];
            return;
        }
        let rate = dev.rate as u32;
        let (req, note) =
            match workbench_core::live::capture_start_request(self.session.panel(), rate) {
                Ok(r) => r,
                Err(e) => {
                    self.messages = vec![e.to_string()];
                    return;
                }
            };
        let channel = match self.capture_channel {
            Channel::Index(i) if usize::from(i) >= dev.channels => Channel::Index(0),
            Channel::Mix if dev.channels == 1 => Channel::Index(0),
            c => c,
        };
        let fallback = (channel != self.capture_channel).then(|| {
            format!(
                "The input device has {} channel(s): channel 1 is used",
                dev.channels
            )
        });
        let list: Vec<(rr_dr60::changes::Clock, u64, u32, f64)> = self
            .schedule
            .iter()
            .map(|&(control, value, t)| {
                let clock =
                    rr_dr60::changes::clock_of(control).unwrap_or(rr_dr60::changes::Clock::Output);
                (clock, (t * f64::from(rate)).round() as u64, control, value)
            })
            .collect();
        let (core, mut queues) = match workbench_core::live::LiveCore::new_capture(
            &req,
            &list,
            dev.channels,
            channel,
            self.capture_monitor,
        ) {
            Ok(c) => c,
            Err(e) => {
                self.messages = vec![e.to_string()];
                return;
            }
        };
        let latency = render::fresh_engine(&req).map_or(0, |p| p.latency_samples());
        let underruns = Arc::new(AtomicU64::new(0));
        let monitor = match queues.monitor.take() {
            None => None,
            Some(mut q) => {
                let state = queues.state.clone();
                let under = underruns.clone();
                let block = render::block_for(rate);
                let (target, ceiling) = (rate as usize / 50, rate as usize / 10);
                let mut ms = workbench_core::capture::MonitorState::default();
                match Playback::start_live(rate, self.shared.clone(), move |o| {
                    workbench_core::capture::monitor_fill(
                        &mut q,
                        o,
                        &mut ms,
                        target,
                        ceiling,
                        block,
                        state.paused.load(Ordering::Relaxed),
                        &under,
                        &state.monitor_ceiling_drops,
                    );
                }) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        self.messages = vec![e];
                        return;
                    }
                }
            }
        };
        let gaps = Arc::new(workbench_core::capture::GapCounter::default());
        match capture::Capture::start(dev, core, gaps.clone()) {
            Ok(c) => {
                self.messages = note.iter().cloned().chain(fallback).collect();
                self.capture = Some(CaptureRun {
                    capture: c,
                    monitor,
                    queues,
                    take: workbench_core::live::TakeBuilder::default(),
                    start_req: req,
                    gaps,
                    underruns,
                    silence: workbench_core::capture::SilenceWatch::new(rate),
                    seen: 0,
                    latency,
                    note,
                });
            }
            Err(e) => self.messages = vec![e],
        }
    }

    /// Stops the capture (spec 007 D5): the input unit first, then the queues drained; the
    /// capture written and opened as the session's input, the take accepted. `why` heads the
    /// messages (a lost device, the length limit).
    fn stop_capture(&mut self, why: Option<&str>) {
        let Some(mut run) = self.capture.take() else {
            return;
        };
        drop(run.capture);
        drop(run.monitor);
        run.take.drain(&mut run.queues);
        let staged = run.queues.state.staged.load(Ordering::Relaxed);
        let mut m: Vec<String> = why.iter().map(|w| (*w).to_owned()).collect();
        let state = &run.queues.state;
        let rate = run.start_req.rate_hz;
        if run.take.captured.is_empty() {
            m.push("Stopped before the first 10 ms chunk: no take".into());
            self.messages = m;
            return;
        }
        let gaps = run.gaps.gaps.load(Ordering::Relaxed);
        if gaps > 0 {
            m.push(format!(
                "The input device lost {} frames in {gaps} gaps (the take reproduces; the room's sound has holes there)",
                run.gaps.missing.load(Ordering::Relaxed)
            ));
        }
        if staged > 0 {
            m.push(format!(
                "{staged} samples (under 10 ms) after the last whole chunk are not in the capture or the take"
            ));
        }
        m.push(format!(
            "The engine's last {} samples ({:.1} ms, its latency) are not in the take: the tail is off",
            run.latency,
            f64::from(run.latency) * 1000.0 / f64::from(rate)
        ));
        let incomplete = state.incomplete.load(Ordering::Relaxed);
        let req = run.take.capture_request(&run.start_req);
        let dir = workbench_core::capture::folder();
        if let Err(e) = std::fs::create_dir_all(&dir) {
            m.push(format!("{}: {e}", text::OPEN_IO));
            self.messages = m;
            return;
        }
        let path = workbench_core::capture::next_path(&dir);
        let wav = match workbench_core::live::capture_wav(&run.take.captured, rate) {
            Ok(w) => w,
            Err(e) => {
                m.push(e.to_string());
                self.messages = m;
                return;
            }
        };
        if let Err(e) = std::fs::write(&path, &wav) {
            m.push(format!("{}: {e}", text::OPEN_IO));
            self.messages = m;
            return;
        }
        let mut req = req;
        req.file_name = path
            .file_name()
            .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
        match run.take.finish(&req, state) {
            Ok(take) => {
                let n = take.samples.len();
                match self.session.accept_capture(&path, take) {
                    Ok(true) => {
                        m.push(format!(
                            "Captured {} input samples into {} ({n} output samples). The capture replaced the open file; its render, B and any restored range are dropped.",
                            req.frames,
                            path.display()
                        ));
                        if incomplete {
                            m.push(text::LIVE_INCOMPLETE.to_owned());
                        }
                    }
                    Ok(false) => m.push("The take did not match the capture file".into()),
                    Err(e) => m.push(e.to_string()),
                }
                self.int_edits.clear();
            }
            Err(e) => m.push(e.to_string()),
        }
        self.messages = m;
    }

    /// The one-pass call-form check (spec 006 D1), on its own thread.
    fn start_forms_check(&mut self, ctx: &egui::Context) {
        let Some(req) = self.session.request() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        self.forms_cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.forms_cancel.clone();
        std::thread::spawn(move || {
            let c = workbench_core::checks::one_pass_check(&req, &cancel);
            let mut m: Vec<String> = c
                .forms
                .iter()
                .map(|f| match &f.run {
                    Ok(_) if f.agrees => format!("{}: agrees with the render", f.form.name()),
                    Ok(_) => format!(
                        "{}: DISAGREES with the render (an engine finding)",
                        f.form.name()
                    ),
                    Err(e) => format!("{}: {e}", f.form.name()),
                })
                .collect();
            m.push(match &c.reset {
                Ok(r) if r.agrees => "reset, then render again: agrees with a fresh engine built from the state after the first render".into(),
                Ok(_) => "reset, then render again: DISAGREES with a fresh engine built from the state (an engine finding)".into(),
                Err(e) => format!("reset: {e}"),
            });
            let _ = tx.send(m);
            ctx2.request_repaint();
        });
        self.forms_job = Some(rx);
        self.forms_result = vec!["Checking the call forms…".into()];
    }

    /// The guided tour (spec 006 D4): the stage manifest stage by stage.
    fn tour_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let pages = workbench_core::tour::pages();
        self.tour_page = self.tour_page.min(pages.len() - 1);
        ui.horizontal(|ui| {
            if ui.button("Previous").clicked() && self.tour_page > 0 {
                self.tour_page -= 1;
            }
            egui::ComboBox::from_id_salt("tour_page")
                .selected_text(format!("{}", pages[self.tour_page]))
                .show_ui(ui, |ui| {
                    for (k, n) in pages.iter().enumerate() {
                        let name = workbench_core::stages::stages()
                            .iter()
                            .find(|e| e.number == *n)
                            .map_or("", |e| e.name);
                        ui.selectable_value(&mut self.tour_page, k, format!("{n} {name}"));
                    }
                });
            if ui.button("Next").clicked() && self.tour_page + 1 < pages.len() {
                self.tour_page += 1;
            }
        });
        let n = pages[self.tour_page];
        if let Some(e) = workbench_core::stages::stages()
            .iter()
            .find(|e| e.number == n)
        {
            egui::Grid::new("tour_fields").striped(true).show(ui, |ui| {
                for (k, v) in workbench_core::stages::stage_fields(e) {
                    ui.label(k);
                    ui.label(v);
                    ui.end_row();
                }
            });
        }
        ui.separator();
        let panel_values: Vec<(u32, ControlReading)> = self
            .session
            .panel()
            .map(|p| p.rows().iter().map(|r| (r.index, r.value)).collect())
            .unwrap_or_default();
        egui::Grid::new("tour_controls")
            .striped(true)
            .show(ui, |ui| {
                for e in workbench_core::tour::controls(n) {
                    ui.label(e.identifier);
                    let i = rr_dr60::controls::controls()
                        .iter()
                        .position(|x| x.identifier == e.identifier)
                        .unwrap_or(0) as u32;
                    ui.monospace(
                        panel_values
                            .iter()
                            .find(|(j, _)| *j == i)
                            .map_or_else(String::new, |(_, r)| {
                                workbench_core::sidecar::fmt_reading(*r)
                            }),
                    );
                    ui.label(e.description);
                    ui.end_row();
                }
            });
        if workbench_core::tour::switches(n).is_empty() {
            ui.label(workbench_core::tour::NotOffered::NoSwitch.to_string());
            return;
        }
        ui.separator();
        if ui
            .add_enabled(
                self.job.is_none(),
                egui::Button::new("Compare without this stage"),
            )
            .clicked()
        {
            let (Some(r), Some(cur)) = (self.session.render(), self.session.request()) else {
                self.messages = vec!["Render first".into()];
                return;
            };
            match workbench_core::tour::without_stage(r, &cur, n) {
                Err(e) => self.messages = vec![e.to_string()],
                Ok(w) => {
                    let mut m = Vec::new();
                    if self.session.b().is_some() {
                        m.push("B is replaced by the current render (with the stage)".to_owned());
                    }
                    self.stop_ab();
                    self.session.keep_as_b();
                    if let Some(p) = self.session.panel_mut() {
                        p.set_many(
                            &w.turned_off
                                .iter()
                                .map(|(i, _)| (*i, ControlReading::Switch(false)))
                                .collect::<Vec<_>>(),
                        );
                    }
                    m.push(format!(
                        "The panel's {} switched off; rendering A without the stage",
                        w.turned_off
                            .iter()
                            .map(|(_, id)| *id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                    if !w.removed.is_empty() {
                        m.push(format!(
                            "{} scheduled change(s) of these switches are left out of A",
                            w.removed.len()
                        ));
                    }
                    // A comparison notice of the workbench's UI, keyed on the request, not on
                    // a stage number (coach review 8).
                    let gate_off = w.turned_off.iter().any(|(_, id)| *id == "vas.enabled");
                    if gate_off && w.request.settings.vas.mode == rr_dr60::VasMode::Drop {
                        m.push("With the gate off in drop mode, A and B do not line up sample for sample".into());
                    }
                    self.messages = m;
                    self.tour_pending = Some(());
                    self.start_render_with(ctx, w.request);
                    self.views.tab = views::Tab::Compare;
                }
            }
        }
    }

    /// The capture's section of the Session tab (spec 007).
    fn capture_section(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.strong("Capture from the Mac's default input (spec 007)");
        let idle = self.live.is_none() && self.live_play.is_none();
        ui.horizontal(|ui| {
            ui.label("channel");
            let label = |c: Channel| match c {
                Channel::Index(i) => format!("{}", i + 1),
                Channel::Mix => "mix (the mean of all the device's channels, silent ones included: a boundary conversion)".to_owned(),
            };
            egui::ComboBox::from_id_salt("capture_channel")
                .selected_text(label(self.capture_channel))
                .show_ui(ui, |ui| {
                    for i in 0..8u16 {
                        ui.selectable_value(&mut self.capture_channel, Channel::Index(i), label(Channel::Index(i)));
                    }
                    ui.selectable_value(&mut self.capture_channel, Channel::Mix, label(Channel::Mix));
                });
            ui.add_enabled(
                self.capture.is_none(),
                egui::Checkbox::new(&mut self.capture_monitor, "Monitor"),
            )
            .on_hover_text("Plays the engine's output on the default output device. Use headphones: the microphone hears the speakers. A Bluetooth headset's microphone may switch its output to the headset profile.");
            if self.capture.is_none() {
                if ui.add_enabled(idle, egui::Button::new("Capture")).clicked() {
                    self.start_capture();
                }
            } else if ui.button("Stop capture").clicked() {
                self.stop_capture(None);
            }
            if let Some(p) = self.session.path().map(Path::to_path_buf)
                && p.starts_with(workbench_core::capture::folder())
                && ui.button("Save capture…").clicked()
                && let Some(to) = rfd::FileDialog::new()
                    .set_file_name(p.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned()))
                    .save_file()
            {
                self.messages = vec![match std::fs::copy(&p, &to) {
                    Ok(_) => format!("Saved the capture as {}", to.display()),
                    Err(e) => format!("{}: {e}", text::OPEN_IO),
                }];
            }
        });
        if let Some(run) = &self.capture {
            let st = &run.queues.state;
            let d = run.capture.device();
            ui.label(format!(
                "{} · {} Hz · {} ch · fed {} ({:.1} s) · gaps {} · monitor samples dropped: queue full {}, above 100 ms {} · underrun samples {}{}",
                d.name,
                d.rate,
                d.channels,
                st.fed.load(Ordering::Relaxed),
                st.fed.load(Ordering::Relaxed) as f64 / d.rate,
                run.gaps.gaps.load(Ordering::Relaxed),
                st.monitor_drops.load(Ordering::Relaxed),
                st.monitor_ceiling_drops.load(Ordering::Relaxed),
                run.underruns.load(Ordering::Relaxed),
                if st.incomplete.load(Ordering::Relaxed) { " · incomplete" } else { "" }
            ));
            if let Some(n) = &run.note {
                ui.label(n);
            }
            if run.silence.silent() {
                ui.label("The input is digital silence: check the microphone permission (System Settings → Privacy & Security → Microphone, for the app the workbench runs from)");
            }
        }
    }

    /// Stops the A/B playback only (spec 004b review B4: a finished render does not end a
    /// live playback).
    fn stop_ab(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        // Dropping stops the unit before the takes are released (plan P3).
        self.playback = None;
    }

    fn open_file(&mut self, path: PathBuf, channel: Channel) {
        // A capture is finished, its take handed over, before the user's file opens (coach 1); its
        // messages are kept.
        let had_capture = self.capture.is_some();
        self.stop_capture(None);
        let capture_messages = if had_capture {
            std::mem::take(&mut self.messages)
        } else {
            Vec::new()
        };
        self.stop_live();
        let before = self.session.input().map(|i| i.raw_sha256);
        match self.session.open(&path, channel) {
            Ok(()) => {
                // A render of the old input must not land on the new one (coach 1).
                if let Some(job) = self.job.take() {
                    job.cancel.store(true, Ordering::Relaxed);
                }
                self.stop_playback();
                if self.session.input().map(|i| i.raw_sha256) != before {
                    self.int_edits.clear();
                }
                self.messages = capture_messages;
            }
            Err(e) => self.messages = vec![e.to_string()],
        }
    }

    fn start_render(&mut self, ctx: &egui::Context) {
        let Some(req) = self.session.request() else {
            return;
        };
        self.start_render_with(ctx, req);
    }

    /// Renders `req` (spec 006 coach 1: the tour's request, which may differ from the panel's in
    /// its schedule).
    fn start_render_with(&mut self, ctx: &egui::Context, req: render::RenderRequest) {
        let progress = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c, ctx2) = (progress.clone(), cancel.clone(), ctx.clone());
        let total = req.input.len() as u64;
        std::thread::spawn(move || {
            let r = render::render(&req, &p, &c).map_err(|e| e.to_string());
            let _ = tx.send(r);
            ctx2.request_repaint();
        });
        self.job = Some(Job {
            progress,
            cancel,
            total,
            rx,
        });
    }

    fn poll_job(&mut self) {
        let Some(job) = &self.job else {
            return;
        };
        if let Ok(result) = job.rx.try_recv() {
            self.job = None;
            match result {
                Ok(Some(r)) => {
                    self.stop_ab();
                    let pending = self.tour_pending.take();
                    let latency_a = r.latency_samples;
                    if !self.session.accept_render(r) {
                        self.messages = vec!["A render of another file was discarded".into()];
                    } else if let (Some(()), Some(a), Some(b)) =
                        (pending, self.session.render(), self.session.b())
                    {
                        // The tour's comparison (spec 006 D4; reviews coach 2, hardware B2).
                        if workbench_core::tour::no_effect(b, a) {
                            self.messages.push(
                                "No effect at this tap and route: A and B are bit-identical".into(),
                            );
                        }
                        if latency_a != b.latency_samples {
                            self.messages.push(format!(
                                "A and B are offset by {} host samples (the engine's latency changes without the stage): the Compare tab aligns them by latency",
                                i64::from(b.latency_samples) - i64::from(latency_a)
                            ));
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => self.messages = vec![e],
            }
        }
    }

    fn play(&mut self, take: Take) {
        let Some(inp) = self.session.input() else {
            return;
        };
        let render: Arc<[f32]> = self
            .session
            .render()
            .map_or_else(|| Arc::from(Vec::new()), |r| r.samples.clone());
        let b: Arc<[f32]> = self
            .session
            .b()
            .map_or_else(|| Arc::from(Vec::new()), |r| r.samples.clone());
        let original = inp.samples.clone();
        let rate = inp.rate_hz;
        if self.playback.is_none() {
            let takes = Takes {
                original,
                render,
                b,
                rate_hz: rate,
            };
            match Playback::start(takes, self.shared.clone()) {
                Ok(p) => self.playback = Some(p),
                Err(e) => {
                    self.messages = vec![e];
                    return;
                }
            }
            self.shared.request(take, 0);
        } else if self.shared.cursor().0 != take {
            // Playing another take goes through the input (hardware review B2; spec 005b D5).
            self.switch_to(take);
        }
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    /// The A/B switch (D6; spec 005b D5): original → render → B (when kept) → original.
    fn switch(&mut self) {
        let next = match self.shared.cursor().0 {
            Take::Original => Take::Render,
            Take::Render if self.session.b().is_some() => Take::B,
            _ => Take::Original,
        };
        self.switch_to(next);
    }

    /// Moves the playback to `to` at the moment the current take is at, through the input.
    fn switch_to(&mut self, to: Take) {
        let Some(inp) = self.session.input() else {
            return;
        };
        if self.playback.is_none() || self.session.render().is_none() {
            return;
        }
        let (from, pos) = self.shared.cursor();
        let mapped = ab::switch(
            from,
            pos,
            to,
            self.session.render(),
            self.session.b(),
            inp.samples.len() as u64,
            self.ab_mode,
        );
        self.shared.request_from(to, mapped, pos);
    }

    fn export(&mut self) {
        let Some(r) = self.session.render() else {
            return;
        };
        let stem = Path::new(&r.request.file_name)
            .file_stem()
            .map_or_else(|| "render".to_owned(), |s| s.to_string_lossy().into_owned());
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{stem}-render.wav"))
            .save_file()
        else {
            return;
        };
        let (wav, side) = export::paths(&path);
        if side.exists() && !wav.exists() {
            let ok = rfd::MessageDialog::new()
                .set_description(format!("Replace the sidecar {}?", side.display()))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if ok != rfd::MessageDialogResult::Yes {
                return;
            }
        }
        let mut m = vec![match export::export(&mut export::RealFs, &path, r) {
            Ok(()) => format!("Exported {} and {}", wav.display(), side.display()),
            Err(e) => e.to_string(),
        }];
        if self.session.is_stale() {
            m.push(text::STALE_EXPORT.to_owned());
        }
        self.messages = m;
    }

    fn open_sidecar(&mut self) {
        // A capture is finished before the restore changes the session (coach review 1).
        self.stop_capture(None);
        let Some(path) = rfd::FileDialog::new()
            .add_filter("sidecar", &["txt"])
            .pick_file()
        else {
            return;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.messages = vec![format!("{}: {e}", text::OPEN_IO)];
                return;
            }
        };
        let rep = match self.session.restore(&content) {
            Ok(rep) => rep,
            Err(e) => {
                self.messages = vec![e.to_string()];
                return;
            }
        };
        self.stop_playback();
        self.int_edits.clear();
        let mut m = vec![format!(
            "Restored {} controls from {}",
            rep.applied,
            path.display()
        )];
        if let Some(c) = &rep.commit_differs {
            m.push(format!(
                "Made with engine commit {c}; this build is {}: the render may differ",
                workbench_core::engine_commit()
            ));
        }
        if rep.input_differs {
            m.push("The input differs from the sidecar's (its converted samples' digest)".into());
        }
        for id in &rep.unknown_ids {
            m.push(format!("Skipped {id}: this engine does not know it"));
        }
        for id in &rep.missing_controls {
            m.push(format!("{id} is not in the sidecar: left at its default"));
        }
        for (id, why) in &rep.refused {
            m.push(format!("{id} not applied: {why}"));
        }
        if let Some(r) = &rep.rate_differs {
            m.push(format!(
                "The sidecar's rate was {r} Hz; the session uses the file's rate"
            ));
        }
        if !rep.renderable {
            m.push("The restored values are not renderable as they stand: see the panel".into());
        }
        self.messages = m;
    }
}

fn secs(n: u64, rate: u32) -> String {
    format!("{n} samples ({:.3} s)", n as f64 / f64::from(rate.max(1)))
}

impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_job();
        // The checks' results (spec 006), wherever the user is.
        if let Some(rx) = &self.forms_job {
            if let Ok(m) = rx.try_recv() {
                self.messages = m.clone();
                self.forms_result = m;
                self.forms_job = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        if self.playback.as_ref().is_some_and(Playback::device_lost) {
            self.stop_ab();
            self.messages = vec![text::DEVICE_LOST.to_owned()];
        }
        self.views.poll(&ctx);
        if let Some(run) = &mut self.capture {
            run.take.drain(&mut run.queues);
            let new = &run.take.captured[run.seen..];
            run.silence.observe(new);
            run.seen = run.take.captured.len();
            let lost = run.capture.lost();
            // The monitor's own output device (review B7): the take continues without it.
            if run.monitor.as_ref().is_some_and(Playback::device_lost) {
                run.monitor = None;
                self.messages.push("The monitor's output device changed: monitoring stopped, the capture continues".into());
            }
            let failed = run.queues.state.failed.load(Ordering::Relaxed);
            // The export's limit (coach review 9): stop at it.
            let full = export::check_size(
                run.take.captured.len() as u64 + 10 * u64::from(run.start_req.rate_hz),
            )
            .is_err();
            if let Some(why) = lost {
                self.stop_capture(Some(why));
            } else if failed {
                self.stop_capture(Some("The engine refused a chunk: the capture stopped"));
            } else if full {
                self.stop_capture(Some(
                    "The capture reached the WAV export's limit and stopped",
                ));
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(run) = &mut self.live {
            run.take.drain(&mut run.queues);
            // Stop once every sample has been played (B11), on a failure (B2) or a lost device.
            let drained = run.queues.state.drained.load(Ordering::Relaxed);
            let failed = run.queues.state.failed.load(Ordering::Relaxed);
            let lost = run.playback.device_lost();
            if drained || failed || lost {
                self.messages.clear();
                self.stop_live();
                if lost {
                    self.messages.insert(0, text::DEVICE_LOST.to_owned());
                }
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(run) = &mut self.live_play {
            let log0 = run.take.log.len();
            run.take.drain(&mut run.queues);
            // The widgets follow every landed change, from the list or a command (B9).
            for e in &run.take.log[log0..] {
                if let Some(slot) = run.readings.iter_mut().find(|(i, _)| *i == e.control) {
                    slot.1 = workbench_core::live_play::reading_of(e.control, e.after);
                }
            }
            // The refusals as they arrive, whichever tab is shown (B2).
            let replies = run.take.take_replies(&mut run.queues).to_vec();
            let mut refused = Vec::new();
            for r in replies.iter().filter(|r| r.error.is_some()) {
                refused.push(format!(
                    "A live change was refused: {}",
                    r.error.map_or_else(String::new, |e| e.to_string())
                ));
                // The widget returns to the value in force: the last landing, else the start.
                let last = run
                    .take
                    .log
                    .iter()
                    .rev()
                    .find(|e| e.control == r.control)
                    .map(|e| workbench_core::live_play::reading_of(e.control, e.after))
                    .or_else(|| {
                        workbench_core::live_play::readings_at_start(&run.queues.start)
                            .into_iter()
                            .find(|(i, _)| *i == r.control)
                            .map(|(_, v)| v)
                    });
                if let (Some(v), Some(slot)) =
                    (last, run.readings.iter_mut().find(|(i, _)| *i == r.control))
                {
                    slot.1 = v;
                }
            }
            if !refused.is_empty() {
                self.messages = refused;
            }
            let st = &run.queues.state;
            let done = st.drained.load(Ordering::Relaxed) || st.failed.load(Ordering::Relaxed);
            let lost = run.playback.device_lost();
            if done || lost {
                self.stop_live_play();
                if lost {
                    self.messages.insert(0, text::DEVICE_LOST.to_owned());
                }
            }
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.job.is_some() || self.playback.is_some() || self.views.busy() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.live.is_none()
            && self.live_play.is_none()
            && ctx.input(|i| i.key_pressed(egui::Key::Space))
        {
            self.switch();
        }
        egui::Panel::top("file").show(ui, |ui| self.file_bar(ui));
        egui::Panel::left("controls")
            .resizable(true)
            .default_size(560.0)
            .show(ui, |ui| self.controls_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (t, name) in [
                    (views::Tab::Session, "Session"),
                    (views::Tab::Scopes, "Scopes"),
                    (views::Tab::Meters, "Meters"),
                    (views::Tab::Taps, "Taps"),
                    (views::Tab::Stages, "Stages"),
                    (views::Tab::Device, "Device"),
                    (views::Tab::Compare, "Compare"),
                    (views::Tab::Tour, "Tour"),
                ] {
                    ui.selectable_value(&mut self.views.tab, t, name);
                }
            });
            for m in &self.views.messages {
                ui.label(m);
            }
            ui.separator();
            let playhead = self.playback.as_ref().map(|_| self.shared.cursor());
            // The playhead in input positions, through the engine's map (spec 005b D5).
            let playhead = playhead.and_then(|(t, p)| {
                let inp = self.session.input()?;
                Some(ab::switch(
                    t,
                    p,
                    Take::Original,
                    self.session.render(),
                    self.session.b(),
                    inp.samples.len() as u64,
                    AbMode::EngineMap,
                ))
            });
            match self.views.tab {
                views::Tab::Session => self.session_view(ui, &ctx),
                views::Tab::Scopes => self.views.scopes(ui, &self.session, playhead),
                views::Tab::Meters => self.views.meters(ui, &self.session),
                views::Tab::Taps => self.views.taps(ui, &self.session),
                views::Tab::Stages => self.views.stages(ui),
                views::Tab::Compare => self.views.compare(ui, &self.session),
                views::Tab::Tour => self.tour_view(ui, &ctx),
                views::Tab::Device => {
                    self.device_view(ui);
                    ui.separator();
                    self.records_view(ui, &ctx);
                }
            }
        });
    }
}

impl Workbench {
    fn file_bar(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<(PathBuf, Channel)> = None;
        let mut sidecar = false;
        let mut preset: Option<bool> = None;
        ui.horizontal(|ui| {
            ui.strong(format!(
                "rr_dr60 {} ({})",
                workbench_core::engine_version(),
                &workbench_core::engine_commit()[..7]
            ));
            if ui.button("Open audio…").clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("audio", &["wav", "aif", "aiff", "aifc", "flac"])
                    .pick_file()
            {
                open = Some((p, Channel::Index(0)));
            }
            if let Some(inp) = self.session.input() {
                ui.label(format!(
                    "{} · {:?} {:?} · {} Hz · {} ch · {}",
                    inp.file_name,
                    inp.format,
                    inp.sample_type,
                    inp.rate_hz,
                    inp.channels,
                    secs(inp.frames, inp.rate_hz)
                ));
                let current = inp.channel;
                let mut chosen = current;
                let label = |c: Channel| match c {
                    Channel::Index(i) => format!("Channel {}", i + 1),
                    Channel::Mix => "Mix".to_owned(),
                };
                egui::ComboBox::from_id_salt("channel")
                    .selected_text(label(current))
                    .show_ui(ui, |ui| {
                        for i in 0..inp.channels {
                            ui.selectable_value(
                                &mut chosen,
                                Channel::Index(i),
                                label(Channel::Index(i)),
                            );
                        }
                        if inp.channels > 1 {
                            ui.selectable_value(&mut chosen, Channel::Mix, label(Channel::Mix))
                                .on_hover_text(text::MIX_LABEL);
                        }
                    });
                if chosen != current
                    && let Some(p) = self.session.path()
                {
                    open = Some((p.to_path_buf(), chosen));
                }
            }
            if ui.button("Open sidecar…").clicked() {
                sidecar = true;
            }
            if ui
                .add_enabled(
                    self.session.panel().is_some(),
                    egui::Button::new("Save preset…"),
                )
                .clicked()
            {
                preset = Some(true);
            }
            if ui
                .add_enabled(
                    self.session.panel().is_some(),
                    egui::Button::new("Load preset…"),
                )
                .on_disabled_hover_text("Open an input first: its file decides the rate")
                .clicked()
            {
                preset = Some(false);
            }
        });
        if let Some((p, c)) = open {
            self.open_file(p, c);
        }
        if sidecar {
            self.open_sidecar();
        }
        match preset {
            Some(true) => self.save_preset(),
            Some(false) => self.load_preset(),
            None => {}
        }
    }

    /// Writes the panel's values as a preset (spec 005b D6).
    fn save_preset(&mut self) {
        let Some(panel) = self.session.panel() else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("preset.txt")
            .save_file()
        else {
            return;
        };
        self.messages = vec![
            match std::fs::write(&path, workbench_core::preset::write(panel)) {
                Ok(()) => format!("Saved the panel's values as a preset: {}", path.display()),
                Err(e) => format!("{}: {e}", text::OPEN_IO),
            },
        ];
    }

    /// Loads a preset into the panel by the sidecar's restore rules (spec 005b D6).
    fn load_preset(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("preset", &["txt"])
            .pick_file()
        else {
            return;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.messages = vec![format!("{}: {e}", text::OPEN_IO)];
                return;
            }
        };
        let rep = match self.session.load_preset(&content) {
            Ok(r) => r,
            Err(e) => {
                self.messages = vec![e.to_string()];
                return;
            }
        };
        self.int_edits.clear();
        let mut m = vec![format!(
            "Loaded {} controls from the preset {}",
            rep.applied,
            path.display()
        )];
        if let Some(c) = &rep.commit_differs {
            m.push(format!(
                "Saved with engine commit {c}; this build is {}",
                workbench_core::engine_commit()
            ));
        }
        for id in &rep.unknown_ids {
            m.push(format!("Skipped {id}: this engine does not know it"));
        }
        for id in &rep.missing_controls {
            m.push(format!("{id} is not in the preset: left at its default"));
        }
        for (id, why) in &rep.refused {
            m.push(format!("{id} not applied: {why}"));
        }
        if let Some(r) = &rep.rate_differs {
            m.push(format!(
                "The preset's rate was {r} Hz; the session uses the file's rate"
            ));
        }
        self.messages = m;
    }

    fn controls_panel(&mut self, ui: &mut egui::Ui) {
        let int_edits = &mut self.int_edits;
        let live_reasons: Option<Vec<Option<&'static str>>> =
            self.live.as_ref().map(|r| r.reasons.clone());
        let Some(panel) = self.session.panel_mut() else {
            ui.label("Open a file to edit the engine's controls.");
            return;
        };
        if let Some(why) = panel.general_refusal() {
            ui.colored_label(egui::Color32::LIGHT_RED, why);
        }
        ui.horizontal(|ui| {
            ui.heading(format!("{} controls", panel.rows().len()));
            if ui.button("Reset to defaults").clicked() {
                panel.reset_to_defaults();
                int_edits.clear();
            }
        });
        let mut stages: Vec<u8> = panel.rows().iter().map(|r| r.entry.stage).collect();
        stages.sort_unstable();
        stages.dedup();
        let mut edits: Vec<(u32, ControlReading)> = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for stage in stages {
                egui::CollapsingHeader::new(panel::stage_title(stage))
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new(("grid", stage))
                            .num_columns(3)
                            .striped(true)
                            .show(ui, |ui| {
                                for row in panel.rows().iter().filter(|r| r.entry.stage == stage) {
                                    let e = &row.entry;
                                    let hover = format!(
                                        "{}\n\n{}\nkind: {} · timing: {}\nsources: {}\nassumptions: {}",
                                        e.identifier,
                                        e.description,
                                        text::kind_label(e.kind),
                                        text::timing_label(e.timing),
                                        e.sources.join(", "),
                                        e.assumptions.join(", ")
                                    );
                                    ui.label(e.name).on_hover_text(hover);
                                    let live_reason = live_reasons
                                        .as_ref()
                                        .and_then(|r| r.get(row.index as usize).copied().flatten());
                                    let enabled = matches!(
                                        row.status,
                                        RowStatus::Accepted | RowStatus::Refused(_)
                                    ) && live_reason.is_none();
                                    if let Some(why) = live_reason {
                                        ui.weak(why);
                                    }
                                    ui.add_enabled_ui(enabled, |ui| {
                                        if let Some(v) =
                                            editor(ui, row.index, &e.value, row.value, e.unit, int_edits)
                                        {
                                            edits.push((row.index, v));
                                        }
                                    });
                                    match &row.status {
                                        RowStatus::Accepted => ui.label(""),
                                        RowStatus::Refused(why) => {
                                            ui.colored_label(egui::Color32::LIGHT_RED, why)
                                        }
                                        RowStatus::NotOnOnePass => ui.weak(text::NOT_ON_ONE_PASS),
                                        RowStatus::ReadOnly => ui.weak(text::HOST_RATE_READ_ONLY),
                                    };
                                    ui.end_row();
                                }
                            });
                    });
            }
        });
        for (i, v) in edits {
            panel.set(i, v);
            if let Some(run) = &mut self.live
                && run
                    .queues
                    .commands
                    .push(workbench_core::live::Command {
                        control: i,
                        value: workbench_core::live::value_of(v),
                    })
                    .is_err()
            {
                self.messages = vec!["The change queue is full: try again".into()];
            }
        }
    }

    fn session_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        for m in &self.messages {
            ui.label(m);
        }
        let Some(inp) = self.session.input() else {
            ui.heading("Open a WAV, AIFF or FLAC file at one of the engine's rates.");
            ui.label(text::supported_rates());
            return;
        };
        let rate = inp.rate_hz;
        if inp.sample_type == SampleType::I32 {
            ui.weak(text::I32_ROUNDING);
        }
        if inp.channel == Channel::Mix {
            ui.weak(text::MIX_LABEL);
        }
        if inp.non_finite + inp.subnormal > 0 {
            ui.label(format!(
                "{} non-finite and {} subnormal input samples: the engine zeroes them",
                inp.non_finite, inp.subnormal
            ));
        }
        let input_peak = inp.samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        if input_peak > 1.0 {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Original: {}", text::PEAK_WARNING),
            );
            ui.weak(text::headroom_hint(input_peak));
        }
        ui.separator();
        let renderable = self.session.panel().is_some_and(Panel::renderable);
        let mut start = false;
        let mut tail = self.session.tail();
        ui.horizontal(|ui| {
            ui.checkbox(&mut tail, "Tail")
                .on_hover_text(text::TAIL_LABEL);
            if let Some(job) = &self.job {
                let done = job.progress.load(Ordering::Relaxed);
                ui.add(
                    egui::ProgressBar::new(done as f32 / job.total.max(1) as f32).show_percentage(),
                );
                if ui.button("Cancel").clicked() {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            } else if ui
                .add_enabled(renderable, egui::Button::new("Render"))
                .clicked()
            {
                start = true;
            }
        });
        self.session.set_tail(tail);
        if start {
            self.start_render(ctx);
        }
        let stale = self.session.is_stale();
        let Some(r) = self.session.render() else {
            ui.label("No render yet.");
            return;
        };
        if stale {
            ui.colored_label(egui::Color32::YELLOW, text::STALE);
        }
        egui::Grid::new("readings").num_columns(2).show(ui, |ui| {
            ui.label("Length");
            ui.label(secs(r.samples.len() as u64, rate));
            ui.end_row();
            ui.label("Latency (the engine's)");
            ui.label(secs(u64::from(r.latency_samples), rate));
            ui.end_row();
            ui.label("Tail").on_hover_text(text::TAIL_LABEL);
            ui.label(secs(r.tail_samples, rate));
            ui.end_row();
            if r.gate_reached {
                ui.label(if r.drop_mode {
                    text::SPLICES
                } else {
                    text::MUTED_REGIONS
                });
                ui.label(r.splices_or_regions.to_string());
                ui.end_row();
                ui.label("Paused at end");
                ui.label(if r.paused_at_end { "yes" } else { "no" });
                ui.end_row();
            } else {
                ui.label(text::GATE_NOT_REACHED);
                ui.label("");
                ui.end_row();
            }
            ui.label(text::NOT_EMITTED);
            ui.label(secs(r.not_emitted, rate));
            ui.end_row();
            ui.label(text::PEAK_LABEL);
            ui.label(format!("{:.4}", r.peak));
            ui.end_row();
            ui.label("Output SHA-256");
            ui.monospace(input::hex(&r.output_sha256));
            ui.end_row();
            ui.label("Engine footprint");
            ui.label(format!("{} bytes", r.footprint_bytes));
            ui.end_row();
        });
        if r.peak > 1.0 {
            ui.colored_label(egui::Color32::LIGHT_RED, text::PEAK_WARNING);
            ui.weak(text::headroom_hint(r.peak));
        }
        ui.separator();
        let mut play: Option<Take> = None;
        let mut switch = false;
        let mut stop = false;
        let mut export = false;
        let (take, pos) = self.shared.cursor();
        // One output stream: A/B playback is off while live (spec 003 D9).
        let idle = self.live.is_none() && self.capture.is_none();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(idle, egui::Button::new("Play original"))
                .clicked()
            {
                play = Some(Take::Original);
            }
            if ui
                .add_enabled(idle, egui::Button::new("Play render"))
                .clicked()
            {
                play = Some(Take::Render);
            }
            if ui
                .add_enabled(
                    idle && self.session.b().is_some(),
                    egui::Button::new("Play B"),
                )
                .clicked()
            {
                play = Some(Take::B);
            }
            if ui
                .add_enabled(
                    self.session.render().is_some(),
                    egui::Button::new("Keep as B"),
                )
                .on_hover_text("Keep the current render as the comparison render B (Compare tab)")
                .clicked()
            {
                self.stop_ab();
                self.session.keep_as_b();
                self.messages = vec![
                    "B kept; the A/B playback stopped (press Play to listen to the original, A or B)"
                        .into(),
                ];
            }
            if ui
                .add_enabled(idle, egui::Button::new("A/B (space)"))
                .on_hover_text(
                    "Switches original → A → B (when kept) → original at the same moment of the input",
                )
                .clicked()
            {
                switch = true;
            }
            if ui.button("Stop").clicked() {
                stop = true;
            }
            egui::ComboBox::from_id_salt("abmode")
                .selected_text(match self.ab_mode {
                    AbMode::EngineMap => text::AB_ENGINE_MAP,
                    AbMode::SameSeconds => text::AB_SAME_SECONDS,
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.ab_mode, AbMode::EngineMap, text::AB_ENGINE_MAP);
                    ui.selectable_value(
                        &mut self.ab_mode,
                        AbMode::SameSeconds,
                        text::AB_SAME_SECONDS,
                    );
                });
        });
        if self.playback.is_some() {
            ui.label(format!(
                "Playing the {} at {}",
                match take {
                    Take::Original => "original",
                    Take::Render => "render (A)",
                    Take::B => "comparison render (B)",
                },
                secs(pos, rate)
            ));
        }
        if ui
            .add(egui::Slider::new(&mut self.monitor_volume, 0.0..=1.0).text(text::MONITOR_VOLUME))
            .changed()
        {
            self.shared.set_volume(self.monitor_volume);
        }
        if let Some(p) = &self.playback {
            if p.device_rate().is_some_and(|d| d != f64::from(rate)) {
                ui.weak(text::DEVICE_RATE_NOTE);
            }
            ui.weak(format!(
                "{}: {}",
                text::OVERLOADS,
                self.shared.overloads.load(Ordering::Relaxed)
            ));
        }
        if !cfg!(target_os = "macos") {
            ui.weak(text::PLAYBACK_UNAVAILABLE);
        }
        ui.separator();
        self.live_section(ui);
        self.capture_section(ui);
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.forms_job.is_none() && self.session.request().is_some(),
                    egui::Button::new("Check the call forms"),
                )
                .on_hover_text("Renders the session through each of the engine's six one-pass calls and after a reset, against the render (spec 006)")
                .clicked()
            {
                self.start_forms_check(ui.ctx());
            }
            if self.forms_job.is_some() && ui.button("Cancel the check").clicked() {
                self.forms_cancel.store(true, Ordering::Relaxed);
            }
        });
        for l in &self.forms_result {
            ui.label(l);
        }
        if ui.button("Export render…").clicked() {
            export = true;
        }
        if let Some(t) = play {
            self.play(t);
        }
        if switch {
            self.switch();
        }
        if stop {
            self.stop_ab();
        }
        if export {
            self.export();
        }
    }
}

/// An editor for one control's value type; returns a new value when it changed.
fn editor(
    ui: &mut egui::Ui,
    index: u32,
    ty: &ControlValue,
    value: ControlReading,
    unit: &str,
    int_edits: &mut HashMap<u32, String>,
) -> Option<ControlReading> {
    match (ty, value) {
        (ControlValue::Switch { .. }, ControlReading::Switch(mut b)) => ui
            .checkbox(&mut b, "")
            .changed()
            .then_some(ControlReading::Switch(b)),
        (ControlValue::Integer { min, max, .. }, ControlReading::Integer(v)) => {
            let buf = int_edits.entry(index).or_insert_with(|| v.to_string());
            let r = ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
            ui.weak(format!("{min} – {max} {unit}"));
            if r.lost_focus() || r.changed() {
                buf.trim()
                    .parse::<u64>()
                    .ok()
                    .filter(|&n| n != v)
                    .map(ControlReading::Integer)
            } else {
                None
            }
        }
        (ControlValue::Continuous { min, max, .. }, ControlReading::Continuous(mut f)) => {
            let span = f64::from(*max - *min).abs().max(1e-6);
            let r = ui.add(
                egui::DragValue::new(&mut f)
                    .range(*min..=*max)
                    .speed(span / 400.0)
                    .max_decimals(6)
                    .suffix(format!(" {unit}")),
            );
            r.changed().then_some(ControlReading::Continuous(f))
        }
        (ControlValue::Choice { options, .. }, ControlReading::Choice(c)) => {
            let mut chosen = c;
            let name = |v: u32| {
                options
                    .iter()
                    .find(|o| o.value == v)
                    .map_or_else(|| v.to_string(), |o| o.identifier.to_owned())
            };
            egui::ComboBox::from_id_salt(("choice", index))
                .selected_text(name(c))
                .show_ui(ui, |ui| {
                    for o in *options {
                        ui.selectable_value(&mut chosen, o.value, o.identifier);
                    }
                });
            (chosen != c).then_some(ControlReading::Choice(chosen))
        }
        _ => {
            ui.label(format!("{value:?}"));
            None
        }
    }
}

impl Workbench {
    /// Live mode, the schedule list and the timeline (spec 003 D1 – D6).
    fn live_section(&mut self, ui: &mut egui::Ui) {
        ui.strong("Live");
        ui.horizontal(|ui| {
            if self.live.is_none() {
                ui.label("start at (s)");
                ui.add(
                    egui::DragValue::new(&mut self.live_start_s)
                        .range(0.0..=36_000.0)
                        .speed(0.1),
                );
                if ui
                    .add_enabled(self.capture.is_none(), egui::Button::new("Go live"))
                    .on_disabled_hover_text("Stop the capture first (one audio stream at a time)")
                    .clicked()
                {
                    self.start_live();
                }
            } else if ui.button("Stop live").clicked() {
                self.stop_live();
            }
        });
        if let Some(run) = &self.live {
            let st = &run.queues.state;
            let rate = f64::from(run.start_req.rate_hz);
            ui.label(format!(
                "Live: fed {:.2} s, produced {:.2} s; skips {}",
                st.fed.load(Ordering::Relaxed) as f64 / rate,
                st.produced.load(Ordering::Relaxed) as f64 / rate,
                st.skips.load(Ordering::Relaxed)
            ));
            if st.skips.load(Ordering::Relaxed) > 0 {
                ui.weak(text::LIVE_SKIPPING);
            }
            // The latest 10 ms row, every field with 002's reach rule (D7, B13).
            if let Some(row) = run.take.rows.last() {
                let route = workbench_core::meters::route_of(&run.start_req.engine_values);
                egui::Grid::new("live meters").show(ui, |ui| {
                    for f in &workbench_core::meters::FIELDS {
                        ui.label(f.id);
                        if workbench_core::meters::reached(f, &run.start_req.settings, route) {
                            let v = (f.get)(&row.meters);
                            ui.add(
                                egui::ProgressBar::new((v as f32 / 2.0).clamp(0.0, 1.0))
                                    .desired_width(120.0),
                            );
                            ui.label(workbench_core::meters::display(
                                f,
                                row,
                                run.start_req.rate_hz,
                            ));
                        } else {
                            ui.weak("not reached (rest)");
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
            }
        }
        // The schedule list (D3).
        egui::CollapsingHeader::new(format!("Scheduled changes ({})", self.schedule.len())).show(
            ui,
            |ui| {
                let mut remove = None;
                for (k, (c, v, t)) in self.schedule.iter().enumerate() {
                    let e = &rr_dr60::controls::controls()[*c as usize];
                    let clock = match rr_dr60::changes::clock_of(*c) {
                        Ok(rr_dr60::changes::Clock::Input) => "input s from the start",
                        _ => "take s (output)",
                    };
                    ui.horizontal(|ui| {
                        ui.label(format!("{} = {v} at {t} {clock}", e.identifier));
                        if ui.small_button("remove").clicked() {
                            remove = Some(k);
                        }
                    });
                }
                if let Some(k) = remove {
                    self.schedule.remove(k);
                }
                ui.horizontal(|ui| {
                    let live: Vec<(u32, &str)> = rr_dr60::controls::controls()
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.timing == rr_dr60::controls::ControlTiming::Live)
                        .map(|(i, e)| (i as u32, e.identifier))
                        .collect();
                    let name = live
                        .iter()
                        .find(|(i, _)| *i == self.sched_control)
                        .map_or("", |(_, n)| n);
                    egui::ComboBox::from_id_salt("sched control")
                        .selected_text(name)
                        .show_ui(ui, |ui| {
                            for (i, n) in &live {
                                ui.selectable_value(&mut self.sched_control, *i, *n);
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sched_value)
                            .hint_text("value")
                            .desired_width(70.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sched_time)
                            .hint_text("time s")
                            .desired_width(70.0),
                    );
                    if ui.button("Add").clicked()
                        && let (Ok(v), Ok(t)) = (
                            self.sched_value.trim().parse::<f64>(),
                            self.sched_time.trim().parse::<f64>(),
                        )
                    {
                        self.schedule.push((self.sched_control, v, t));
                    }
                });
            },
        );
        // The timeline of the session's take (D6).
        if let Some(r) = self.session.render()
            && r.live.is_some()
        {
            let live = r.live.as_ref().unwrap();
            egui::CollapsingHeader::new(format!(
                "Timeline: {} changes, {} gate reports, {} skips",
                r.log.len(),
                r.events.len(),
                live.skips.len()
            ))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for e in &r.log {
                            let id = rr_dr60::controls::controls()
                                .get(e.control as usize)
                                .map_or("?", |c| c.identifier);
                            ui.monospace(format!(
                                "change {id} at {} : {} → {} (tag {})",
                                e.position, e.before, e.after, e.tag
                            ));
                        }
                        for e in &r.events {
                            ui.monospace(format!(
                                "{} at {} ({} samples)",
                                if r.drop_mode {
                                    "splice"
                                } else {
                                    "muted region"
                                },
                                e.output_position,
                                e.input_length
                            ));
                        }
                        for (at, n) in &live.skips {
                            ui.monospace(format!(
                                "skip at {at} ({n} samples of silence, not engine output)"
                            ));
                        }
                    });
                if ui.button("Export timeline CSV…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_file_name("timeline.csv")
                        .save_file()
                {
                    let csv = workbench_core::csv::timeline_csv(r);
                    self.messages = vec![match workbench_core::csv::write_with_sidecar(
                        &mut export::RealFs,
                        &path,
                        &csv,
                        r,
                    ) {
                        Ok(()) => format!("Exported {} with its sidecar", path.display()),
                        Err(e) => format!("The export failed: {e}"),
                    }];
                }
            });
            if live.incomplete {
                ui.colored_label(egui::Color32::LIGHT_RED, text::LIVE_INCOMPLETE);
            }
        }
    }
}

impl Workbench {
    /// The device (spec 004): build or reset it from the panel, record the session's input, the
    /// memory, and play an item to a take.
    fn device_view(&mut self, ui: &mut egui::Ui) {
        use workbench_core::device::{self, Device, RecordEnd};
        let Some(settings) = self.session.plain_request().map(|r| r.settings) else {
            ui.label("Open a file first: the device uses the panel's values and the file's rate.");
            return;
        };
        ui.horizontal(|ui| {
            if self.device.is_none() {
                if ui.button("Build device from the panel").clicked() {
                    match Device::new(settings) {
                        Ok(d) => self.device = Some(d),
                        Err(e) => self.messages = vec![e.to_string()],
                    }
                }
            } else {
                if ui.button("Apply the panel's settings").clicked()
                    && let Some(d) = &mut self.device
                    && let Err(e) = d.sync(settings)
                {
                    let mut m = vec![e.to_string()];
                    if matches!(e, rr_dr60::Error::StoredExceedsCapacity) {
                        m.push("One memory holds one encoding; erase first".into());
                    }
                    self.messages = m;
                }
                if ui.button("Reset device (empties the memory)").clicked() {
                    self.device = Device::new(settings).ok();
                    self.device_take = None;
                }
            }
        });
        if self.live_play.is_some() {
            self.live_play_view(ui);
            ui.separator();
        }
        let live_playing = self.live_play.is_some();
        let capturing = self.capture.is_some();
        let Some(dev) = &mut self.device else {
            return;
        };
        let st = dev.status();
        ui.label(format!(
            "Memory: {} of {} device samples used ({:.1} of {:.1} s), {} items, {} slots left",
            st.used_samples,
            st.capacity_samples,
            st.used_samples as f64 / 6000.0,
            st.capacity_samples as f64 / 6000.0,
            st.count,
            st.slots_remaining
        ));
        ui.horizontal(|ui| {
            ui.label("record from (s)");
            ui.add(
                egui::DragValue::new(&mut self.record_start_s)
                    .range(0.0..=36_000.0)
                    .speed(0.1),
            );
            if ui.button("Record the session's input").clicked()
                && let Some(inp) = self.session.input()
            {
                let start = (self.record_start_s * f64::from(inp.rate_hz)) as usize;
                let changes = device::record_request(&self.schedule, inp.rate_hz);
                let source = workbench_core::records::RunSource::of(inp, start as u64);
                self.messages = vec![match dev.record_from(
                    &inp.samples,
                    source,
                    start,
                    None,
                    &changes,
                ) {
                    Ok(o) => match o.end {
                        RecordEnd::Kept { number, length } => {
                            format!("Recorded item {number}: {length} device samples")
                        }
                        RecordEnd::SelfStopped { number, length } => {
                            format!(
                                "The memory filled: item {number} kept, {length} device samples"
                            )
                        }
                        RecordEnd::DiscardedEmpty => "Nothing was kept: no item added".into(),
                    },
                    Err(e) => e.to_string(),
                }];
            }
        });
        let items = dev.items();
        let mut erase = None;
        let mut export = None;
        let mut play = None;
        let mut play_live = None;
        let mut save_record = None;
        let mut check_forms = None;
        egui::Grid::new("items").striped(true).show(ui, |ui| {
            for h in ["item", "length", "counter", "digest", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for it in &items {
                ui.label(it.number.to_string());
                ui.label(format!(
                    "{} ({:.2} s)",
                    it.length,
                    it.length as f64 / 6000.0
                ));
                ui.label(
                    it.counter
                        .map_or_else(|| "imported".to_owned(), |c| c.to_string()),
                );
                ui.monospace(format!("{:016x}", it.digest));
                ui.horizontal(|ui| {
                    if ui.small_button("readings").clicked() {
                        self.device_item = Some(it.number);
                    }
                    if ui
                        .add_enabled(!live_playing, egui::Button::new("play to take").small())
                        .clicked()
                    {
                        play = Some(it.number);
                    }
                    if ui
                        .add_enabled(
                            !live_playing && !capturing,
                            egui::Button::new("play live").small(),
                        )
                        .clicked()
                    {
                        play_live = Some(it.number);
                    }
                    if ui.small_button("export…").clicked() {
                        export = Some(it.number);
                    }
                    if ui.small_button("save record…").clicked() {
                        save_record = Some(it.number);
                    }
                    if ui.small_button("check forms").clicked() {
                        check_forms = Some(it.number);
                    }
                    if ui.small_button("erase").clicked() {
                        erase = Some(it.number);
                    }
                });
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Erase all").clicked()
                && let Err(e) = dev.erase_all()
            {
                self.messages = vec![e.to_string()];
            }
            if ui.button("Import…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("stored form", &["rrdr60"])
                    .pick_file()
            {
                self.messages = vec![match std::fs::read(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| dev.import(&b).map_err(|e| e.to_string()))
                {
                    Ok(n) => format!("Imported as item {n}"),
                    Err(e) => e,
                }];
            }
            if ui.button("Replay a playback sidecar…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("playback sidecar", &["txt"])
                    .pick_file()
            {
                self.messages = replay_messages(&path);
            }
        });
        if let Some(n) = erase
            && let Err(e) = dev.erase(n)
        {
            self.messages = vec![e.to_string()];
        }
        if let Some(n) = export
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!("item{n}.rrdr60"))
                .save_file()
        {
            self.messages = vec![match dev
                .export(n)
                .map_err(|e| e.to_string())
                .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
            {
                Ok(()) => format!("Exported item {n} to {}", path.display()),
                Err(e) => e,
            }];
        }
        if let Some(n) = check_forms {
            // On a thread, with a copy of the device (coach review 4): five full playbacks.
            let (values, changes) = match self.session.panel() {
                Some(p) => device::play_request(p, &self.schedule, dev.settings().host_rate_hz),
                None => (Vec::new(), Vec::new()),
            };
            let copy = dev.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let m = match copy.playback_check(n, &values, &changes) {
                    Ok(c) => vec![format!(
                        "Item {n}: play and play_with_changes, values by index and by the plain setters: the four takes {}, {} 004's play to take; the getters {}",
                        if c.identical {
                            "agree"
                        } else {
                            "DISAGREE (an engine finding)"
                        },
                        if c.equals_take {
                            "equal"
                        } else {
                            "DIFFER from"
                        },
                        if c.getters_match {
                            "read the values set"
                        } else {
                            "DISAGREE with the values set (an engine finding)"
                        }
                    )],
                    Err(e) => vec![e.to_string()],
                };
                let _ = tx.send(m);
            });
            self.forms_job = Some(rx);
            self.forms_result = vec![format!("Checking item {n}'s playback forms…")];
        }
        if let Some(n) = save_record
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!("item{n}.record.txt"))
                .save_file()
        {
            self.messages = vec![save_record_files(dev, n, self.session.input(), &path)];
        }
        if let Some(n) = play {
            // The playback values from the panel's rows, the schedule list's playback entries.
            let (values, changes) = match self.session.panel() {
                Some(p) => device::play_request(p, &self.schedule, dev.settings().host_rate_hz),
                None => (Vec::new(), Vec::new()),
            };
            match dev.play_to_take(n, &values, &changes) {
                Ok(t) => {
                    self.messages = vec![format!(
                        "Played item {n}: {} samples, {} frames, {} changes landed",
                        t.samples.len(),
                        t.frames,
                        t.log.iter().filter(|e| e.tag >= 1).count()
                    )];
                    self.device_take = Some(t);
                }
                Err(e) => self.messages = vec![e.to_string()],
            }
        }
        if let Some(n) = self
            .device_item
            .filter(|n| items.iter().any(|i| i.number == *n))
            .or_else(|| items.first().map(|i| i.number))
        {
            egui::CollapsingHeader::new(format!("Readings of item {n}")).show(ui, |ui| {
                for (k, v) in dev.readings(n) {
                    ui.label(format!("{k}: {v}"));
                }
            });
        }
        let mut listen = false;
        let mut export_take = false;
        if let Some(t) = &self.device_take {
            ui.separator();
            ui.label(format!(
                "Playback take of item {}: {} samples ({} frames), place reached {}",
                t.number,
                t.samples.len(),
                t.frames,
                t.place
            ));
            egui::CollapsingHeader::new(format!("Change log ({} entries)", t.log.len())).show(
                ui,
                |ui| {
                    egui::Grid::new("take_log").striped(true).show(ui, |ui| {
                        for h in ["position", "control", "before", "after", "tag"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for e in &t.log {
                            ui.label(e.position.to_string());
                            ui.label(
                                rr_dr60::controls::controls()
                                    .get(e.control as usize)
                                    .map_or("?", |c| c.identifier),
                            );
                            ui.label(format!("{}", e.before));
                            ui.label(format!("{}", e.after));
                            ui.label(if e.tag == 0 {
                                "plain".to_owned()
                            } else {
                                e.tag.to_string()
                            });
                            ui.end_row();
                        }
                    });
                },
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.capture.is_none(),
                        egui::Button::new("Listen (take only)"),
                    )
                    .clicked()
                {
                    listen = true;
                }
                if ui.button("Export take…").clicked() {
                    export_take = true;
                }
            });
        }
        let listen_takes = if listen {
            match (&self.device_take, self.session.input()) {
                (Some(t), Some(inp)) => Some((
                    inp.samples.clone(),
                    Arc::<[f32]>::from(t.samples.clone()),
                    inp.rate_hz,
                )),
                _ => None,
            }
        } else {
            None
        };
        if let Some((original, render, rate_hz)) = listen_takes {
            self.stop_playback();
            let takes = Takes {
                original,
                render,
                b: Arc::from(Vec::new()),
                rate_hz,
            };
            match Playback::start(takes, self.shared.clone()) {
                Ok(p) => {
                    self.playback = Some(p);
                    self.shared.request(Take::Render, 0);
                    self.shared.playing.store(true, Ordering::Relaxed);
                }
                Err(e) => self.messages = vec![e],
            }
        }
        if export_take
            && let (Some(t), Some(dev)) = (&self.device_take, &self.device)
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!("item{}-playback.wav", t.number))
                .save_file()
        {
            let base = path.with_extension("");
            let stored_path = base.with_extension("rrdr60");
            let result = dev
                .export_take(t)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    use sha2::{Digest, Sha256};
                    let sha: [u8; 32] = Sha256::digest(&bytes).into();
                    std::fs::write(&stored_path, &bytes).map_err(|e| e.to_string())?;
                    let wav = export::wav_bytes(&t.samples, t.settings.host_rate_hz)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(base.with_extension("wav"), wav).map_err(|e| e.to_string())?;
                    let name = stored_path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let side = device::playback_sidecar(t, &name, &sha);
                    let mut sp = base.into_os_string();
                    sp.push(".playback.txt");
                    std::fs::write(sp, side).map_err(|e| e.to_string())
                });
            self.messages = vec![match result {
                Ok(()) => {
                    "Exported the playback take, its stored form and its playback sidecar".into()
                }
                Err(e) => e,
            }];
        }
        if let Some(n) = play_live {
            self.start_live_play(n);
        }
    }

    /// The processing record section (spec 005 D3, D4).
    fn records_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use workbench_core::records;
        ui.heading("Processing record");
        self.poll_rerender();
        ui.horizontal(|ui| {
            if ui.button("Open processing record…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("processing record", &["txt"])
                    .pick_file()
            {
                self.open_record(path);
            }
            if self.record.is_some() && ui.button("Close").clicked() {
                self.record = None;
            }
        });
        let Some(rec) = &mut self.record else {
            return;
        };
        let r = &rec.readout;
        ui.label(rec.path.display().to_string());
        let ver = |v: [u16; 3]| format!("{}.{}.{}", v[0], v[1], v[2]);
        egui::Grid::new("record_readout")
            .striped(true)
            .show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.label(k);
                    ui.monospace(v);
                    ui.end_row();
                };
                row("form version", r.form_version.to_string());
                row("library version", ver(r.library_version));
                row("host rate", format!("{} Hz", r.host_rate_hz));
                row("origin", format!("{:?}", r.origin));
                row("seed", r.seed.to_string());
                row("recording counter", r.recording_counter.to_string());
                row(
                    "stored form",
                    format!(
                        "version {}, coder model {}, {:?}, {} units",
                        r.stored_form_version,
                        ver(r.stored_codec_model_version),
                        r.encoding,
                        r.stored_unit_count
                    ),
                );
                row("stored form digest", format!("{:016x}", r.recording_digest));
                row(
                    "remaining at the start",
                    format!("{} device samples", r.remaining_samples_at_start),
                );
                row(
                    "splices or muted regions",
                    format!("{} (in the record: {})", r.events_total, r.event_count),
                );
                row(
                    "change entries",
                    format!("{} (in the record: {})", r.changes_total, r.change_count),
                );
                row(
                    "stop",
                    format!(
                        "{:?} at input {}, padding from {}, {} input samples, {} of silence",
                        r.stop.how,
                        r.stop.stop_input_position,
                        r.stop.padding_start_sample,
                        r.stop.input_samples_total,
                        r.stop.silence_fed_samples
                    ),
                );
                row(
                    "slot",
                    format!("{:?}, parameters {:?}", r.component, r.store),
                );
                row("stage mask", format!("0x{:04x}", r.stage_mask));
                let names = records::flag_names(r.flags);
                row(
                    "flags",
                    if names.is_empty() {
                        "none".into()
                    } else {
                        names.join(", ")
                    },
                );
            });
        for (i, v) in r.stage_model_versions.iter().enumerate() {
            if *v != [0, 0, 0] {
                ui.label(format!("stage {} model {}", i + 1, ver(*v)));
            }
        }
        egui::CollapsingHeader::new("Splices or muted regions in the record").show(ui, |ui| {
            egui::Grid::new("record_rows").striped(true).show(ui, |ui| {
                for h in ["#", "kept position", "input length", "host input position"] {
                    ui.strong(h);
                }
                ui.end_row();
                for i in 0..r.event_count.min(500) {
                    if let Some(e) = rr_dr60::record::record_event(&rec.bytes, i) {
                        ui.label(i.to_string());
                        ui.monospace(e.event.output_position.to_string());
                        ui.monospace(e.event.input_length.to_string());
                        ui.monospace(e.host_input_position.to_string());
                        ui.end_row();
                    }
                }
            });
            if r.event_count > 500 {
                ui.label(format!("… the first 500 of {}", r.event_count));
            }
        });
        for name in records::unknown_settings(&rec.bytes, r) {
            ui.label(format!("Unknown setting in the record: {name}"));
        }
        egui::CollapsingHeader::new("Settings against the panel").show(ui, |ui| {
            let Some(panel) = self.session.panel().map(|p| {
                p.rows()
                    .iter()
                    .map(|r| (r.index, r.value))
                    .collect::<Vec<_>>()
            }) else {
                ui.label("No panel open: no diff");
                return;
            };
            match records::settings_diff(r, &panel) {
                Ok(None) => {
                    ui.label("No settings in an imported record");
                }
                Ok(Some(d)) if d.is_empty() => {
                    ui.label("No difference");
                }
                Ok(Some(d)) => {
                    egui::Grid::new("record_diff").striped(true).show(ui, |ui| {
                        for h in ["control", "record", "panel"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for row in d {
                            ui.label(row.identifier);
                            ui.monospace(workbench_core::sidecar::fmt_reading(row.record));
                            ui.monospace(workbench_core::sidecar::fmt_reading(row.panel));
                            ui.end_row();
                        }
                    });
                }
                Err(e) => {
                    ui.label(e.to_string());
                }
            }
            ui.label(format!(
                "Not in a record: {}",
                records::not_in_record().join(", ")
            ));
        });
        ui.horizontal(|ui| {
            ui.label("item");
            ui.add(egui::DragValue::new(&mut self.check_item).range(1..=99));
            if ui
                .add_enabled(
                    self.device.is_some(),
                    egui::Button::new("Check against item"),
                )
                .on_disabled_hover_text("No device: build it in the Device tab first")
                .clicked()
                && let Some(dev) = &self.device
            {
                match dev.check_record(self.check_item, &mut rec.readout) {
                    Ok(same) => rec.check = Some((self.check_item, same)),
                    Err(e) => self.messages = vec![e.to_string()],
                }
            }
            if let Some((n, same)) = rec.check {
                ui.label(if same {
                    format!("Item {n} is this recording")
                } else {
                    format!("Item {n} is not this recording")
                });
            }
        });
        if let Some(job) = &self.rerender {
            ui.horizontal(|ui| {
                let done = job.progress.load(Ordering::Relaxed);
                ui.add(
                    egui::ProgressBar::new(done as f32 / job.total.max(1) as f32)
                        .text("re-rendering"),
                );
                if ui.button("Cancel").clicked() {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            });
            ctx.request_repaint();
        } else if ui.button("Re-render from the session's input").clicked() {
            self.start_rerender(ctx);
        }
    }

    fn open_record(&mut self, path: PathBuf) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                self.messages = vec![format!("{}: {e}", text::OPEN_IO)];
                return;
            }
        };
        let read = match &self.device {
            Some(d) => d.read_record(&bytes),
            None => {
                let mut r = rr_dr60::record::RecordReadout::default();
                rr_dr60::record::read(&bytes, &mut r).map(|()| r)
            }
        };
        match read {
            Ok(readout) => {
                self.record = Some(OpenedRecord {
                    path,
                    bytes,
                    readout,
                    check: None,
                });
            }
            Err(e) => self.messages = vec![format!("The record is refused: {e}")],
        }
    }

    fn start_rerender(&mut self, ctx: &egui::Context) {
        use workbench_core::records;
        let (Some(rec), Some(inp)) = (&self.record, self.session.input()) else {
            self.messages = vec!["Open the record's input file first".into()];
            return;
        };
        let (txt, _) = records::companion_paths(&rec.path);
        let Ok(input_text) = std::fs::read_to_string(&txt) else {
            self.messages = vec![format!(
                "No record-input file beside the record ({}): it names the input to re-render",
                txt.display()
            )];
            return;
        };
        let prep = match records::prepare(&rec.bytes, &input_text, inp) {
            Ok(p) => p,
            Err(e) => {
                self.messages = vec![e.to_string()];
                return;
            }
        };
        // The device reader's flags (it alone sets HOST_RATE_DIFFERS) with the pure reader's.
        let note = records::expected_note(rec.readout.flags | prep.readout.flags);
        self.messages = note
            .iter()
            .map(|n| format!("The record's flags: a difference is {n}"))
            .collect();
        let progress = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (p, c, ctx2) = (progress.clone(), cancel.clone(), ctx.clone());
        let samples = inp.samples.clone();
        let span = prep.span.clone();
        let total = span.len() as u64;
        let digest = prep.readout.recording_digest;
        std::thread::spawn(move || {
            let r = records::re_render(&prep.recipe, digest, &samples[span], &p, &c)
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
            ctx2.request_repaint();
        });
        self.rerender = Some(ReRenderJob {
            progress,
            cancel,
            total,
            rx,
            note,
        });
    }

    fn poll_rerender(&mut self) {
        let Some(job) = &self.rerender else {
            return;
        };
        if let Ok(r) = job.rx.try_recv() {
            let note = job.note.clone();
            self.rerender = None;
            self.messages.push(match r {
                Ok(o) => format!(
                    "{}: record {:016x}, re-render {:016x}",
                    match (o.reproduces(), note) {
                        (true, _) => "Reproduces".to_owned(),
                        (false, Some(n)) => format!("Differs ({n})"),
                        (false, None) => "Differs".to_owned(),
                    },
                    o.expected,
                    o.digest
                ),
                Err(e) => e,
            });
        }
    }

    /// The live playback's controls (spec 004b D2): each change is sent to the callback and
    /// lands at the stream's current position.
    fn live_play_view(&mut self, ui: &mut egui::Ui) {
        use rr_dr60::control_map::ControlReading;
        use rr_dr60::controls::{ControlValue, controls};
        let Some(run) = &mut self.live_play else {
            return;
        };
        let st = &run.queues.state;
        let rate = f64::from(run.queues.start.settings.host_rate_hz);
        let mut stop = false;
        ui.horizontal(|ui| {
            ui.strong(format!(
                "Playing item {} live: {:.2} s produced",
                run.queues.start.number,
                st.produced.load(Ordering::Relaxed) as f64 / rate
            ));
            if ui.button("Stop").clicked() {
                stop = true;
            }
        });
        let mut send: Vec<(u32, ControlReading)> = Vec::new();
        egui::Grid::new("live_play_controls").show(ui, |ui| {
            for (i, r) in run.readings.iter_mut() {
                let Some(e) = controls().get(*i as usize) else {
                    continue;
                };
                ui.label(e.identifier);
                let before = *r;
                match (e.value, &mut *r) {
                    (ControlValue::Switch { .. }, ControlReading::Switch(b)) => {
                        ui.checkbox(b, "");
                    }
                    (ControlValue::Integer { min, max, .. }, ControlReading::Integer(v)) => {
                        ui.horizontal(|ui| {
                            for k in min..=max {
                                ui.selectable_value(v, k, k.to_string());
                            }
                        });
                    }
                    (ControlValue::Continuous { min, max, .. }, ControlReading::Continuous(v)) => {
                        ui.add(egui::Slider::new(v, min..=max));
                    }
                    (ControlValue::Choice { options, .. }, ControlReading::Choice(v)) => {
                        ui.horizontal(|ui| {
                            for o in options {
                                ui.selectable_value(v, o.value, o.identifier);
                            }
                        });
                    }
                    _ => {
                        ui.label("");
                    }
                }
                if *r != before {
                    send.push((*i, *r));
                }
                ui.end_row();
            }
        });
        for (control, r) in send {
            let c = workbench_core::live::Command {
                control,
                value: workbench_core::live::value_of(r),
            };
            if run.queues.commands.push(c).is_err() {
                self.messages = vec!["The command queue is full: the change was not sent".into()];
            }
        }
        if stop {
            self.stop_live_play();
        }
    }
}

/// Writes item `n`'s processing record and its companions beside `path` (spec 005 D2).
fn save_record_files(
    dev: &workbench_core::device::Device,
    n: u32,
    input: Option<&workbench_core::input::Input>,
    path: &std::path::Path,
) -> String {
    use workbench_core::records;
    let name = path
        .file_name()
        .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
    let f = match records::files(dev, n, input, &name) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let (txt, i24) = records::companion_paths(path);
    let mut written = vec![path.display().to_string(), txt.display().to_string()];
    let result = std::fs::write(path, &f.record)
        .and_then(|()| std::fs::write(&txt, &f.input_text))
        .and_then(|()| match &f.i24 {
            Some(b) => {
                written.push(i24.display().to_string());
                std::fs::write(&i24, b)
            }
            None => Ok(()),
        });
    match result {
        Ok(()) => format!(
            "Saved item {n}'s processing record: {}{}",
            written.join(", "),
            if f.i24.is_none() && input.is_some() && dev.run_of(n).is_some() {
                " (the input is not on the 24-bit grid: it re-renders in the workbench only)"
            } else {
                ""
            }
        ),
        Err(e) => format!("{}: {e}", text::OPEN_IO),
    }
}

/// Replays a playback sidecar against the stored form named in it, beside the sidecar (D5),
/// and reports each check.
fn replay_messages(path: &std::path::Path) -> Vec<String> {
    use workbench_core::device;
    let result = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| device::read_playback(&t))
        .and_then(|sc| {
            let stored_path = path.with_file_name(&sc.stored_file);
            let bytes = std::fs::read(&stored_path)
                .map_err(|e| format!("{}: {e}", stored_path.display()))?;
            device::replay(&sc, &bytes)
        });
    match result {
        Ok(r) => vec![
            if r.reproduces() {
                format!(
                    "The take reproduces: {} samples, bit for bit",
                    r.take.samples.len()
                )
            } else {
                "The take does not reproduce".to_owned()
            },
            format!("stored form's SHA-256 matches: {}", r.stored_matches),
            format!("produced count matches: {}", r.produced_matches),
            format!("output's SHA-256 matches: {}", r.output_matches),
        ],
        Err(e) => vec![e],
    }
}
