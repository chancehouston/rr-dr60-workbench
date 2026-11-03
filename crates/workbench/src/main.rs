//! rr-dr60-workbench: a desktop power tool for the rr_dr60 engine.
//!
//! The bootstrap shell lists the engine's control manifest grouped by stage, read-only, to prove
//! the link to the engine. Features arrive through Spec-Kit (`docs/workbench-plan.md`).

use eframe::egui;
use workbench_core::{
    controls_for_stage, engine_version, kind_label, stages_with_controls, timing_label,
};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("rr-dr60 workbench")
            .with_inner_size([960.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "rr-dr60 workbench",
        options,
        Box::new(|_cc| Ok(Box::new(Workbench))),
    )
}

struct Workbench;

impl eframe::App for Workbench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header").show(ui, |ui| {
            ui.heading(format!(
                "rr_dr60 {} — {} controls",
                engine_version(),
                rr_dr60::controls::controls().len()
            ));
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for stage in stages_with_controls() {
                    let title = if stage == 0 {
                        "No stage".to_owned()
                    } else {
                        format!("Stage {stage}")
                    };
                    ui.collapsing(title, |ui| {
                        egui::Grid::new(("controls", stage))
                            .striped(true)
                            .show(ui, |ui| {
                                for e in controls_for_stage(stage) {
                                    ui.monospace(e.identifier);
                                    ui.label(e.name);
                                    ui.label(kind_label(e.kind));
                                    ui.label(timing_label(e.timing));
                                    ui.end_row();
                                }
                            });
                    });
                }
            });
        });
    }
}
