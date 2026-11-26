pub mod dashboard;
pub mod led_setup;
pub mod settings;
pub mod setup_wizard;
pub mod white_tuning;

use crate::capture::{
    list_connected_displays, run_ambilight_stream, run_calibration_single_led_stream,
    run_white_calibration_stream, DisplayInfo,
};
use crate::config::{
    load_layout_config, reset_layout_config, save_layout_config, LayoutConfig, TuningConfig,
};
use crate::provision::{
    fast_target_from_cache, fetch_scanned_wifis, resolve_esp32_addr_with_cache, send_ap_setup,
    ScannedWifi,
};
use crate::update_app::{run_app_update_flow, AppUpdateEvent};
use crate::update_esp32::{run_esp32_ota_flow, EspUpdateEvent};
use eframe::egui::{self, Color32, Rounding, Stroke};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const STEP_OFFSET: f32 = 0.015;
pub const BG_COLOR: Color32 = Color32::from_rgb(0xEB, 0xF4, 0xFF);
pub const FG_COLOR: Color32 = Color32::from_rgb(0x0F, 0x1B, 0x2E);
pub const ACCENT_COLOR: Color32 = Color32::from_rgb(0x25, 0x63, 0xEB);
pub const SURFACE_COLOR: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const BORDER_COLOR: Color32 = Color32::from_rgb(0xBF, 0xDB, 0xFE);

const APP_TITLE: &str = "Aura Pro";
const TRAY_ICON_PNG_BYTES: &[u8] = include_bytes!("../../assets/icon.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    SetupApConnect,
    SetupWifiPin,
    SetupReconnectHome,
    Dashboard,
    LedLayoutSetup,
    FullscreenCalibration,
    FullscreenWhiteTuning,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditingSide {
    Top,
    Bottom,
    Left,
    Right,
}

enum ScanEvent {
    Done(Result<Vec<ScannedWifi>, String>),
}

enum SetupSubmitEvent {
    Done(String, Result<String, String>),
}

fn apply_app_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BG_COLOR;
    visuals.window_fill = BG_COLOR;
    visuals.extreme_bg_color = SURFACE_COLOR;
    visuals.faint_bg_color = Color32::from_rgb(0xF8, 0xFB, 0xFF);
    visuals.hyperlink_color = ACCENT_COLOR;
    visuals.selection.bg_fill = ACCENT_COLOR;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);

    visuals.widgets.noninteractive.bg_fill = SURFACE_COLOR;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, FG_COLOR);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);

    visuals.widgets.inactive.bg_fill = SURFACE_COLOR;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, FG_COLOR);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_COLOR);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xDB, 0xEA, 0xFE);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, FG_COLOR);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_COLOR);

    visuals.widgets.active.bg_fill = Color32::from_rgb(0xBF, 0xDB, 0xFE);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, FG_COLOR);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT_COLOR);

    visuals.widgets.open.bg_fill = SURFACE_COLOR;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, FG_COLOR);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, ACCENT_COLOR);

    ctx.set_visuals(visuals);
}

pub fn primary_button(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label).color(Color32::WHITE).strong())
        .fill(ACCENT_COLOR)
        .stroke(Stroke::new(1.0_f32, ACCENT_COLOR))
        .rounding(Rounding::same(6.0))
}

pub fn secondary_button(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label).color(FG_COLOR))
        .fill(SURFACE_COLOR)
        .stroke(Stroke::new(1.0_f32, BORDER_COLOR))
        .rounding(Rounding::same(6.0))
}

fn build_tray_icon() -> Option<Icon> {
    let img = image::load_from_memory(TRAY_ICON_PNG_BYTES)
        .ok()?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).ok()
}

#[cfg(windows)]
fn win32_find_aura_hwnd() -> isize {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
    }

    let title: Vec<u16> = OsStr::new(APP_TITLE)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
}

#[cfg(not(windows))]
fn win32_find_aura_hwnd() -> isize {
    0
}

#[cfg(windows)]
fn win32_hide_hwnd(hwnd: isize) {
    extern "system" {
        fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
    }
    const SW_HIDE: i32 = 0;
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn win32_hide_hwnd(_hwnd: isize) {}

#[cfg(windows)]
pub(crate) fn win32_force_restore_hwnd(saved_hwnd: isize) {
    extern "system" {
        fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        fn SetForegroundWindow(hWnd: isize) -> i32;
        fn BringWindowToTop(hWnd: isize) -> i32;
    }

    const SW_SHOW: i32 = 5;
    const SW_RESTORE: i32 = 9;

    let hwnd = if saved_hwnd != 0 {
        saved_hwnd
    } else {
        win32_find_aura_hwnd()
    };

    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn win32_force_restore_hwnd(_saved_hwnd: isize) {}

#[cfg(windows)]
fn win32_close_hwnd(saved_hwnd: isize) {
    extern "system" {
        fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
    }

    const WM_CLOSE: u32 = 0x0010;
    let hwnd = if saved_hwnd != 0 {
        saved_hwnd
    } else {
        win32_find_aura_hwnd()
    };

    if hwnd != 0 {
        unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
}

#[cfg(not(windows))]
fn win32_close_hwnd(_saved_hwnd: isize) {}

pub struct AmbilightApp {
    pub screen: Screen,
    pub in_initial_setup_flow: bool,
    theme_applied: bool,
    last_fullscreen: Option<bool>,

    pub esp_connected: Arc<AtomicBool>,
    pub esp_checking: Arc<AtomicBool>,
    pub discovered_ip: Arc<Mutex<Option<String>>>,

    pub networks: Vec<ScannedWifi>,
    pub selected_wifi_idx: Option<usize>,
    pub manual_ssid: String,
    pub wifi_password: String,
    pub show_password: bool,
    pub selected_pin: u8,
    pub wifi_scanning: bool,
    pub setup_submitting: bool,
    scan_tx: Sender<ScanEvent>,
    scan_rx: Receiver<ScanEvent>,
    setup_tx: Sender<SetupSubmitEvent>,
    setup_rx: Receiver<SetupSubmitEvent>,

    pub available_displays: Vec<DisplayInfo>,
    pub layout: LayoutConfig,
    pub tuning_shared: Arc<Mutex<TuningConfig>>,
    pub editing_side: Option<EditingSide>,
    pub side_input_buf: String,
    dirty_config: bool,
    last_config_save: Instant,

    pub cal_led_idx: usize,
    cal_shared_idx: Arc<AtomicUsize>,
    cal_stop: Option<Arc<AtomicBool>>,

    white_stop: Option<Arc<AtomicBool>>,

    pub is_streaming: bool,
    stream_stop: Option<Arc<AtomicBool>>,
    stream_alive: Arc<AtomicBool>,

    pub app_update_rx: Receiver<AppUpdateEvent>,
    pub app_update_tx: Sender<AppUpdateEvent>,
    pub app_updating: bool,
    pub app_update_progress: f32,

    pub esp_update_rx: Receiver<EspUpdateEvent>,
    pub esp_update_tx: Sender<EspUpdateEvent>,
    pub esp_updating: bool,
    pub esp_elapsed_secs: u64,
    pub esp_dl_progress: f32,
    pub esp_flash_progress: f32,

    tokio_rt: tokio::runtime::Runtime,

    _tray_icon: Option<TrayIcon>,
    pub tray_show_id: tray_icon::menu::MenuId,
    pub tray_quit_id: tray_icon::menu::MenuId,
    pub saved_hwnd: Arc<AtomicIsize>,
    hidden_to_tray: bool,
    pub pending_restore: Arc<AtomicBool>,
    pub pending_exit: Arc<AtomicBool>,

    pub status_msg: String,
}

impl AmbilightApp {
    pub fn new() -> Self {
        let mut layout = load_layout_config().unwrap_or_default();
        layout.sync_offsets();

        let available_displays = list_connected_displays();
        if let Some(ref saved_name) = layout.display_name {
            if let Some(matched) = available_displays.iter().find(|d| &d.name == saved_name) {
                layout.display_index = matched.index;
            }
        } else if let Some(first) = available_displays.first() {
            layout.display_name = Some(first.name.clone());
        }

        let has_cached = layout.last_esp_ip.is_some();
        let esp_connected = Arc::new(AtomicBool::new(has_cached));
        let esp_checking = Arc::new(AtomicBool::new(false));
        let discovered_ip = Arc::new(Mutex::new(layout.last_esp_ip.clone()));

        let in_initial_setup_flow = !layout.setup_completed;
        let initial_screen = if in_initial_setup_flow {
            Screen::SetupApConnect
        } else {
            Screen::Dashboard
        };

        let tuning_shared = Arc::new(Mutex::new(layout.tuning.clone()));
        let (app_update_tx, app_update_rx) = mpsc::channel();
        let (esp_update_tx, esp_update_rx) = mpsc::channel();
        let (scan_tx, scan_rx) = mpsc::channel();
        let (setup_tx, setup_rx) = mpsc::channel();
        let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to start Tokio runtime");

        let pending_restore = Arc::new(AtomicBool::new(false));
        let pending_exit = Arc::new(AtomicBool::new(false));
        let saved_hwnd = Arc::new(AtomicIsize::new(win32_find_aura_hwnd()));

        let menu = Menu::new();
        let show_item = MenuItem::new("Open Aura Pro", true, None);
        let quit_item = MenuItem::new("Exit", true, None);
        let tray_show_id = show_item.id().clone();
        let tray_quit_id = quit_item.id().clone();
        let _ = menu.append(&show_item);
        let _ = menu.append(&quit_item);

        let tray_icon = build_tray_icon().and_then(|icon| {
            TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .with_tooltip(APP_TITLE)
                .with_icon(icon)
                .build()
                .ok()
        });

        let mut app = Self {
            screen: initial_screen,
            in_initial_setup_flow,
            theme_applied: false,
            last_fullscreen: None,
            esp_connected,
            esp_checking,
            discovered_ip,
            networks: Vec::new(),
            selected_wifi_idx: None,
            manual_ssid: String::new(),
            wifi_password: String::new(),
            show_password: false,
            selected_pin: 18,
            wifi_scanning: false,
            setup_submitting: false,
            scan_tx,
            scan_rx,
            setup_tx,
            setup_rx,
            available_displays,
            layout,
            tuning_shared,
            editing_side: None,
            side_input_buf: String::new(),
            dirty_config: false,
            last_config_save: Instant::now() - std::time::Duration::from_secs(10),
            cal_led_idx: 0,
            cal_shared_idx: Arc::new(AtomicUsize::new(0)),
            cal_stop: None,
            white_stop: None,
            is_streaming: false,
            stream_stop: None,
            stream_alive: Arc::new(AtomicBool::new(false)),
            app_update_rx,
            app_update_tx,
            app_updating: false,
            app_update_progress: 0.0,
            esp_update_rx,
            esp_update_tx,
            esp_updating: false,
            esp_elapsed_secs: 0,
            esp_dl_progress: 0.0,
            esp_flash_progress: 0.0,
            tokio_rt,
            _tray_icon: tray_icon,
            tray_show_id,
            tray_quit_id,
            saved_hwnd,
            hidden_to_tray: false,
            pending_restore,
            pending_exit,
            status_msg: "Ready".to_string(),
        };

        if app.layout.setup_completed && app.layout.tuning.auto_start {
            app.start_streaming();
        }

        app
    }

    pub fn refresh_displays(&mut self) {
        let displays = list_connected_displays();
        if let Some(ref saved_name) = self.layout.display_name {
            if let Some(matched) = displays.iter().find(|d| &d.name == saved_name) {
                self.layout.display_index = matched.index;
            }
        }
        if !displays.iter().any(|d| d.index == self.layout.display_index) {
            if let Some(first) = displays.first() {
                self.layout.display_index = first.index;
                self.layout.display_name = Some(first.name.clone());
            }
        }
        self.available_displays = displays;
    }

    fn capture_hwnd_if_needed(&self) {
        if self.saved_hwnd.load(Ordering::SeqCst) == 0 {
            let hwnd = win32_find_aura_hwnd();
            if hwnd != 0 {
                self.saved_hwnd.store(hwnd, Ordering::SeqCst);
            }
        }
    }

    fn restore_from_tray(&mut self, ctx: &egui::Context) {
        self.hidden_to_tray = false;
        self.last_fullscreen = None;
        let hwnd = self.saved_hwnd.load(Ordering::SeqCst);
        win32_force_restore_hwnd(hwnd);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn exit_from_tray(&mut self, ctx: &egui::Context) {
        self.flush_config_now();
        self.stop_all_streams();
        let hwnd = self.saved_hwnd.load(Ordering::SeqCst);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        win32_close_hwnd(hwnd);
    }

    fn sync_discovered_ip_to_config(&mut self) {
        let new_ip = {
            if let Ok(mut guard) = self.discovered_ip.lock() {
                guard.take()
            } else {
                None
            }
        };

        if let Some(ip) = new_ip {
            if self.layout.last_esp_ip.as_deref() != Some(ip.as_str()) {
                self.layout.last_esp_ip = Some(ip);
                self.flush_config_now();
            }
        }
    }

    fn sync_stream_state(&mut self) {
        let alive = self.stream_alive.load(Ordering::Relaxed);
        if self.is_streaming && !alive && self.stream_stop.is_none() {
            self.is_streaming = false;
        }
    }

    pub fn manual_check_esp32(&mut self) {
        if self.esp_checking.swap(true, Ordering::SeqCst) {
            return;
        }
        self.status_msg = "Checking ESP32...".to_string();
        let conn_arc = self.esp_connected.clone();
        let chk_arc = self.esp_checking.clone();
        let disc_ip = self.discovered_ip.clone();
        let cached_ip = self.layout.last_esp_ip.clone();
        thread::spawn(move || {
            let found = resolve_esp32_addr_with_cache(cached_ip.as_deref());
            if let Some(addr) = found {
                if let Ok(mut g) = disc_ip.lock() {
                    *g = Some(addr.ip().to_string());
                }
                conn_arc.store(true, Ordering::Relaxed);
            } else {
                conn_arc.store(false, Ordering::Relaxed);
            }
            chk_arc.store(false, Ordering::SeqCst);
        });
    }

    pub fn sync_tuning_to_shared(&self) {
        if let Ok(mut guard) = self.tuning_shared.lock() {
            *guard = self.layout.tuning.clone();
        }
    }

    pub fn mark_config_dirty(&mut self) {
        self.layout.sync_offsets();
        self.sync_tuning_to_shared();
        self.dirty_config = true;
    }

    pub fn flush_config_now(&mut self) {
        self.layout.sync_offsets();
        self.sync_tuning_to_shared();
        self.dirty_config = false;
        self.last_config_save = Instant::now();
        let _ = save_layout_config(&self.layout);
    }

    fn flush_debounced_config(&mut self) {
        if self.dirty_config
            && self.last_config_save.elapsed() >= std::time::Duration::from_millis(600)
        {
            self.flush_config_now();
        }
    }

    fn sync_window_fullscreen(&mut self, ctx: &egui::Context) {
        let want_fs = matches!(
            self.screen,
            Screen::FullscreenCalibration | Screen::FullscreenWhiteTuning
        );
        if self.last_fullscreen != Some(want_fs) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(want_fs));
            self.last_fullscreen = Some(want_fs);
        }
    }

    fn handle_minimize_to_tray(&mut self, ctx: &egui::Context) {
        self.capture_hwnd_if_needed();

        if self.pending_exit.swap(false, Ordering::SeqCst) {
            self.exit_from_tray(ctx);
            return;
        }

        if self.pending_restore.swap(false, Ordering::SeqCst) {
            self.restore_from_tray(ctx);
            return;
        }

        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        if minimized && !self.hidden_to_tray {
            self.flush_config_now();
            self.hidden_to_tray = true;
            let hwnd = self.saved_hwnd.load(Ordering::SeqCst);
            win32_hide_hwnd(hwnd);
        }
    }

    pub fn stop_calibration_stream(&mut self) {
        if let Some(f) = self.cal_stop.take() {
            f.store(true, Ordering::Relaxed);
        }
    }

    pub fn stop_white_stream(&mut self) {
        if let Some(f) = self.white_stop.take() {
            f.store(true, Ordering::Relaxed);
        }
    }

    pub fn stop_live_stream(&mut self) {
        if let Some(f) = self.stream_stop.take() {
            f.store(true, Ordering::Relaxed);
        }
        self.stream_alive.store(false, Ordering::Relaxed);
        self.is_streaming = false;
    }

    pub fn stop_all_streams(&mut self) {
        self.stop_calibration_stream();
        self.stop_white_stream();
        self.stop_live_stream();
    }

    pub fn start_streaming(&mut self) {
        self.stop_all_streams();
        self.flush_config_now();

        let total = self.layout.total_leds();
        if total == 0 {
            self.status_msg = "LED count is 0.".to_string();
            return;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let stop_c = stop.clone();
        let alive_c = self.stream_alive.clone();
        let layout_c = self.layout.clone();
        let cached_ip = self.layout.last_esp_ip.clone();
        let disc_ip = self.discovered_ip.clone();
        let tuning_c = self.tuning_shared.clone();
        let conn_c = self.esp_connected.clone();

        let fast_addr = fast_target_from_cache(cached_ip.as_deref());

        thread::spawn(move || {
            let addr = if let Some(found) = resolve_esp32_addr_with_cache(cached_ip.as_deref()) {
                if let Ok(mut g) = disc_ip.lock() {
                    *g = Some(found.ip().to_string());
                }
                conn_c.store(true, Ordering::Relaxed);
                found
            } else if cached_ip.is_some() {
                fast_addr
            } else {
                conn_c.store(false, Ordering::Relaxed);
                fast_addr
            };
            let _ = run_ambilight_stream(
                addr,
                cached_ip,
                disc_ip,
                conn_c,
                layout_c,
                tuning_c,
                stop_c,
                alive_c,
            );
        });

        self.stream_stop = Some(stop);
        self.is_streaming = true;
        self.status_msg = format!(
            "Streaming {} LEDs (Display {})",
            total,
            self.layout.display_index + 1
        );
    }

    pub fn start_fullscreen_calibration(&mut self) {
        self.stop_all_streams();
        self.flush_config_now();

        let total = self.layout.total_leds().max(1);
        if self.cal_led_idx >= total {
            self.cal_led_idx = 0;
        }
        self.cal_shared_idx.store(self.cal_led_idx, Ordering::Relaxed);
        self.screen = Screen::FullscreenCalibration;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_c = stop.clone();
        let idx_c = self.cal_shared_idx.clone();
        let conn_c = self.esp_connected.clone();
        let cached_ip = self.layout.last_esp_ip.clone();
        let disc_ip = self.discovered_ip.clone();
        let addr = fast_target_from_cache(cached_ip.as_deref());

        thread::spawn(move || {
            let _ = run_calibration_single_led_stream(
                addr,
                cached_ip,
                disc_ip,
                conn_c,
                total,
                idx_c,
                stop_c,
            );
        });

        self.cal_stop = Some(stop);
    }

    pub fn start_fullscreen_white_tuning(&mut self) {
        self.stop_all_streams();
        self.flush_config_now();
        self.screen = Screen::FullscreenWhiteTuning;

        let total = self.layout.total_leds();
        if total > 0 {
            let stop = Arc::new(AtomicBool::new(false));
            let stop_c = stop.clone();
            let tuning_c = self.tuning_shared.clone();
            let conn_c = self.esp_connected.clone();
            let cached_ip = self.layout.last_esp_ip.clone();
            let disc_ip = self.discovered_ip.clone();
            let addr = fast_target_from_cache(cached_ip.as_deref());

            thread::spawn(move || {
                let _ = run_white_calibration_stream(
                    addr,
                    cached_ip,
                    disc_ip,
                    conn_c,
                    total,
                    tuning_c,
                    stop_c,
                );
            });

            self.white_stop = Some(stop);
        }
    }

    pub fn set_cal_idx(&mut self, idx: usize) {
        let total = self.layout.total_leds().max(1);
        self.cal_led_idx = idx % total;
        self.cal_shared_idx.store(self.cal_led_idx, Ordering::Relaxed);
    }

    pub fn cal_prev(&mut self) {
        let total = self.layout.total_leds().max(1);
        let next = if self.cal_led_idx == 0 {
            total - 1
        } else {
            self.cal_led_idx - 1
        };
        self.set_cal_idx(next);
    }

    pub fn cal_next(&mut self) {
        let total = self.layout.total_leds().max(1);
        self.set_cal_idx(self.cal_led_idx + 1);
    }

    pub fn cal_shift(&mut self, delta: f32) {
        self.layout.sync_offsets();
        let total = self.layout.total_leds();
        if total == 0 || self.cal_led_idx >= self.layout.led_offsets.len() {
            return;
        }
        let cur = self.layout.led_offsets[self.cal_led_idx];
        self.layout.led_offsets[self.cal_led_idx] = (cur + delta).clamp(-0.45, 0.45);
        self.mark_config_dirty();
    }

    pub fn draw_esp_status_bar(&mut self, ui: &mut egui::Ui) {
        let connected = self.esp_connected.load(Ordering::Relaxed);
        let checking = self.esp_checking.load(Ordering::Relaxed);
        let (label, col) = if connected {
            ("ESP32 Connected", Color32::from_rgb(22, 163, 74))
        } else {
            ("ESP32 Disconnected", Color32::from_rgb(220, 38, 38))
        };

        ui.horizontal(|ui| {
            ui.colored_label(col, egui::RichText::new(label).strong());
            if ui
                .add_enabled(!checking, secondary_button("Check"))
                .clicked()
            {
                self.manual_check_esp32();
            }
        });
    }

    pub fn start_app_update(&mut self) {
        if self.app_updating {
            return;
        }
        self.app_updating = true;
        self.app_update_progress = 0.0;
        let tx = self.app_update_tx.clone();
        self.tokio_rt.spawn(async move {
            run_app_update_flow(tx).await;
        });
    }

    pub fn start_esp32_update(&mut self) {
        if self.esp_updating {
            return;
        }
        self.stop_all_streams();
        self.esp_updating = true;
        self.esp_elapsed_secs = 0;
        self.esp_dl_progress = 0.0;
        self.esp_flash_progress = 0.0;
        let tx = self.esp_update_tx.clone();
        let cached_ip = self.layout.last_esp_ip.clone();
        thread::spawn(move || {
            run_esp32_ota_flow(tx, cached_ip);
        });
    }

    pub fn trigger_reset_to_initial_setup(&mut self) {
        self.stop_all_streams();
        let cached = self.layout.last_esp_ip.clone();
        crate::provision::send_esp32_reset_message_fast(cached.as_deref());
        if let Ok(fresh) = reset_layout_config() {
            self.layout = fresh;
            self.sync_tuning_to_shared();
        }
        self.dirty_config = false;
        self.refresh_displays();
        self.in_initial_setup_flow = true;
        self.screen = Screen::SetupApConnect;
        self.status_msg = "Reset sent.".to_string();
    }

    pub fn scan_wifis(&mut self) {
        if self.wifi_scanning {
            return;
        }
        self.wifi_scanning = true;
        self.status_msg = "Scanning Wi-Fi...".to_string();
        let tx = self.scan_tx.clone();
        thread::spawn(move || {
            let res = fetch_scanned_wifis();
            let _ = tx.send(ScanEvent::Done(res));
        });
    }

    pub fn submit_ap_setup(&mut self, ssid: String, password: String, pin: u8) {
        if self.setup_submitting {
            return;
        }
        self.setup_submitting = true;
        self.status_msg = "Saving setup...".to_string();
        let tx = self.setup_tx.clone();
        thread::spawn(move || {
            let res = send_ap_setup(&ssid, &password, pin);
            let _ = tx.send(SetupSubmitEvent::Done(ssid, res));
        });
    }

    fn poll_background_events(&mut self) {
        while let Ok(ev) = self.scan_rx.try_recv() {
            self.wifi_scanning = false;
            match ev {
                ScanEvent::Done(Ok(list)) => {
                    self.networks = list;
                    if !self.networks.is_empty() && self.selected_wifi_idx.is_none() {
                        self.selected_wifi_idx = Some(0);
                    }
                    self.status_msg = format!("{} networks found", self.networks.len());
                }
                ScanEvent::Done(Err(e)) => {
                    self.status_msg = e;
                }
            }
        }

        while let Ok(ev) = self.setup_rx.try_recv() {
            self.setup_submitting = false;
            match ev {
                SetupSubmitEvent::Done(ssid, Ok(_)) => {
                    self.status_msg = format!("Reconnect PC to '{}'", ssid);
                    self.screen = Screen::SetupReconnectHome;
                }
                SetupSubmitEvent::Done(_, Err(e)) => {
                    self.status_msg = e;
                }
            }
        }

        while let Ok(ev) = self.app_update_rx.try_recv() {
            match ev {
                AppUpdateEvent::Status(s) => self.status_msg = s,
                AppUpdateEvent::Progress(p) => self.app_update_progress = p.clamp(0.0, 1.0),
                AppUpdateEvent::ReadyToInstall(path) => {
                    self.app_update_progress = 1.0;
                    self.app_updating = false;
                    self.stop_all_streams();
                    if let Err(e) = crate::update_app::launch_installer_and_exit(&path) {
                        self.status_msg = e;
                    }
                }
                AppUpdateEvent::Error(e) => {
                    self.app_updating = false;
                    self.status_msg = e;
                }
            }
        }

        while let Ok(ev) = self.esp_update_rx.try_recv() {
            match ev {
                EspUpdateEvent::Status(s) => self.status_msg = s,
                EspUpdateEvent::ElapsedSeconds(secs) => self.esp_elapsed_secs = secs,
                EspUpdateEvent::DownloadProgress(p) => self.esp_dl_progress = p.clamp(0.0, 1.0),
                EspUpdateEvent::FlashProgress(p) => self.esp_flash_progress = p.clamp(0.0, 1.0),
                EspUpdateEvent::Done(msg) => {
                    self.esp_updating = false;
                    self.status_msg = msg;
                }
                EspUpdateEvent::Error(e) => {
                    self.esp_updating = false;
                    self.status_msg = e;
                }
            }
        }
    }
}

impl eframe::App for AmbilightApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            apply_app_theme(ctx);
            self.theme_applied = true;
        }

        self.poll_background_events();
        self.sync_discovered_ip_to_config();
        self.sync_stream_state();
        self.flush_debounced_config();
        self.handle_minimize_to_tray(ctx);

        if self.hidden_to_tray {
            ctx.request_repaint_after(std::time::Duration::from_millis(350));
            return;
        }

        self.sync_window_fullscreen(ctx);

        if self.wifi_scanning
            || self.setup_submitting
            || self.esp_checking.load(Ordering::Relaxed)
            || self.esp_updating
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).fill(BG_COLOR))
            .show(ctx, |ui| {
                match self.screen {
                    Screen::FullscreenCalibration => {
                        led_setup::draw_fullscreen_calibration(self, ui);
                        return;
                    }
                    Screen::FullscreenWhiteTuning => {
                        white_tuning::draw_fullscreen_white_tuning(self, ui);
                        return;
                    }
                    _ => {}
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    match self.screen {
                        Screen::SetupApConnect => setup_wizard::draw_setup_ap(self, ui),
                        Screen::SetupWifiPin => setup_wizard::draw_setup_wifi_pin(self, ui),
                        Screen::SetupReconnectHome => setup_wizard::draw_setup_reconnect(self, ui),
                        Screen::Dashboard => dashboard::draw_dashboard(self, ui),
                        Screen::LedLayoutSetup => led_setup::draw_led_layout_setup(self, ui),
                        Screen::Settings => settings::draw_settings(self, ui),
                        Screen::FullscreenCalibration | Screen::FullscreenWhiteTuning => {
                            unreachable!()
                        }
                    }

                    ui.add_space(6.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(
                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                            |ui| {
                                ui.small(
                                    egui::RichText::new(&self.status_msg).color(FG_COLOR),
                                );
                            },
                        );
                    });
                });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.flush_config_now();
        self.stop_all_streams();
    }
}