#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod config;
mod provision;
mod ui;
mod update_app;
mod update_esp32;

use eframe::egui;
use std::sync::atomic::Ordering;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
use ui::AmbilightApp;

const ICON_PNG_BYTES: &[u8] = include_bytes!("../assets/icon.png");

fn load_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(ICON_PNG_BYTES).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

#[cfg(windows)]
fn acquire_single_instance() -> Option<isize> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn CreateMutexW(
            lpMutexAttributes: *mut std::ffi::c_void,
            bInitialOwner: i32,
            lpName: *const u16,
        ) -> isize;
        fn GetLastError() -> u32;
        fn CloseHandle(hObject: isize) -> i32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;

    let name: Vec<u16> = OsStr::new("Global\\AuraProAmbilightSingleInstanceMutex")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let handle = CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
        if handle == 0 {
            return None;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(handle);
            return None;
        }
        Some(handle)
    }
}

#[cfg(not(windows))]
fn acquire_single_instance() -> Option<i32> {
    Some(1)
}

fn main() -> eframe::Result<()> {
    let _instance_mutex = match acquire_single_instance() {
        Some(h) => h,
        None => return Ok(()),
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([780.0, 440.0])
        .with_min_inner_size([680.0, 380.0])
        .with_title("Aura Pro");

    if let Some(icon_data) = load_window_icon() {
        viewport = viewport.with_icon(icon_data);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Aura Pro",
        options,
        Box::new(|cc| {
            let app = AmbilightApp::new();

            let r_menu = app.pending_restore.clone();
            let q_menu = app.pending_exit.clone();
            let show_id = app.tray_show_id.clone();
            let quit_id = app.tray_quit_id.clone();
            let hwnd_menu = app.saved_hwnd.clone();
            let ctx_menu = cc.egui_ctx.clone();

            MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
                if ev.id == show_id {
                    r_menu.store(true, Ordering::SeqCst);
                    ui::win32_force_restore_hwnd(hwnd_menu.load(Ordering::SeqCst));
                    ctx_menu.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx_menu.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx_menu.request_repaint();
                } else if ev.id == quit_id {
                    q_menu.store(true, Ordering::SeqCst);
                    ui::win32_force_restore_hwnd(hwnd_menu.load(Ordering::SeqCst));
                    ctx_menu.send_viewport_cmd(egui::ViewportCommand::Close);
                    ctx_menu.request_repaint();
                }
            }));

            let r_click = app.pending_restore.clone();
            let hwnd_click = app.saved_hwnd.clone();
            let ctx_click = cc.egui_ctx.clone();

            TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
                match ev {
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    } => {
                        r_click.store(true, Ordering::SeqCst);
                        ui::win32_force_restore_hwnd(hwnd_click.load(Ordering::SeqCst));
                        ctx_click.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx_click.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx_click.request_repaint();
                    }
                    _ => {}
                }
            }));

            cc.egui_ctx.request_repaint();
            Ok(Box::new(app))
        }),
    )
}