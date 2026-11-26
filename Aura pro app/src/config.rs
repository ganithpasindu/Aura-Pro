use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

pub const BORDER_THICKNESS: usize = 20;
pub const MAX_LEDS: usize = 300;
pub const CONFIG_FILE: &str = "aura_pro_config.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Corner {
    BottomRight,
    BottomLeft,
    TopLeft,
    TopRight,
}

impl Corner {
    pub fn all() -> [Self; 4] {
        [
            Self::TopLeft,
            Self::TopRight,
            Self::BottomRight,
            Self::BottomLeft,
        ]
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            Self::BottomRight => "Bottom-Right",
            Self::BottomLeft => "Bottom-Left",
            Self::TopLeft => "Top-Left",
            Self::TopRight => "Top-Right",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Bottom,
    Left,
    Top,
    Right,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TuningConfig {
    pub brightness: f32,
    pub gamma: f32,
    pub wb_r: f32,
    pub wb_g: f32,
    pub wb_b: f32,
    pub auto_start: bool,
}

impl Default for TuningConfig {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            gamma: 2.2,
            wb_r: 1.0,
            wb_g: 1.0,
            wb_b: 1.0,
            auto_start: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutConfig {
    pub bottom: usize,
    pub left: usize,
    pub top: usize,
    pub right: usize,
    pub start_corner: Corner,
    pub direction: Direction,
    #[serde(default)]
    pub display_index: usize,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub led_offsets: Vec<f32>,
    #[serde(default)]
    pub tuning: TuningConfig,
    #[serde(default)]
    pub setup_completed: bool,
    #[serde(default)]
    pub last_esp_ip: Option<String>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        let mut cfg = Self {
            bottom: 20,
            left: 10,
            top: 20,
            right: 10,
            start_corner: Corner::TopLeft,
            direction: Direction::Clockwise,
            display_index: 0,
            display_name: None,
            led_offsets: Vec::new(),
            tuning: TuningConfig::default(),
            setup_completed: false,
            last_esp_ip: None,
        };
        cfg.sync_offsets();
        cfg
    }
}

impl LayoutConfig {
    pub fn total_leds(&self) -> usize {
        self.bottom + self.left + self.top + self.right
    }

    pub fn sync_offsets(&mut self) {
        let n = self.total_leds().min(MAX_LEDS);
        if self.led_offsets.len() != n {
            self.led_offsets.resize(n, 0.0);
        }
        for off in &mut self.led_offsets {
            *off = off.clamp(-0.45, 0.45);
        }
        self.tuning.brightness = self.tuning.brightness.clamp(0.05, 1.0);
        self.tuning.gamma = self.tuning.gamma.clamp(0.5, 3.0);
        self.tuning.wb_r = self.tuning.wb_r.clamp(0.2, 1.2);
        self.tuning.wb_g = self.tuning.wb_g.clamp(0.2, 1.2);
        self.tuning.wb_b = self.tuning.wb_b.clamp(0.2, 1.2);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ZoneRect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize,
    pub y1: usize,
}

pub fn config_file_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("AuraPro");
        let _ = fs::create_dir_all(&dir);
        return dir.join(CONFIG_FILE);
    }

    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let _ = fs::create_dir_all(exe_dir);
            return exe_dir.join(CONFIG_FILE);
        }
    }

    PathBuf::from(CONFIG_FILE)
}

pub fn load_layout_config() -> Option<LayoutConfig> {
    let target_path = config_file_path();

    if !target_path.exists() {
        let legacy = PathBuf::from(CONFIG_FILE);
        if legacy.exists() {
            if let Some(parent) = target_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::copy(&legacy, &target_path);
        }
    }

    if !target_path.exists() {
        return None;
    }

    let raw = fs::read_to_string(&target_path).ok()?;
    let mut cfg: LayoutConfig = serde_json::from_str(&raw).ok()?;
    if cfg.total_leds() == 0 || cfg.total_leds() > MAX_LEDS {
        return None;
    }
    cfg.sync_offsets();
    Some(cfg)
}

pub fn save_layout_config(cfg: &LayoutConfig) -> io::Result<()> {
    let mut copy = cfg.clone();
    copy.sync_offsets();
    let json = serde_json::to_string_pretty(&copy).map_err(io::Error::other)?;

    let target_path = config_file_path();
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(target_path, json)
}

pub fn reset_layout_config() -> io::Result<LayoutConfig> {
    let mut fresh = LayoutConfig::default();
    fresh.setup_completed = false;
    save_layout_config(&fresh)?;
    Ok(fresh)
}

pub fn ordered_sides(corner: Corner, dir: Direction) -> [Side; 4] {
    match (corner, dir) {
        (Corner::BottomRight, Direction::Clockwise) => {
            [Side::Bottom, Side::Left, Side::Top, Side::Right]
        }
        (Corner::BottomLeft, Direction::Clockwise) => {
            [Side::Left, Side::Top, Side::Right, Side::Bottom]
        }
        (Corner::TopLeft, Direction::Clockwise) => {
            [Side::Top, Side::Right, Side::Bottom, Side::Left]
        }
        (Corner::TopRight, Direction::Clockwise) => {
            [Side::Right, Side::Bottom, Side::Left, Side::Top]
        }
        (Corner::BottomRight, Direction::CounterClockwise) => {
            [Side::Right, Side::Top, Side::Left, Side::Bottom]
        }
        (Corner::BottomLeft, Direction::CounterClockwise) => {
            [Side::Bottom, Side::Right, Side::Top, Side::Left]
        }
        (Corner::TopLeft, Direction::CounterClockwise) => {
            [Side::Left, Side::Bottom, Side::Right, Side::Top]
        }
        (Corner::TopRight, Direction::CounterClockwise) => {
            [Side::Top, Side::Left, Side::Bottom, Side::Right]
        }
    }
}

pub fn side_forward(side: Side, dir: Direction) -> bool {
    match (side, dir) {
        (Side::Bottom, Direction::Clockwise) => false,
        (Side::Left, Direction::Clockwise) => false,
        (Side::Top, Direction::Clockwise) => true,
        (Side::Right, Direction::Clockwise) => true,
        (Side::Bottom, Direction::CounterClockwise) => true,
        (Side::Right, Direction::CounterClockwise) => false,
        (Side::Top, Direction::CounterClockwise) => false,
        (Side::Left, Direction::CounterClockwise) => true,
    }
}

pub fn build_zones(fw: usize, fh: usize, cfg: &LayoutConfig) -> Vec<ZoneRect> {
    let total = cfg.total_leds();
    let mut zones = Vec::with_capacity(total);
    let border_x = BORDER_THICKNESS.min(fw / 3).max(1);
    let border_y = BORDER_THICKNESS.min(fh / 3).max(1);

    let y_top0 = 0;
    let y_top1 = border_y.min(fh);
    let y_bot0 = fh.saturating_sub(border_y);
    let y_bot1 = fh;

    let x_left0 = 0;
    let x_left1 = border_x.min(fw);
    let x_right0 = fw.saturating_sub(border_x);
    let x_right1 = fw;

    for side in ordered_sides(cfg.start_corner, cfg.direction) {
        let count = match side {
            Side::Bottom => cfg.bottom,
            Side::Left => cfg.left,
            Side::Top => cfg.top,
            Side::Right => cfg.right,
        };
        if count == 0 {
            continue;
        }

        let forward = side_forward(side, cfg.direction);
        let seg_len = 1.0_f32 / count as f32;
        let half_seg = seg_len * 0.5;

        for i in 0..count {
            let led_idx = zones.len();
            let offset = cfg.led_offsets.get(led_idx).copied().unwrap_or(0.0);
            let base_idx = if forward { i } else { count - 1 - i };

            let base_center = (base_idx as f32 + 0.5) * seg_len;
            let shifted_center = (base_center + offset).clamp(0.0, 1.0);
            let t0 = (shifted_center - half_seg).clamp(0.0, 1.0);
            let t1 = (shifted_center + half_seg).clamp(0.0, 1.0);

            let rect = match side {
                Side::Bottom | Side::Top => {
                    let y0 = if side == Side::Bottom { y_bot0 } else { y_top0 };
                    let y1 = if side == Side::Bottom { y_bot1 } else { y_top1 };
                    let x0 = ((t0 * fw as f32).floor() as usize).min(fw.saturating_sub(1));
                    let x1 = ((t1 * fw as f32).ceil() as usize).max(x0 + 1).min(fw);
                    ZoneRect { x0, y0, x1, y1 }
                }
                Side::Left | Side::Right => {
                    let x0 = if side == Side::Left { x_left0 } else { x_right0 };
                    let x1 = if side == Side::Left { x_left1 } else { x_right1 };
                    let y0 = ((t0 * fh as f32).floor() as usize).min(fh.saturating_sub(1));
                    let y1 = ((t1 * fh as f32).ceil() as usize).max(y0 + 1).min(fh);
                    ZoneRect { x0, y0, x1, y1 }
                }
            };
            zones.push(rect);
        }
    }

    zones
}