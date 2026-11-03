//! Spec 002's views: Scopes (waveforms, spectra, spectrogram), Meters, Taps and Stages. Every
//! figure comes from `workbench_core`; this file only draws it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use egui_plot::{Line, Plot};
use workbench_core::ab::{self, AbMode};
use workbench_core::analysis::envelope::Envelope;
use workbench_core::analysis::spectrogram::{self, Spectrogram};
use workbench_core::analysis::spectrum::{self, SpectrumPair};
use workbench_core::meters::{self, FIELDS};
use workbench_core::render::Render;
use workbench_core::session::Session;
use workbench_core::taps::{self, TapTable};
use workbench_core::{csv, export, stages, text};

/// The central tabs (D10).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    /// 001's session view.
    #[default]
    Session,
    /// Waveforms, spectra, spectrogram.
    Scopes,
    /// The meter timeline.
    Meters,
    /// The tap browser.
    Taps,
    /// The stage manifest.
    Stages,
}

struct Running<T> {
    progress: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    total: u64,
    rx: Receiver<T>,
}

/// The views' state.
#[derive(Default)]
pub struct Views {
    pub tab: Tab,
    envelopes: Option<([u8; 32], Envelope, Envelope)>,
    view: Option<(u64, u64)>,
    raw: bool,
    pair: Option<Result<SpectrumPair, String>>,
    pair_key: Option<([u8; 32], (u64, u64))>,
    per_hz: bool,
    sg_render: bool,
    sg_wide: bool,
    sg_job: Option<Running<Option<Spectrogram>>>,
    sg: Option<(Spectrogram, egui::TextureHandle)>,
    taps_job: Option<Running<TapTable>>,
    taps: Option<TapTable>,
    meter_on: Vec<bool>,
    pub messages: Vec<String>,
}

fn start<T: Send + 'static>(
    total: u64,
    f: impl FnOnce(&AtomicU64, &AtomicBool) -> T + Send + 'static,
) -> Running<T> {
    let progress = Arc::new(AtomicU64::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let (p, c) = (progress.clone(), cancel.clone());
    std::thread::spawn(move || {
        let _ = tx.send(f(&p, &c));
    });
    Running {
        progress,
        cancel,
        total,
        rx,
    }
}

fn save_csv_text(stem_hint: &str, content: &str, sidecar: &str) -> String {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{stem_hint}.csv"))
        .save_file()
    else {
        return String::new();
    };
    match csv::write_with_sidecar_text(&mut export::RealFs, &path, content, sidecar) {
        Ok(()) => format!("Exported {} with its sidecar", path.display()),
        Err(e) => format!("The export failed: {e}"),
    }
}

fn save_csv(stem_hint: &str, content: &str, r: &Render) -> String {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name(format!("{stem_hint}.csv"))
        .save_file()
    else {
        return String::new();
    };
    match csv::write_with_sidecar(&mut export::RealFs, &path, content, r) {
        Ok(()) => format!("Exported {} with its sidecar", path.display()),
        Err(e) => format!("The export failed: {e}"),
    }
}

impl Views {
    /// Polls the background jobs.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(j) = &self.sg_job
            && let Ok(res) = j.rx.try_recv()
        {
            self.sg_job = None;
            if let Some(s) = res {
                let img = colour_image(&s);
                let tex = ctx.load_texture("spectrogram", img, egui::TextureOptions::NEAREST);
                self.sg = Some((s, tex));
            }
        }
        if let Some(j) = &self.taps_job
            && let Ok(t) = j.rx.try_recv()
        {
            self.taps_job = None;
            self.taps = Some(t);
        }
    }

    /// True while a background job runs.
    pub fn busy(&self) -> bool {
        self.sg_job.is_some() || self.taps_job.is_some()
    }

    pub fn scopes(&mut self, ui: &mut egui::Ui, session: &Session, playhead: Option<u64>) {
        let (Some(inp), Some(r)) = (session.input(), session.render()) else {
            ui.label("Render first.");
            return;
        };
        if inp.samples.is_empty() || r.samples.is_empty() {
            ui.label(text::NO_SAMPLES);
            return;
        }
        let key = r.output_sha256;
        if self.envelopes.as_ref().is_none_or(|(k, _, _)| *k != key) {
            self.envelopes = Some((key, Envelope::new(&inp.samples), Envelope::new(&r.samples)));
            self.view = Some((0, inp.samples.len() as u64));
            self.pair = None;
            self.sg = None;
        }
        let len = inp.samples.len() as u64;
        let (mut a, mut b) = self.view.unwrap_or((0, len));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.raw, "Raw (the render on its own clock)");
            if ui.button("Zoom in").clicked() {
                let m = (a + b) / 2;
                let h = ((b - a) / 4).max(16);
                (a, b) = (m.saturating_sub(h), (m + h).min(len));
            }
            if ui.button("Zoom out").clicked() {
                let m = (a + b) / 2;
                let h = (b - a).max(16);
                (a, b) = (m.saturating_sub(h), (m + h).min(len));
            }
            if ui.button("◀").clicked() {
                let w = b - a;
                a = a.saturating_sub(w / 2);
                b = a + w;
            }
            if ui.button("▶").clicked() {
                let w = b - a;
                b = (b + w / 2).min(len);
                a = b.saturating_sub(w);
            }
            if ui.button("Whole take").clicked() {
                (a, b) = (0, len);
            }
            ui.label(format!(
                "{:.3} – {:.3} s",
                a as f64 / f64::from(inp.rate_hz),
                b as f64 / f64::from(inp.rate_hz)
            ));
        });
        self.view = Some((a, b));
        let (_, eo, er) = self.envelopes.as_ref().unwrap();
        let w = ui.available_width().max(100.0);
        let cols = w as usize;
        strip(
            ui,
            "Original",
            &eo.columns(&inp.samples, (a, b), cols),
            playhead.map(|p| (p, a, b)),
            &[],
        );
        let (ra, rb) = if self.raw {
            (a, b)
        } else {
            (
                ab::to_render(a, r, AbMode::EngineMap),
                ab::to_render(b, r, AbMode::EngineMap),
            )
        };
        let marks: Vec<f32> = ab::marks(r)
            .iter()
            .filter(|m| m.output_position >= ra && m.output_position < rb.max(ra + 1))
            .map(|m| (m.output_position - ra) as f32 / (rb - ra).max(1) as f32)
            .collect();
        strip(
            ui,
            if self.raw {
                "Render (raw)"
            } else {
                "Render (engine's map)"
            },
            &er.columns(&r.samples, (ra, rb), cols),
            None,
            &marks,
        );
        ui.weak(format!(
            "Aligned within about one device sample (a few with the speech coder on); marks: removed pauses or muted regions. On the engine's map the first {} render samples (before input 0) and the {}-sample tail are not drawn; Raw shows them.",
            r.latency_samples, r.tail_samples
        ));
        ui.separator();
        // Spectra (D6) over the view's range.
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.per_hz, "dB/Hz");
            if ui.button("Export spectra CSV…").clicked()
                && let Some(Ok(p)) = &self.pair
            {
                self.messages = vec![save_csv("spectrum", &csv::spectrum_csv(p, r), r)];
            }
        });
        if self.pair_key != Some((key, (a, b))) {
            self.pair = Some(spectrum::pair(&inp.samples, r, (a, b)).map_err(|e| e.to_string()));
            self.pair_key = Some((key, (a, b)));
        }
        match &self.pair {
            Some(Ok(p)) => {
                let off = if self.per_hz {
                    10.0 * (spectrum::ENBW_BINS * spectrum::BIN_HZ).log10()
                } else {
                    0.0
                };
                let pts = |s: &spectrum::Spectrum| -> Vec<[f64; 2]> {
                    (1..s.power.len())
                        .filter(|&k| s.power[k] > 0.0)
                        .map(|k| [s.freq(k).log10(), s.db(k) - off])
                        .collect()
                };
                let ratio: Vec<[f64; 2]> = (1..p.original.power.len())
                    .filter_map(|k| p.ratio_db(k).map(|d| [p.original.freq(k).log10(), d]))
                    .collect();
                ui.weak(format!(
                    "Welch, N {} (5 Hz bins), Hann, 50 % overlap; {} re full-scale sine; frames: original {}, render {} ({} dropped: splices, muted regions; a guard of {} samples at their edges). Tone levels are not read here (scalloping up to 1.42 dB).",
                    p.original.n,
                    if self.per_hz { "dB/Hz" } else { "dB per 5 Hz bin (ENBW 7.5 Hz)" },
                    p.original.frames,
                    p.render.frames,
                    p.dropped_frames,
                    r.playback_latency_samples
                ));
                Plot::new("spectra")
                    .height(220.0)
                    .legend(egui_plot::Legend::default())
                    .x_axis_label("log10 Hz")
                    .show(ui, |pu| {
                        pu.line(Line::new("original", pts(&p.original)));
                        pu.line(Line::new("render", pts(&p.render)));
                    });
                Plot::new("ratio")
                    .height(140.0)
                    .x_axis_label("log10 Hz")
                    .y_axis_label("dB")
                    .show(ui, |pu| {
                        pu.line(Line::new(
                            "spectral ratio, render ÷ original (not a transfer function)",
                            ratio,
                        ));
                    });
            }
            Some(Err(e)) => {
                ui.label(e);
            }
            None => {}
        }
        ui.separator();
        // Spectrogram (D7).
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.sg_render, "Render (else original)");
            ui.checkbox(&mut self.sg_wide, "Wideband (10 ms)");
            if let Some(j) = &self.sg_job {
                ui.add(egui::ProgressBar::new(
                    j.progress.load(Ordering::Relaxed) as f32 / j.total.max(1) as f32,
                ));
                if ui.button("Cancel").clicked() {
                    j.cancel.store(true, Ordering::Relaxed);
                }
            } else if ui.button("Spectrogram").clicked() {
                let (x, rate, wide) = if self.sg_render {
                    (r.samples.clone(), r.request.rate_hz, self.sg_wide)
                } else {
                    (inp.samples.clone(), inp.rate_hz, self.sg_wide)
                };
                let range = if self.sg_render { (ra, rb) } else { (a, b) };
                let frames = (range.1 - range.0) / u64::from(rate / 100).max(1);
                self.sg_job = Some(start(frames, move |p, c| {
                    spectrogram::compute(&x, rate, wide, range, 4096, p, c)
                }));
            }
        });
        if let Some((s, tex)) = &self.sg {
            let rate = if self.sg_render {
                r.request.rate_hz
            } else {
                inp.rate_hz
            };
            let bin = f64::from(rate) / s.frame_n as f64;
            ui.weak(format!(
                "Hann {} samples ({bin:.0} Hz bins, ENBW {:.1} Hz), hop {}, each column the maximum over its frames; range {} – {}; dB re full-scale sine per bin, −120 … +7 dB, brighter is louder, red above +7.25 dB (saturated)",
                s.frame_n,
                1.5 * bin,
                s.hop,
                s.range.0,
                s.range.1
            ));
            let current = if self.sg_render { (ra, rb) } else { (a, b) };
            if s.range != current {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "The spectrogram is of an earlier view range: compute it again.",
                );
            }
            let size = egui::vec2(ui.available_width(), 240.0);
            ui.image((tex.id(), size));
        }
    }

    pub fn meters(&mut self, ui: &mut egui::Ui, session: &Session) {
        let Some(r) = session.render() else {
            ui.label("Render first.");
            return;
        };
        if self.meter_on.len() != FIELDS.len() {
            self.meter_on = FIELDS
                .iter()
                .map(|f| {
                    matches!(
                        f.id,
                        "input_peak" | "gain_reduction_db" | "vas_paused" | "route_peak"
                    )
                })
                .collect();
        }
        let route = meters::route_of(&r.request.engine_values);
        ui.horizontal(|ui| {
            if ui.button("Export meters CSV…").clicked() {
                self.messages = vec![save_csv("meters", &csv::meters_csv(r), r)];
            }
            ui.weak(format!(
                "{} rows; 10 ms calls ({} host samples); maxima and counts as steps over their call, states at its end; output-side fields mapped through the engine's map; device-point fields up to the record path's latency late. A 10 ms maximum does not show the limiter's dead time.",
                r.meters.len(),
                r.request.block
            ));
        });
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, f) in FIELDS.iter().enumerate() {
                let reached = meters::reached(f, &r.request.settings, route);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.meter_on[i], f.id);
                    ui.weak(format!("{} · {:?} · {:?}", f.unit, f.kind, f.side))
                        .on_hover_text(format!(
                            "Workbench note, from the engine's rustdoc at a06318c: {}",
                            f.note
                        ));
                    if !reached {
                        ui.colored_label(egui::Color32::GRAY, "not reached (rest)");
                    }
                });
                if !self.meter_on[i] || !reached {
                    continue;
                }
                // The core places the figures (rest point first, steps and states, breaks at
                // pauses: `meters::plot_points`).
                let pts = meters::plot_points(f, r);
                if let Some(last) = r.meters.last() {
                    ui.weak(format!(
                        "last call: {}",
                        meters::display(f, last, r.request.rate_hz)
                    ));
                }
                Plot::new(("meter", i))
                    .height(90.0)
                    .x_axis_label("s (original's clock)")
                    .show(ui, |pu| {
                        pu.line(Line::new(f.id, pts));
                    });
            }
        });
    }

    pub fn taps(&mut self, ui: &mut egui::Ui, session: &Session) {
        ui.horizontal(|ui| {
            if let Some(j) = &self.taps_job {
                ui.add(egui::ProgressBar::new(
                    j.progress.load(Ordering::Relaxed) as f32 / j.total.max(1) as f32,
                ));
                if ui.button("Cancel").clicked() {
                    j.cancel.store(true, Ordering::Relaxed);
                }
            } else if ui.button("Render at every tap").clicked()
                && let Some(req) = session.request()
            {
                self.taps_job = Some(start(taps::tap_count() as u64, move |p, c| {
                    taps::browse(&req, p, c)
                }));
            }
            if let Some(t) = &self.taps
                && let Some(side) = &t.sidecar
                && ui.button("Export taps CSV…").clicked()
            {
                self.messages = vec![save_csv_text("taps", &csv::taps_csv(t), side)];
            }
        });
        let Some(t) = &self.taps else {
            ui.label("A fresh engine per tap, with the session's values; each tap's readings, digest and spectrum (to listen, set the tap in the panel and render).");
            return;
        };
        if session.request().is_some_and(|q| t.is_stale(&q)) {
            ui.colored_label(egui::Color32::YELLOW, text::STALE);
        }
        egui::Grid::new("taps").striped(true).show(ui, |ui| {
            for h in [
                "tap",
                "length",
                "latency",
                "tail",
                "splices/regions",
                "not emitted",
                "sample peak",
                "SHA-256",
            ] {
                ui.strong(h);
            }
            ui.end_row();
            for row in &t.rows {
                ui.monospace(row.tap);
                match &row.outcome {
                    Ok(r) => {
                        ui.label(r.length.to_string());
                        ui.label(r.latency.to_string());
                        ui.label(r.tail.to_string());
                        ui.label(if r.gate_reached {
                            r.splices_or_regions.to_string()
                        } else {
                            text::GATE_NOT_REACHED.to_owned()
                        });
                        ui.label(r.not_emitted.to_string());
                        ui.label(format!("{:.4}", r.peak));
                        ui.monospace(&workbench_core::input::hex(&r.sha256)[..16]);
                    }
                    Err(e) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                    }
                }
                ui.end_row();
            }
        });
        let curves: Vec<(&str, Vec<[f64; 2]>)> = t
            .rows
            .iter()
            .filter_map(|row| {
                let s = row.outcome.as_ref().ok()?.spectrum.as_ref()?;
                Some((
                    row.tap,
                    (1..s.power.len())
                        .filter(|&k| s.power[k] > 0.0)
                        .map(|k| [s.freq(k).log10(), s.db(k)])
                        .collect(),
                ))
            })
            .collect();
        ui.weak("Render side at each tap, over the render's own runs: Welch N rate ÷ 5, dB re full-scale sine per 5 Hz bin (ENBW 7.5 Hz).");
        Plot::new("tap spectra")
            .height(260.0)
            .legend(egui_plot::Legend::default())
            .x_axis_label("log10 Hz")
            .show(ui, |pu| {
                for (name, pts) in curves {
                    pu.line(Line::new(name, pts));
                }
            });
    }

    pub fn stages(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("manifest header").show(ui, |ui| {
                for (k, v) in stages::header_lines() {
                    ui.strong(k);
                    ui.label(v);
                    ui.end_row();
                }
            });
            for e in stages::stages() {
                egui::CollapsingHeader::new(format!("Stage {} · {}", e.number, e.name)).show(
                    ui,
                    |ui| {
                        egui::Grid::new(("stage", e.number))
                            .num_columns(2)
                            .show(ui, |ui| {
                                for (k, v) in stages::stage_fields(e) {
                                    ui.strong(k);
                                    ui.label(v);
                                    ui.end_row();
                                }
                            });
                        let cs = stages::controls_of(e.number);
                        if !cs.is_empty() {
                            ui.label(format!(
                                "Controls: {}",
                                cs.iter()
                                    .map(|c| c.identifier)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                        }
                    },
                );
            }
            let none = stages::controls_of(0);
            ui.label(format!(
                "No stage: {}",
                none.iter()
                    .map(|c| c.identifier)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        });
    }
}

/// One waveform strip: an envelope per column, an optional playhead, and marks (fractions).
fn strip(
    ui: &mut egui::Ui,
    title: &str,
    cols: &[(f32, f32)],
    playhead: Option<(u64, u64, u64)>,
    marks: &[f32],
) {
    ui.label(title);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 110.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
    let mid = rect.center().y;
    let h = rect.height() / 2.0;
    let colour = ui.visuals().text_color();
    let mut beyond = 0usize;
    for (i, &(lo, hi)) in cols.iter().enumerate() {
        let x = rect.left() + i as f32;
        let clipped = lo < -2.0 || hi > 2.0;
        beyond += usize::from(clipped);
        let (lo, hi) = (lo.clamp(-2.0, 2.0) / 2.0, hi.clamp(-2.0, 2.0) / 2.0);
        let c = if clipped { egui::Color32::RED } else { colour };
        painter.line_segment(
            [egui::pos2(x, mid - hi * h), egui::pos2(x, mid - lo * h)],
            egui::Stroke::new(1.0, c),
        );
    }
    if beyond > 0 {
        painter.text(
            rect.right_top(),
            egui::Align2::RIGHT_TOP,
            format!("{beyond} columns beyond ±2 (red)"),
            egui::FontId::proportional(10.0),
            egui::Color32::RED,
        );
    }
    for &m in marks {
        let x = rect.left() + m * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, egui::Color32::YELLOW),
        );
    }
    if let Some((p, a, b)) = playhead
        && p >= a
        && p < b
    {
        let x = rect.left() + (p - a) as f32 / (b - a) as f32 * rect.width();
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.5, egui::Color32::LIGHT_BLUE),
        );
    }
    painter.text(
        rect.left_top(),
        egui::Align2::LEFT_TOP,
        "±2.0 full scale",
        egui::FontId::proportional(10.0),
        colour,
    );
}

/// The spectrogram's u8 indices as an image: time across, frequency up.
fn colour_image(s: &Spectrogram) -> egui::ColorImage {
    let (w, h) = (s.cols.max(1) as usize, s.bins.max(1) as usize);
    let mut px = vec![egui::Color32::BLACK; w * h];
    for c in 0..s.cols as usize {
        for k in 0..h {
            let v = s.data[c * h + k];
            px[(h - 1 - k) * w + c] = if v == spectrogram::SATURATED {
                egui::Color32::RED
            } else {
                egui::Color32::from_rgb(v / 2, v, (255 - v) / 3)
            };
        }
    }
    egui::ColorImage::new([w, h], px)
}
