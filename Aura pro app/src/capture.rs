use crate::config::{build_zones, LayoutConfig, TuningConfig, ZoneRect};
use crate::provision::resolve_esp32_addr_with_cache;
use dxgi_capture_rs::{CaptureError, DXGIManager};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TARGET_FPS: u64 = 20;
const SMOOTH_SHIFT: u8 = 2;
const CHANGE_THRESHOLD: u8 = 1;
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub is_primary: bool,
}

#[cfg(windows)]
pub fn list_connected_displays() -> Vec<DisplayInfo> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    #[repr(C)]
    struct DisplayDeviceW {
        cb: u32,
        device_name: [u16; 32],
        device_string: [u16; 128],
        state_flags: u32,
        device_id: [u16; 128],
        device_key: [u16; 128],
    }

    const DISPLAY_DEVICE_ACTIVE: u32 = 0x00000001;
    const DISPLAY_DEVICE_PRIMARY_DEVICE: u32 = 0x00000004;

    extern "system" {
        fn EnumDisplayDevicesW(
            lp_device: *const u16,
            i_dev_num: u32,
            lp_display_device: *mut DisplayDeviceW,
            dw_flags: u32,
        ) -> i32;
    }

    let mut list = Vec::new();
    let mut dev_idx: u32 = 0;

    unsafe {
        loop {
            let mut dd = DisplayDeviceW {
                cb: std::mem::size_of::<DisplayDeviceW>() as u32,
                device_name: [0; 32],
                device_string: [0; 128],
                state_flags: 0,
                device_id: [0; 128],
                device_key: [0; 128],
            };

            if EnumDisplayDevicesW(std::ptr::null(), dev_idx, &mut dd, 0) == 0 {
                break;
            }

            if (dd.state_flags & DISPLAY_DEVICE_ACTIVE) != 0 {
                let raw_name = OsString::from_wide(
                    &dd.device_name[..dd.device_name.iter().position(|&c| c == 0).unwrap_or(32)],
                )
                .to_string_lossy()
                .into_owned();

                let raw_desc = OsString::from_wide(
                    &dd.device_string
                        [..dd.device_string.iter().position(|&c| c == 0).unwrap_or(128)],
                )
                .to_string_lossy()
                .into_owned();

                let is_primary = (dd.state_flags & DISPLAY_DEVICE_PRIMARY_DEVICE) != 0;
                let label = if raw_desc.trim().is_empty() {
                    format!(
                        "Display {} ({}){}",
                        list.len() + 1,
                        raw_name,
                        if is_primary { " [Primary]" } else { "" }
                    )
                } else {
                    format!(
                        "Display {} — {}{}",
                        list.len() + 1,
                        raw_desc.trim(),
                        if is_primary { " [Primary]" } else { "" }
                    )
                };

                list.push(DisplayInfo {
                    index: list.len(),
                    name: label,
                    is_primary,
                });
            }

            dev_idx += 1;
        }
    }

    if list.is_empty() {
        list.push(DisplayInfo {
            index: 0,
            name: "Display 1 [Primary]".to_string(),
            is_primary: true,
        });
    }

    list
}

#[cfg(not(windows))]
pub fn list_connected_displays() -> Vec<DisplayInfo> {
    vec![DisplayInfo {
        index: 0,
        name: "Display 1".to_string(),
        is_primary: true,
    }]
}

#[derive(Clone, Copy, Default)]
struct RgbU32 {
    r: u32,
    g: u32,
    b: u32,
}

#[derive(Clone)]
struct TuningLut {
    gamma: f32,
    brightness: f32,
    wb_r: f32,
    wb_g: f32,
    wb_b: f32,
    r: [u8; 256],
    g: [u8; 256],
    b: [u8; 256],
}

impl TuningLut {
    fn from_tuning(t: &TuningConfig) -> Self {
        let g_exp = t.gamma.clamp(0.5, 3.0);
        let br = t.brightness.clamp(0.05, 1.0);
        let wr = t.wb_r.clamp(0.2, 1.2);
        let wg = t.wb_g.clamp(0.2, 1.2);
        let wb = t.wb_b.clamp(0.2, 1.2);

        let mut lut = Self {
            gamma: g_exp,
            brightness: br,
            wb_r: wr,
            wb_g: wg,
            wb_b: wb,
            r: [0; 256],
            g: [0; 256],
            b: [0; 256],
        };

        for i in 0..256 {
            let norm = (i as f32) / 255.0;
            let curved = norm.powf(g_exp);
            lut.r[i] = (curved * br * wr * 255.0).round().clamp(0.0, 255.0) as u8;
            lut.g[i] = (curved * br * wg * 255.0).round().clamp(0.0, 255.0) as u8;
            lut.b[i] = (curved * br * wb * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        lut
    }

    fn matches(&self, t: &TuningConfig) -> bool {
        (self.gamma - t.gamma).abs() < 1e-4
            && (self.brightness - t.brightness).abs() < 1e-4
            && (self.wb_r - t.wb_r).abs() < 1e-4
            && (self.wb_g - t.wb_g).abs() < 1e-4
            && (self.wb_b - t.wb_b).abs() < 1e-4
    }
}

struct PrecomputedZoneSamples {
    indices: Vec<usize>,
}

fn build_zone_sample_indices(
    zones: &[ZoneRect],
    fw: usize,
    fh: usize,
    total_pixels: usize,
) -> Vec<PrecomputedZoneSamples> {
    let (step_x, step_y) = if fw > 2560 || fh > 1440 {
        (6usize, 4usize)
    } else if fw > 1920 || fh > 1080 {
        (4usize, 3usize)
    } else {
        (3usize, 2usize)
    };

    zones
        .iter()
        .map(|z| {
            let mut indices = Vec::new();
            let mut y = z.y0;
            while y < z.y1 {
                let row_offset = y * fw;
                let mut x = z.x0;
                while x < z.x1 {
                    let idx = row_offset + x;
                    if idx < total_pixels {
                        indices.push(idx);
                    }
                    x += step_x;
                }
                y += step_y;
            }
            PrecomputedZoneSamples { indices }
        })
        .collect()
}

#[inline(always)]
fn smooth_u32(current: u32, target: u32, shift: u8) -> u32 {
    if target >= current {
        current + ((target - current) >> shift)
    } else {
        current - ((current - target) >> shift)
    }
}

fn create_dxgi(
    timeout_ms: u32,
    display_index: usize,
) -> Result<DXGIManager, Box<dyn std::error::Error + Send + Sync>> {
    let mut dxgi = DXGIManager::new(timeout_ms)?;
    dxgi.set_capture_source_index(display_index);
    Ok(dxgi)
}

fn build_adalight_header(num_zones: usize) -> [u8; 6] {
    let hi = ((num_zones - 1) >> 8) as u8;
    let lo = ((num_zones - 1) & 0xff) as u8;
    let chk = hi ^ lo ^ 0x55;
    [b'A', b'd', b'a', hi, lo, chk]
}

fn open_connected_udp(addr: SocketAddr) -> Option<UdpSocket> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    let _ = sock.set_write_timeout(Some(Duration::from_millis(25)));
    let _ = sock.connect(addr);
    Some(sock)
}

fn send_with_auto_reconnect(
    socket: &mut Option<UdpSocket>,
    addr: &mut SocketAddr,
    cached_ip: Option<&str>,
    discovered_ip: &Arc<Mutex<Option<String>>>,
    connected_flag: &Arc<AtomicBool>,
    packet: &[u8],
    last_resolve: &mut Instant,
) -> bool {
    if socket.is_none() {
        *socket = open_connected_udp(*addr);
    }

    let should_refresh = match socket {
        Some(ref s) => match s.send(packet) {
            Ok(_) => {
                connected_flag.store(true, Ordering::Relaxed);
                if let Ok(mut guard) = discovered_ip.lock() {
                    *guard = Some(addr.ip().to_string());
                }
                return true;
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                {
                    return false;
                }
                true
            }
        },
        None => true,
    };

    if should_refresh && last_resolve.elapsed() >= Duration::from_secs(2) {
        *last_resolve = Instant::now();
        if let Some(new_addr) = resolve_esp32_addr_with_cache(cached_ip) {
            *addr = new_addr;
            if let Ok(mut guard) = discovered_ip.lock() {
                *guard = Some(new_addr.ip().to_string());
            }
            *socket = open_connected_udp(*addr);
            if let Some(ref s) = socket {
                if s.send(packet).is_ok() {
                    connected_flag.store(true, Ordering::Relaxed);
                    return true;
                }
            }
        } else {
            connected_flag.store(false, Ordering::Relaxed);
        }
    }

    false
}

pub fn run_calibration_single_led_stream(
    mut target_addr: SocketAddr,
    cached_ip: Option<String>,
    discovered_ip: Arc<Mutex<Option<String>>>,
    connected_flag: Arc<AtomicBool>,
    total_leds: usize,
    active_led_idx: Arc<AtomicUsize>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if total_leds == 0 {
        return Ok(());
    }

    let mut socket = open_connected_udp(target_addr);
    let mut last_resolve = Instant::now() - Duration::from_secs(5);
    let header = build_adalight_header(total_leds);
    let mut packet = vec![0u8; 6 + total_leds * 3];
    packet[..6].copy_from_slice(&header);

    let mut last_idx = usize::MAX;
    let mut last_send = Instant::now() - Duration::from_secs(2);

    while !stop_flag.load(Ordering::Relaxed) {
        let cur_idx = active_led_idx.load(Ordering::Relaxed).min(total_leds - 1);

        if cur_idx != last_idx || last_send.elapsed() >= Duration::from_millis(250) {
            packet[6..].fill(0);
            let base = 6 + cur_idx * 3;
            packet[base] = 34;
            packet[base + 1] = 220;
            packet[base + 2] = 110;

            send_with_auto_reconnect(
                &mut socket,
                &mut target_addr,
                cached_ip.as_deref(),
                &discovered_ip,
                &connected_flag,
                &packet,
                &mut last_resolve,
            );
            last_idx = cur_idx;
            last_send = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(20));
    }

    Ok(())
}

pub fn run_white_calibration_stream(
    mut target_addr: SocketAddr,
    cached_ip: Option<String>,
    discovered_ip: Arc<Mutex<Option<String>>>,
    connected_flag: Arc<AtomicBool>,
    total_leds: usize,
    tuning_shared: Arc<Mutex<TuningConfig>>,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if total_leds == 0 {
        return Ok(());
    }

    let mut socket = open_connected_udp(target_addr);
    let mut last_resolve = Instant::now() - Duration::from_secs(5);
    let header = build_adalight_header(total_leds);
    let mut packet = vec![0u8; 6 + total_leds * 3];
    packet[..6].copy_from_slice(&header);

    let init_t = tuning_shared.lock().map(|g| g.clone()).unwrap_or_default();
    let mut lut = TuningLut::from_tuning(&init_t);

    while !stop_flag.load(Ordering::Relaxed) {
        let t = tuning_shared.lock().map(|g| g.clone()).unwrap_or_default();
        if !lut.matches(&t) {
            lut = TuningLut::from_tuning(&t);
        }

        let r = lut.r[255];
        let g = lut.g[255];
        let b = lut.b[255];

        for i in 0..total_leds {
            let base = 6 + i * 3;
            packet[base] = r;
            packet[base + 1] = g;
            packet[base + 2] = b;
        }

        send_with_auto_reconnect(
            &mut socket,
            &mut target_addr,
            cached_ip.as_deref(),
            &discovered_ip,
            &connected_flag,
            &packet,
            &mut last_resolve,
        );
        std::thread::sleep(Duration::from_millis(75));
    }

    Ok(())
}

pub fn run_ambilight_stream(
    mut target_addr: SocketAddr,
    cached_ip: Option<String>,
    discovered_ip: Arc<Mutex<Option<String>>>,
    connected_flag: Arc<AtomicBool>,
    layout: LayoutConfig,
    tuning_shared: Arc<Mutex<TuningConfig>>,
    stop_flag: Arc<AtomicBool>,
    alive_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    alive_flag.store(true, Ordering::SeqCst);

    let num_zones = layout.total_leds();
    if num_zones == 0 {
        alive_flag.store(false, Ordering::SeqCst);
        return Err("Total LED count is 0".into());
    }

    let mut socket = open_connected_udp(target_addr);
    let mut last_resolve = Instant::now() - Duration::from_secs(5);

    let frame_duration = Duration::from_millis(1000 / TARGET_FPS);
    let timeout_ms = frame_duration.as_millis().max(10) as u32;
    let display_idx = layout.display_index;
    let mut dxgi = match create_dxgi(timeout_ms, display_idx) {
        Ok(d) => d,
        Err(e) => {
            alive_flag.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };

    let header = build_adalight_header(num_zones);

    let mut cached_dims: Option<(usize, usize)> = None;
    let mut precomputed_zones: Vec<PrecomputedZoneSamples> = Vec::new();
    let mut smoothed = vec![RgbU32::default(); num_zones];
    let mut first_frame = true;

    let init_t = tuning_shared.lock().map(|g| g.clone()).unwrap_or_default();
    let mut lut = TuningLut::from_tuning(&init_t);

    let payload_len = num_zones * 3;
    let mut packet: Vec<u8> = Vec::with_capacity(6 + payload_len);
    let mut last_sent_rgb: Vec<u8> = vec![0; payload_len];
    let mut has_sent_once = false;
    let mut last_send_time = Instant::now() - KEEPALIVE_INTERVAL;

    while !stop_flag.load(Ordering::Relaxed) {
        let loop_start = Instant::now();
        let tuning = tuning_shared.lock().map(|g| g.clone()).unwrap_or_default();
        if !lut.matches(&tuning) {
            lut = TuningLut::from_tuning(&tuning);
        }

        match dxgi.capture_frame() {
            Ok((pixels, (frame_w, frame_h))) => {
                let fw = frame_w as usize;
                let fh = frame_h as usize;

                if cached_dims != Some((fw, fh)) {
                    let zones = build_zones(fw, fh, &layout);
                    precomputed_zones = build_zone_sample_indices(&zones, fw, fh, pixels.len());
                    cached_dims = Some((fw, fh));
                }

                packet.clear();
                packet.extend_from_slice(&header);

                for (zone_idx, z_samples) in precomputed_zones.iter().enumerate() {
                    let mut r_sum: u64 = 0;
                    let mut g_sum: u64 = 0;
                    let mut b_sum: u64 = 0;

                    for &p_idx in &z_samples.indices {
                        let p = pixels[p_idx];
                        r_sum += p.r as u64;
                        g_sum += p.g as u64;
                        b_sum += p.b as u64;
                    }

                    let count = z_samples.indices.len() as u64;
                    let (avg_r, avg_g, avg_b) = if count > 0 {
                        (
                            ((r_sum / count) as usize).min(255),
                            ((g_sum / count) as usize).min(255),
                            ((b_sum / count) as usize).min(255),
                        )
                    } else {
                        (0, 0, 0)
                    };

                    let cur = &mut smoothed[zone_idx];
                    if first_frame {
                        cur.r = avg_r as u32;
                        cur.g = avg_g as u32;
                        cur.b = avg_b as u32;
                    } else {
                        cur.r = smooth_u32(cur.r, avg_r as u32, SMOOTH_SHIFT);
                        cur.g = smooth_u32(cur.g, avg_g as u32, SMOOTH_SHIFT);
                        cur.b = smooth_u32(cur.b, avg_b as u32, SMOOTH_SHIFT);
                    }

                    packet.push(lut.r[(cur.r as usize).min(255)]);
                    packet.push(lut.g[(cur.g as usize).min(255)]);
                    packet.push(lut.b[(cur.b as usize).min(255)]);
                }

                first_frame = false;

                let current_rgb = &packet[6..];
                let changed = !has_sent_once
                    || current_rgb
                        .iter()
                        .zip(last_sent_rgb.iter())
                        .any(|(&now, &prev)| now.abs_diff(prev) >= CHANGE_THRESHOLD);
                let keep_alive = last_send_time.elapsed() >= KEEPALIVE_INTERVAL;

                if changed || keep_alive {
                    let sent = send_with_auto_reconnect(
                        &mut socket,
                        &mut target_addr,
                        cached_ip.as_deref(),
                        &discovered_ip,
                        &connected_flag,
                        &packet,
                        &mut last_resolve,
                    );
                    if sent {
                        if changed {
                            last_sent_rgb.copy_from_slice(current_rgb);
                            has_sent_once = true;
                        }
                        last_send_time = Instant::now();
                    }
                }
            }
            Err(CaptureError::Timeout) => {}
            Err(_) => {
                std::thread::sleep(Duration::from_millis(350));
                if let Ok(new_dxgi) = create_dxgi(timeout_ms, display_idx) {
                    dxgi = new_dxgi;
                    cached_dims = None;
                    first_frame = true;
                }
            }
        }

        let elapsed = loop_start.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }

    alive_flag.store(false, Ordering::SeqCst);
    Ok(())
}