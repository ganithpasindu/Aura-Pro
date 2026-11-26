use super::{
    primary_button, secondary_button, AmbilightApp, EditingSide, Screen, ACCENT_COLOR, BORDER_COLOR,
    FG_COLOR, STEP_OFFSET, SURFACE_COLOR,
};
use crate::config::{ordered_sides, side_forward, Corner, Direction, Side, MAX_LEDS};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Rounding, Stroke, Vec2};
use std::sync::atomic::Ordering;

pub fn draw_monitor_layout_card(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    let size = Vec2::new(ui.available_width().min(680.0), 210.0);
    let (rect, _resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    let mon = Rect::from_min_max(
        rect.min + Vec2::new(36.0, 6.0),
        rect.max - Vec2::new(36.0, 22.0),
    );
    painter.rect(
        mon,
        Rounding::same(10.0),
        SURFACE_COLOR,
        Stroke::new(2.0_f32, ACCENT_COLOR),
    );

    let stand = Rect::from_min_size(
        Pos2::new(mon.center().x - 42.0, mon.max.y),
        Vec2::new(84.0, 10.0),
    );
    painter.rect_filled(stand, Rounding::same(3.0), BORDER_COLOR);

    let inner = mon.shrink(12.0);
    let mut dots = Vec::new();
    for i in 0..app.layout.top {
        let t = (i as f32 + 0.5) / app.layout.top.max(1) as f32;
        dots.push(Pos2::new(
            egui::lerp(inner.left()..=inner.right(), t),
            inner.top(),
        ));
    }
    for i in 0..app.layout.right {
        let t = (i as f32 + 0.5) / app.layout.right.max(1) as f32;
        dots.push(Pos2::new(
            inner.right(),
            egui::lerp(inner.top()..=inner.bottom(), t),
        ));
    }
    for i in 0..app.layout.bottom {
        let t = (i as f32 + 0.5) / app.layout.bottom.max(1) as f32;
        dots.push(Pos2::new(
            egui::lerp(inner.right()..=inner.left(), t),
            inner.bottom(),
        ));
    }
    for i in 0..app.layout.left {
        let t = (i as f32 + 0.5) / app.layout.left.max(1) as f32;
        dots.push(Pos2::new(
            inner.left(),
            egui::lerp(inner.bottom()..=inner.top(), t),
        ));
    }

    if app.is_streaming {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }
    let time = ui.input(|i| i.time);
    let anim_pos = if !dots.is_empty() {
        let raw = ((time * 8.0) % (dots.len() as f64)) as usize;
        match app.layout.direction {
            Direction::Clockwise => raw,
            Direction::CounterClockwise => (dots.len() - 1).saturating_sub(raw),
        }
    } else {
        0
    };

    for (idx, &p) in dots.iter().enumerate() {
        let pulse = !dots.is_empty() && ((idx + dots.len() - anim_pos) % dots.len() < 3);
        let col = if pulse { ACCENT_COLOR } else { BORDER_COLOR };
        painter.circle_filled(p, 3.5, col);
    }

    let start_pt = match app.layout.start_corner {
        Corner::TopLeft => inner.left_top(),
        Corner::TopRight => inner.right_top(),
        Corner::BottomRight => inner.right_bottom(),
        Corner::BottomLeft => inner.left_bottom(),
    };
    painter.circle_stroke(
        start_pt,
        7.5,
        Stroke::new(2.0_f32, Color32::from_rgb(22, 163, 74)),
    );
    painter.circle_filled(start_pt, 3.5, Color32::from_rgb(22, 163, 74));

    painter.text(
        mon.center() - Vec2::new(0.0, 22.0),
        egui::Align2::CENTER_CENTER,
        format!("{} LEDs", app.layout.total_leds()),
        egui::FontId::proportional(20.0),
        FG_COLOR,
    );

    if matches!(app.screen, Screen::Dashboard) {
        let btn_rect =
            Rect::from_center_size(mon.center() + Vec2::new(0.0, 12.0), Vec2::new(136.0, 32.0));
        let mut btn_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(btn_rect)
                .layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)),
        );
        let (label, fill) = if app.is_streaming {
            ("Stop", Color32::from_rgb(220, 38, 38))
        } else {
            ("Start", ACCENT_COLOR)
        };
        if btn_ui
            .add(
                egui::Button::new(egui::RichText::new(label).color(Color32::WHITE).strong())
                    .fill(fill),
            )
            .clicked()
        {
            if app.is_streaming {
                app.stop_live_stream();
                app.status_msg = "Stopped".to_string();
            } else {
                app.start_streaming();
            }
        }
    }
}

fn begin_edit_side(app: &mut AmbilightApp, side: EditingSide) {
    let val = match side {
        EditingSide::Top => app.layout.top,
        EditingSide::Bottom => app.layout.bottom,
        EditingSide::Left => app.layout.left,
        EditingSide::Right => app.layout.right,
    };
    app.editing_side = Some(side);
    app.side_input_buf = val.to_string();
}

fn apply_side_edit(app: &mut AmbilightApp) {
    if let Some(side) = app.editing_side {
        if let Ok(n) = app.side_input_buf.trim().parse::<usize>() {
            if n <= MAX_LEDS {
                match side {
                    EditingSide::Top => app.layout.top = n,
                    EditingSide::Bottom => app.layout.bottom = n,
                    EditingSide::Left => app.layout.left = n,
                    EditingSide::Right => app.layout.right = n,
                }
                app.flush_config_now();
            }
        }
    }
    app.editing_side = None;
}

pub fn draw_led_layout_setup(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("LED Setup");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            app.draw_esp_status_bar(ui);
        });
    });
    ui.separator();

    ui.vertical_centered(|ui| {
        draw_monitor_layout_card(app, ui);
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            ui.label("Display:");
            let current_label = app
                .available_displays
                .iter()
                .find(|d| d.index == app.layout.display_index)
                .map(|d| d.name.as_str())
                .unwrap_or("Display 1");

            egui::ComboBox::from_id_salt("led_setup_display_combo")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    let displays = app.available_displays.clone();
                    for disp in displays {
                        if ui
                            .selectable_value(
                                &mut app.layout.display_index,
                                disp.index,
                                &disp.name,
                            )
                            .clicked()
                        {
                            app.layout.display_name = Some(disp.name.clone());
                            app.flush_config_now();
                        }
                    }
                });

            if ui.add(secondary_button("Refresh")).clicked() {
                app.refresh_displays();
                app.flush_config_now();
            }

            ui.separator();

            for (side, label) in [
                (EditingSide::Top, format!("Top {}", app.layout.top)),
                (EditingSide::Bottom, format!("Bottom {}", app.layout.bottom)),
                (EditingSide::Left, format!("Left {}", app.layout.left)),
                (EditingSide::Right, format!("Right {}", app.layout.right)),
            ] {
                if ui.add(secondary_button(label)).clicked() {
                    begin_edit_side(app, side);
                }
            }
        });

        if let Some(side) = app.editing_side {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(format!("{:?}:", side));
                let r = ui.text_edit_singleline(&mut app.side_input_buf);
                if ui.add(primary_button("Apply")).clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
                {
                    apply_side_edit(app);
                }
                if ui.add(secondary_button("Cancel")).clicked() {
                    app.editing_side = None;
                }
            });
        }

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            ui.label("Start:");
            for c in Corner::all() {
                let sel = app.layout.start_corner == c;
                let btn = if sel {
                    primary_button(c.short_label())
                } else {
                    secondary_button(c.short_label())
                };
                if ui.add(btn).clicked() {
                    app.layout.start_corner = c;
                    app.flush_config_now();
                }
            }

            ui.separator();

            let cw = app.layout.direction == Direction::Clockwise;
            let cw_btn = if cw {
                primary_button("↻")
            } else {
                secondary_button("↻")
            };
            if ui.add(cw_btn).clicked() {
                app.layout.direction = Direction::Clockwise;
                app.flush_config_now();
            }

            let ccw = app.layout.direction == Direction::CounterClockwise;
            let ccw_btn = if ccw {
                primary_button("↺")
            } else {
                secondary_button("↺")
            };
            if ui.add(ccw_btn).clicked() {
                app.layout.direction = Direction::CounterClockwise;
                app.flush_config_now();
            }
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let total_w = ui.available_width().min(720.0);
            ui.add_space(((ui.available_width() - total_w) * 0.5).max(0.0));

            if app.in_initial_setup_flow {
                if ui.add(secondary_button("Back")).clicked() {
                    app.flush_config_now();
                    app.screen = Screen::SetupReconnectHome;
                }
            } else if ui.add(secondary_button("Back")).clicked() {
                app.flush_config_now();
                app.screen = Screen::Dashboard;
            }

            if ui.add(primary_button("Position Calibration")).clicked() {
                app.start_fullscreen_calibration();
            }
        });
    });
}

pub fn draw_fullscreen_calibration(app: &mut AmbilightApp, ui: &mut egui::Ui) {
    app.layout.sync_offsets();
    let total = app.layout.total_leds().max(1);

    let (k_left, k_right, k_esc) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::Escape),
        )
    });
    if k_left {
        app.cal_shift(-STEP_OFFSET);
    }
    if k_right {
        app.cal_shift(STEP_OFFSET);
    }
    if k_esc {
        app.stop_calibration_stream();
        app.flush_config_now();
        app.screen = Screen::LedLayoutSetup;
        return;
    }

    let active_idx = app.cal_led_idx.min(total - 1);
    let cur_offset = app.layout.led_offsets.get(active_idx).copied().unwrap_or(0.0);

    let avail = ui.available_rect_before_wrap();
    let (canvas, resp) = ui.allocate_exact_size(avail.size(), egui::Sense::click());
    let painter = ui.painter_at(canvas);
    painter.rect_filled(canvas, Rounding::ZERO, Color32::from_rgb(9, 13, 22));

    let box_w = 40.0;
    let box_h = 30.0;
    let border_rect = canvas.shrink(48.0);

    let mut centers: Vec<Pos2> = Vec::with_capacity(total);
    for side in ordered_sides(app.layout.start_corner, app.layout.direction) {
        let count = match side {
            Side::Bottom => app.layout.bottom,
            Side::Left => app.layout.left,
            Side::Top => app.layout.top,
            Side::Right => app.layout.right,
        };
        if count == 0 {
            continue;
        }
        let fwd = side_forward(side, app.layout.direction);
        let seg = 1.0_f32 / count as f32;
        for i in 0..count {
            let led_i = centers.len();
            let off = app.layout.led_offsets.get(led_i).copied().unwrap_or(0.0);
            let base_i = if fwd { i } else { count - 1 - i };
            let t = ((base_i as f32 + 0.5) * seg + off).clamp(0.0, 1.0);

            let pt = match side {
                Side::Top => Pos2::new(
                    egui::lerp(border_rect.left()..=border_rect.right(), t),
                    border_rect.top(),
                ),
                Side::Bottom => Pos2::new(
                    egui::lerp(border_rect.left()..=border_rect.right(), t),
                    border_rect.bottom(),
                ),
                Side::Left => Pos2::new(
                    border_rect.left(),
                    egui::lerp(border_rect.top()..=border_rect.bottom(), t),
                ),
                Side::Right => Pos2::new(
                    border_rect.right(),
                    egui::lerp(border_rect.top()..=border_rect.bottom(), t),
                ),
            };
            centers.push(pt);
        }
    }

    let mut picked: Option<usize> = None;
    let ptr = resp.interact_pointer_pos();

    for (i, &c_pt) in centers.iter().enumerate() {
        let b_rect = Rect::from_center_size(c_pt, Vec2::new(box_w, box_h));
        let active = i == active_idx;
        let (fill, stroke, txt_col) = if active {
            (
                Color32::from_rgb(22, 163, 74),
                Stroke::new(3.0_f32, Color32::from_rgb(134, 239, 172)),
                Color32::WHITE,
            )
        } else {
            (
                Color32::from_rgb(30, 58, 138),
                Stroke::new(1.2_f32, Color32::from_rgb(96, 165, 250)),
                Color32::from_rgb(241, 245, 249),
            )
        };
        painter.rect(b_rect, Rounding::same(6.0), fill, stroke);
        painter.text(
            b_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{}", i + 1),
            egui::FontId::proportional(14.0),
            txt_col,
        );

        if resp.clicked() {
            if let Some(pos) = ptr {
                if b_rect.contains(pos) {
                    picked = Some(i);
                }
            }
        }
    }
    if let Some(idx) = picked {
        app.set_cal_idx(idx);
    }

    let card = Rect::from_center_size(canvas.center(), Vec2::new(460.0, 210.0));
    painter.rect(
        card,
        Rounding::same(12.0),
        Color32::from_rgb(15, 23, 42),
        Stroke::new(1.5_f32, Color32::from_rgb(51, 65, 85)),
    );

    let mut c_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(card.shrink(14.0))
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );

    let esp_badge = if app.esp_connected.load(Ordering::Relaxed) {
        "Connected"
    } else {
        "Disconnected"
    };
    c_ui.label(
        egui::RichText::new(format!(
            "LED #{} / {}  |  {:+.3}  |  {}",
            active_idx + 1,
            total,
            cur_offset,
            esp_badge
        ))
        .strong()
        .size(15.0)
        .color(Color32::from_rgb(74, 222, 128)),
    );
    c_ui.add_space(8.0);

    c_ui.horizontal(|ui| {
        if ui
            .add_sized(
                [100.0, 32.0],
                egui::Button::new("Prev").fill(Color32::from_rgb(30, 41, 59)),
            )
            .clicked()
        {
            app.cal_prev();
        }
        if ui
            .add_sized(
                [100.0, 32.0],
                egui::Button::new("Next").fill(Color32::from_rgb(30, 41, 59)),
            )
            .clicked()
        {
            app.cal_next();
        }
        if ui
            .add_sized(
                [100.0, 32.0],
                egui::Button::new("Move -").fill(ACCENT_COLOR),
            )
            .clicked()
        {
            app.cal_shift(-STEP_OFFSET);
        }
        if ui
            .add_sized(
                [100.0, 32.0],
                egui::Button::new("Move +").fill(ACCENT_COLOR),
            )
            .clicked()
        {
            app.cal_shift(STEP_OFFSET);
        }
    });

    c_ui.add_space(12.0);

    c_ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            app.stop_calibration_stream();
            app.flush_config_now();
            app.screen = Screen::LedLayoutSetup;
        }
        if ui.button("Exit").clicked() {
            app.stop_calibration_stream();
            app.flush_config_now();
            if app.in_initial_setup_flow {
                app.start_fullscreen_white_tuning();
            } else {
                app.screen = Screen::Dashboard;
            }
        }
        if app.in_initial_setup_flow {
            if ui
                .add(
                    egui::Button::new("White Tuning")
                        .fill(Color32::from_rgb(22, 163, 74)),
                )
                .clicked()
            {
                app.start_fullscreen_white_tuning();
            }
        }
    });
}