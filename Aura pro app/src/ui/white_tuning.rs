use super::{primary_button, secondary_button, AmbilightApp, Screen, ACCENT_COLOR, FG_COLOR};
use crate::config::TuningConfig;
use eframe::egui::{self, Color32, Key, Rect, Rounding, Stroke, Vec2};
use std::sync::atomic::Ordering;

pub fn draw_fullscreen_white_tuning(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        app.stop_white_stream();
        app.flush_config_now();
        app.screen = if app.in_initial_setup_flow {
            Screen::FullscreenCalibration
        } else {
            Screen::Dashboard
        };
        return;
    }

    let avail = ui.available_rect_before_wrap();
    let (canvas, _resp) = ui.allocate_exact_size(avail.size(), egui::Sense::hover());
    let painter = ui.painter_at(canvas);

    painter.rect_filled(canvas, Rounding::ZERO, Color32::WHITE);

    let box_rect = Rect::from_center_size(canvas.center(), Vec2::new(560.0, 275.0));
    painter.rect(
        box_rect,
        Rounding::same(12.0),
        Color32::from_rgba_unmultiplied(235, 244, 255, 248),
        Stroke::new(2.0_f32, ACCENT_COLOR),
    );

    let mut t_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(box_rect.shrink(16.0))
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );

    t_ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("Tuning & Auto-Start").color(FG_COLOR));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let conn = app.esp_connected.load(Ordering::Relaxed);
            let (lbl, col) = if conn {
                ("Connected", Color32::from_rgb(22, 163, 74))
            } else {
                ("Disconnected", Color32::from_rgb(220, 38, 38))
            };
            ui.colored_label(col, egui::RichText::new(lbl).strong());
        });
    });
    t_ui.add_space(6.0);

    let mut changed = false;
    let mut flush_now = false;

    let r_br = t_ui.add(
        egui::Slider::new(&mut app.layout.tuning.brightness, 0.05..=1.0)
            .text(egui::RichText::new("Brightness").color(FG_COLOR)),
    );
    changed |= r_br.changed();
    flush_now |= r_br.drag_stopped();

    let r_gm = t_ui.add(
        egui::Slider::new(&mut app.layout.tuning.gamma, 0.5..=3.0)
            .text(egui::RichText::new("Gamma").color(FG_COLOR)),
    );
    changed |= r_gm.changed();
    flush_now |= r_gm.drag_stopped();

    let r_wr = t_ui.add(
        egui::Slider::new(&mut app.layout.tuning.wb_r, 0.2..=1.2)
            .text(egui::RichText::new("Red").color(FG_COLOR)),
    );
    changed |= r_wr.changed();
    flush_now |= r_wr.drag_stopped();

    let r_wg = t_ui.add(
        egui::Slider::new(&mut app.layout.tuning.wb_g, 0.2..=1.2)
            .text(egui::RichText::new("Green").color(FG_COLOR)),
    );
    changed |= r_wg.changed();
    flush_now |= r_wg.drag_stopped();

    let r_wb = t_ui.add(
        egui::Slider::new(&mut app.layout.tuning.wb_b, 0.2..=1.2)
            .text(egui::RichText::new("Blue").color(FG_COLOR)),
    );
    changed |= r_wb.changed();
    flush_now |= r_wb.drag_stopped();

    t_ui.add_space(6.0);
    t_ui.horizontal(|ui| {
        if ui
            .checkbox(
                &mut app.layout.tuning.auto_start,
                egui::RichText::new("Auto-Start").color(FG_COLOR),
            )
            .changed()
        {
            changed = true;
            flush_now = true;
        }
        if ui.add(secondary_button("Reset")).clicked() {
            let auto_s = app.layout.tuning.auto_start;
            app.layout.tuning = TuningConfig {
                auto_start: auto_s,
                ..TuningConfig::default()
            };
            changed = true;
            flush_now = true;
        }
    });

    if changed {
        app.mark_config_dirty();
    }
    if flush_now {
        app.flush_config_now();
    }

    t_ui.add_space(8.0);
    t_ui.horizontal(|ui| {
        if app.in_initial_setup_flow {
            if ui.add(secondary_button("Back")).clicked() {
                app.stop_white_stream();
                app.flush_config_now();
                app.start_fullscreen_calibration();
            }
            if ui.add(primary_button("Finish")).clicked() {
                app.stop_white_stream();
                app.layout.setup_completed = true;
                app.in_initial_setup_flow = false;
                app.flush_config_now();
                app.screen = Screen::Dashboard;
            }
        } else if ui.add(primary_button("Done")).clicked() {
            app.stop_white_stream();
            app.flush_config_now();
            app.screen = Screen::Dashboard;
        }
    });
}