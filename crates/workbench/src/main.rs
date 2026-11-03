//! rr-dr60-workbench: a desktop power tool for the rr_dr60 engine.
//!
//! Spec 001, the offline render session: open a file, edit every manifest control, render through
//! a fresh one-pass engine with a tail, play the original and the render A/B on the engine's map,
//! export the render with its sidecar and reopen a sidecar. The logic is in `workbench_core`;
//! this file is the thin egui view (constitution V).

mod playback;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui;
use rr_dr60::control_map::ControlReading;
use rr_dr60::controls::ControlValue;
use workbench_core::ab::{self, AbMode};
use workbench_core::input::{self, Channel, Input, SampleType};
use workbench_core::monitor::{Shared, Take, Takes};
use workbench_core::panel::{Panel, RowStatus};
use workbench_core::render::{self, Render, RenderRequest};
use workbench_core::{export, sidecar, text};

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
        Box::new(|_cc| Ok(Box::<Workbench>::default())),
    )
}

/// A render running on its own thread.
struct Job {
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    total: u64,
    rx: Receiver<Result<Option<Render>, String>>,
}

#[derive(Default)]
struct Workbench {
    path: Option<PathBuf>,
    input: Option<Input>,
    panel: Option<Panel>,
    tail: bool,
    render: Option<Render>,
    job: Option<Job>,
    shared: Arc<Shared>,
    playback: Option<Playback>,
    ab_mode: AbMode,
    monitor_volume: f32,
    int_edits: HashMap<u32, String>,
    messages: Vec<String>,
    initialized: bool,
}

impl Workbench {
    fn init(&mut self) {
        if !self.initialized {
            self.tail = true;
            self.monitor_volume = 1.0;
            self.shared.set_volume(1.0);
            self.initialized = true;
        }
    }

    fn stop_playback(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        // Dropping stops the unit before the takes are released (plan P3).
        self.playback = None;
    }

    fn open_file(&mut self, path: PathBuf, channel: Channel) {
        self.stop_playback();
        match input::open(&path, channel) {
            Ok(inp) => {
                let keep_panel = self
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.rate_hz() == inp.rate_hz)
                    && self.path.as_ref() == Some(&path);
                if !keep_panel {
                    self.panel = Panel::new(inp.rate_hz).ok();
                    self.render = None;
                    self.int_edits.clear();
                }
                self.path = Some(path);
                self.input = Some(inp);
                self.messages.clear();
            }
            Err(e) => self.messages = vec![e.to_string()],
        }
    }

    fn request(&self) -> Option<RenderRequest> {
        let (inp, panel) = (self.input.as_ref()?, self.panel.as_ref()?);
        let (settings, engine_values) = panel.snapshot();
        Some(RenderRequest {
            input: inp.samples.clone(),
            input_sha256: inp.input_sha256,
            channel: inp.channel,
            settings,
            engine_values,
            tail: self.tail,
            block: render::BLOCK,
        })
    }

    fn start_render(&mut self, ctx: &egui::Context) {
        let Some(req) = self.request() else { return };
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
        let Some(job) = &self.job else { return };
        if let Ok(result) = job.rx.try_recv() {
            self.job = None;
            match result {
                Ok(Some(r)) => {
                    self.stop_playback();
                    self.render = Some(r);
                }
                Ok(None) => {}
                Err(e) => self.messages = vec![e],
            }
        }
    }

    fn play(&mut self, take: Take) {
        let (Some(inp), Some(r)) = (&self.input, &self.render) else {
            if let (Some(inp), Take::Original) = (&self.input, take) {
                let empty: Arc<[f32]> = Arc::from(Vec::new());
                self.start_takes(inp.samples.clone(), empty, inp.rate_hz, take);
            }
            return;
        };
        self.start_takes(inp.samples.clone(), r.samples.clone(), inp.rate_hz, take);
    }

    fn start_takes(&mut self, original: Arc<[f32]>, render: Arc<[f32]>, rate: u32, take: Take) {
        if self.playback.is_none() {
            let takes = Takes {
                original,
                render,
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
        } else {
            let (_, pos) = self.shared.cursor();
            self.shared.request(take, pos);
        }
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    /// The A/B switch at the mapped position (D6).
    fn switch(&mut self) {
        let (Some(inp), Some(r)) = (&self.input, &self.render) else {
            return;
        };
        if self.playback.is_none() {
            return;
        }
        let (take, pos) = self.shared.cursor();
        match take {
            Take::Original => self
                .shared
                .request(Take::Render, ab::to_render(pos, r, self.ab_mode)),
            Take::Render => self.shared.request(
                Take::Original,
                ab::to_original(pos, r, self.ab_mode, inp.samples.len() as u64),
            ),
        }
    }

    fn export(&mut self) {
        let (Some(inp), Some(r)) = (&self.input, &self.render) else {
            return;
        };
        let name = format!("{}-render.wav", inp.file_name.trim_end_matches(".wav"));
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
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
        self.messages = vec![match export::export(&mut export::RealFs, &path, inp, r) {
            Ok(()) => format!("Exported {} and {}", wav.display(), side.display()),
            Err(e) => e.to_string(),
        }];
    }

    fn open_sidecar(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("sidecar", &["txt"])
            .pick_file()
        else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                self.messages = vec![format!("{}: {e}", text::OPEN_IO)];
                return;
            }
        };
        let s = match sidecar::read(&text) {
            Ok(s) => s,
            Err(e) => {
                self.messages = vec![e.to_string()];
                return;
            }
        };
        let (Some(inp), Some(panel)) = (self.input.as_ref(), self.panel.as_mut()) else {
            self.messages = vec!["Open the input file before its sidecar".into()];
            return;
        };
        let rep = sidecar::restore(&s, panel, inp);
        self.int_edits.clear();
        self.tail = rep.tail;
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
        let channel = rep.channel;
        self.messages = m;
        if let (Some(c), Some(p)) = (channel, self.path.clone())
            && self.input.as_ref().is_some_and(|i| i.channel != c)
        {
            self.open_file(p, c);
        }
    }
}

fn secs(n: u64, rate: u32) -> String {
    format!("{n} samples ({:.3} s)", n as f64 / f64::from(rate.max(1)))
}

impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.init();
        let ctx = ui.ctx().clone();
        self.poll_job();
        if self.playback.as_ref().is_some_and(Playback::device_lost) {
            self.stop_playback();
            self.messages = vec![text::DEVICE_LOST.to_owned()];
        }
        if self.job.is_some() || self.playback.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.switch();
        }

        egui::Panel::top("file").show(ui, |ui| self.file_bar(ui));
        egui::Panel::left("controls")
            .resizable(true)
            .default_size(560.0)
            .show(ui, |ui| self.controls_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.session(ui, &ctx));
    }
}

impl Workbench {
    fn file_bar(&mut self, ui: &mut egui::Ui) {
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
                self.open_file(p, Channel::Index(0));
            }
            if let Some(inp) = &self.input {
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
                    && let Some(p) = self.path.clone()
                {
                    self.open_file(p, chosen);
                }
            }
            if ui.button("Open sidecar…").clicked() {
                self.open_sidecar();
            }
        });
    }

    fn controls_panel(&mut self, ui: &mut egui::Ui) {
        let Some(panel) = self.panel.as_mut() else {
            ui.label("Open a file to edit the engine's controls.");
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(format!("{} controls", panel.rows().len()));
            if ui.button("Reset to defaults").clicked() {
                panel.reset_to_defaults();
                self.int_edits.clear();
            }
        });
        let mut stages: Vec<u8> = panel.rows().iter().map(|r| r.entry.stage).collect();
        stages.sort_unstable();
        stages.dedup();
        let mut edits: Vec<(u32, ControlReading)> = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for stage in stages {
                let title = if stage == 0 { "No stage".to_owned() } else { format!("Stage {stage}") };
                egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
                    egui::Grid::new(("grid", stage)).num_columns(3).striped(true).show(ui, |ui| {
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
                            let enabled = matches!(row.status, RowStatus::Accepted | RowStatus::Refused(_));
                            ui.add_enabled_ui(enabled, |ui| {
                                if let Some(v) = editor(ui, row.index, &e.value, row.value, e.unit, &mut self.int_edits) {
                                    edits.push((row.index, v));
                                }
                            });
                            match &row.status {
                                RowStatus::Accepted => ui.label(""),
                                RowStatus::Refused(why) => ui.colored_label(egui::Color32::LIGHT_RED, why),
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
        }
    }

    fn session(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        for m in &self.messages {
            ui.label(m);
        }
        let Some(inp) = &self.input else {
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
        }
        ui.separator();
        // Render.
        let renderable = self.panel.as_ref().is_some_and(Panel::renderable);
        let mut start = false;
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.tail, "Tail")
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
        if start {
            self.start_render(ctx);
        }
        let current = self.request();
        let Some(r) = &self.render else {
            ui.label("No render yet.");
            return;
        };
        if current.as_ref().is_some_and(|c| r.is_stale(c)) {
            ui.colored_label(egui::Color32::YELLOW, text::STALE);
        }
        // Readings (FR-009).
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
        }
        ui.separator();
        // Transport (D6).
        let mut play: Option<Take> = None;
        let mut switch = false;
        let mut stop = false;
        let (take, pos) = self.shared.cursor();
        ui.horizontal(|ui| {
            if ui.button("Play original").clicked() {
                play = Some(Take::Original);
            }
            if ui.button("Play render").clicked() {
                play = Some(Take::Render);
            }
            if ui.button("A/B (space)").clicked() {
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
                    Take::Render => "render",
                },
                secs(pos, rate)
            ));
        }
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::Slider::new(&mut self.monitor_volume, 0.0..=1.0)
                        .text(text::MONITOR_VOLUME),
                )
                .changed()
            {
                self.shared.set_volume(self.monitor_volume);
            }
        });
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
        if ui.button("Export render…").clicked() {
            self.export();
        }
        if let Some(t) = play {
            self.play(t);
        }
        if switch {
            self.switch();
        }
        if stop {
            self.stop_playback();
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
