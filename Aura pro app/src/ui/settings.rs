use super::{
    primary_button, secondary_button, AmbilightApp, Screen, BORDER_COLOR, SURFACE_COLOR,
};
use eframe::egui::{self, Rounding, Stroke};

const CREATOR_NAME: &str = "Ganith Pasindu";
const CREATOR_BIO: &str = "DIY hardware & desktop software developer";
const WEBSITE_URL: &str = "https://ganithpasindu.github.io/";

pub fn draw_settings(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Settings");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            app.draw_esp_status_bar(ui);
        });
    });
    ui.separator();
    ui.add_space(6.0);

    ui.vertical_centered(|ui| {
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("App Update");
                ui.add_space(6.0);
                if ui
                    .add_enabled(!app.app_updating, primary_button("Update App"))
                    .clicked()
                {
                    app.start_app_update();
                }
                if app.app_updating || app.app_update_progress > 0.0 {
                    ui.add_space(8.0);
                    ui.add(egui::ProgressBar::new(app.app_update_progress));
                }
            });

            cols[1].group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.strong("ESP32 Update");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!app.esp_updating, primary_button("Update ESP32"))
                        .clicked()
                    {
                        app.start_esp32_update();
                    }
                    if app.esp_updating || app.esp_elapsed_secs > 0 {
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(format!("{}s", app.esp_elapsed_secs));
                            },
                        );
                    }
                });

                let combined_progress = app.esp_flash_progress.max(app.esp_dl_progress);
                if app.esp_updating || combined_progress > 0.0 {
                    ui.add_space(8.0);
                    ui.add(egui::ProgressBar::new(combined_progress));
                }
            });
        });

        ui.add_space(8.0);

        egui::Frame::group(ui.style())
            .fill(SURFACE_COLOR)
            .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
            .rounding(Rounding::same(8.0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width().min(720.0));
                ui.horizontal(|ui| {
                    ui.strong(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.separator();
                    ui.label(format!("{} — {}", CREATOR_NAME, CREATOR_BIO));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.link(WEBSITE_URL).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(WEBSITE_URL));
                        }
                    });
                });
            });

        ui.add_space(10.0);
        if ui.add(secondary_button("Back")).clicked() {
            app.screen = Screen::Dashboard;
        }
    });
}