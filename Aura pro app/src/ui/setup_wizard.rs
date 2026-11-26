use super::{
    primary_button, secondary_button, AmbilightApp, Screen, BORDER_COLOR, SURFACE_COLOR,
};
use crate::provision::{AP_PASS, AP_SSID, VALID_GPIO_PINS};
use eframe::egui::{self, Rounding, Stroke};

pub fn draw_setup_ap(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    ui.heading("Setup — AP Mode");
    ui.separator();
    ui.add_space(10.0);

    ui.vertical_centered(|ui| {
        egui::Frame::group(ui.style())
            .fill(SURFACE_COLOR)
            .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
            .rounding(Rounding::same(8.0))
            .show(ui, |ui| {
                ui.set_min_width(540.0);
                ui.horizontal(|ui| {
                    ui.label("SSID:");
                    ui.strong(AP_SSID);
                    ui.add_space(24.0);
                    ui.label("Password:");
                    ui.strong(AP_PASS);
                    if ui.add(secondary_button("Copy")).clicked() {
                        ui.ctx().copy_text(AP_PASS.to_string());
                        app.status_msg = "Copied".to_string();
                    }
                });
            });

        ui.add_space(14.0);
        if ui.add(primary_button("Next")).clicked() {
            app.screen = Screen::SetupWifiPin;
            app.scan_wifis();
        }
    });
}

pub fn draw_setup_wifi_pin(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    ui.heading("Setup — Wi-Fi & Pin");
    ui.separator();
    ui.add_space(6.0);

    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            ui.label("Wi-Fi:");
            if ui
                .add_enabled(!app.wifi_scanning, secondary_button("Scan"))
                .clicked()
            {
                app.scan_wifis();
            }
            if app.wifi_scanning {
                ui.spinner();
            }

            ui.add_space(12.0);
            ui.label("Pin:");
            egui::ComboBox::from_id_salt("setup_pin_combo")
                .selected_text(format!("GPIO {}", app.selected_pin))
                .show_ui(ui, |ui| {
                    for pin in VALID_GPIO_PINS {
                        ui.selectable_value(&mut app.selected_pin, pin, format!("GPIO {}", pin));
                    }
                });
        });

        ui.add_space(6.0);

        egui::Frame::group(ui.style())
            .fill(SURFACE_COLOR)
            .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
            .rounding(Rounding::same(8.0))
            .show(ui, |ui| {
                ui.set_min_width(680.0);
                egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                    let manual = app.selected_wifi_idx.is_none();
                    if ui.radio(manual, "Manual SSID").clicked() {
                        app.selected_wifi_idx = None;
                    }
                    for (i, n) in app.networks.iter().enumerate() {
                        let lock = if n.secure { "Locked" } else { "Open" };
                        let text = format!("{} — {} ({} dBm)", n.ssid, lock, n.rssi);
                        if ui.radio(app.selected_wifi_idx == Some(i), text).clicked() {
                            app.selected_wifi_idx = Some(i);
                        }
                    }
                });
            });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            if app.selected_wifi_idx.is_none() {
                ui.label("SSID:");
                ui.text_edit_singleline(&mut app.manual_ssid);
            }

            ui.label("Password:");
            ui.add(
                egui::TextEdit::singleline(&mut app.wifi_password).password(!app.show_password),
            );
            let toggle_lbl = if app.show_password { "Hide" } else { "Show" };
            if ui.add(secondary_button(toggle_lbl)).clicked() {
                app.show_password = !app.show_password;
            }
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            if ui.add(secondary_button("Back")).clicked() {
                app.screen = Screen::SetupApConnect;
            }
            if ui
                .add_enabled(!app.setup_submitting, primary_button("Save & Reboot"))
                .clicked()
            {
                let ssid = if let Some(idx) = app.selected_wifi_idx {
                    app.networks
                        .get(idx)
                        .map(|n| n.ssid.clone())
                        .unwrap_or_default()
                } else {
                    app.manual_ssid.trim().to_string()
                };

                if ssid.is_empty() {
                    app.status_msg = "Enter SSID".to_string();
                } else {
                    app.submit_ap_setup(ssid, app.wifi_password.clone(), app.selected_pin);
                }
            }
            if app.setup_submitting {
                ui.spinner();
            }
        });
    });
}

pub fn draw_setup_reconnect(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    ui.heading("Setup — Reconnect");
    ui.separator();
    ui.add_space(12.0);

    ui.vertical_centered(|ui| {
        ui.label("Reconnect PC to home Wi-Fi.");
        ui.add_space(8.0);
        app.draw_esp_status_bar(ui);
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            if ui.add(secondary_button("Back")).clicked() {
                app.screen = Screen::SetupWifiPin;
            }
            if ui.add(primary_button("Next")).clicked() {
                app.manual_check_esp32();
                app.refresh_displays();
                app.screen = Screen::LedLayoutSetup;
            }
        });
    });
}