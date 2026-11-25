use super::led_setup::draw_monitor_layout_card;
use super::{primary_button, secondary_button, AmbilightApp, Screen, ACCENT_COLOR};
use eframe::egui::{self, Color32, Stroke};

pub fn draw_dashboard(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Aura Pro");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            app.draw_esp_status_bar(ui);
        });
    });
    ui.separator();
    ui.add_space(4.0);

    ui.vertical_centered(|ui| {
        draw_monitor_layout_card(app, ui);
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            let gap = 8.0;
            let btn_w = ((total_w - gap * 3.0) / 4.0).max(120.0);
            let btn_h = 34.0;

            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            if ui
                .add_sized([btn_w, btn_h], primary_button("LED Setup"))
                .clicked()
            {
                app.stop_all_streams();
                app.refresh_displays();
                app.in_initial_setup_flow = false;
                app.screen = Screen::LedLayoutSetup;
            }

            if ui
                .add_sized(
                    [btn_w, btn_h],
                    egui::Button::new(
                        egui::RichText::new("Tuning & Auto-Start")
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(ACCENT_COLOR),
                )
                .clicked()
            {
                app.in_initial_setup_flow = false;
                app.start_fullscreen_white_tuning();
            }

            if ui
                .add_sized([btn_w, btn_h], secondary_button("Settings"))
                .clicked()
            {
                app.stop_all_streams();
                app.screen = Screen::Settings;
            }

            if ui
                .add_sized(
                    [btn_w, btn_h],
                    egui::Button::new(
                        egui::RichText::new("Reset")
                            .color(Color32::from_rgb(185, 28, 28))
                            .strong(),
                    )
                    .fill(Color32::from_rgb(254, 226, 226))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(248, 113, 113))),
                )
                .clicked()
            {
                app.trigger_reset_to_initial_setup();
            }
        });
    });
}