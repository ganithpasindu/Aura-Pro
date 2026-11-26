use serde::Deserialize;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::Duration;

pub const MDNS_HOST: &str = "Aurapro.local";
pub const FALLBACK_IP: &str = "192.168.1.50";
pub const ESP32_UDP_PORT: u16 = 4210;

pub const AP_SSID: &str = "Aura Pro";
pub const AP_PASS: &str = "Aura1234";
pub const AP_IP: &str = "192.168.4.1";
pub const AP_HTTP_PORT: u16 = 80;

pub const VALID_GPIO_PINS: [u8; 14] = [2, 4, 5, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23, 25];

#[derive(Clone, Debug, Deserialize)]
pub struct ScannedWifi {
    pub ssid: String,
    pub rssi: i32,
    pub secure: bool,
}

#[derive(Debug, Deserialize)]
struct ScanResponse {
    ok: bool,
    networks: Vec<ScannedWifi>,
}

fn is_host_reachable(ip: IpAddr, timeout_ms: u64) -> bool {
    let http_addr = SocketAddr::new(ip, AP_HTTP_PORT);
    if TcpStream::connect_timeout(&http_addr, Duration::from_millis(timeout_ms)).is_ok() {
        return true;
    }
    let udp_addr = SocketAddr::new(ip, ESP32_UDP_PORT);
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        let _ = sock.set_write_timeout(Some(Duration::from_millis(timeout_ms)));
        if sock.send_to(b"AMBILIGHT_PING", udp_addr).is_ok() {
            return true;
        }
    }
    false
}

pub fn fast_target_from_cache(cached_ip: Option<&str>) -> SocketAddr {
    if let Some(ip_str) = cached_ip {
        if let Ok(ip) = ip_str.trim().parse::<IpAddr>() {
            return SocketAddr::new(ip, ESP32_UDP_PORT);
        }
    }
    format!("{}:{}", FALLBACK_IP, ESP32_UDP_PORT)
        .parse()
        .unwrap()
}

pub fn resolve_esp32_addr_with_cache(cached_ip: Option<&str>) -> Option<SocketAddr> {
    if let Some(ip_str) = cached_ip {
        if let Ok(ip) = ip_str.trim().parse::<IpAddr>() {
            if is_host_reachable(ip, 150) {
                return Some(SocketAddr::new(ip, ESP32_UDP_PORT));
            }
        }
    }

    let mdns_target = format!("{}:{}", MDNS_HOST, ESP32_UDP_PORT);
    if let Ok(mut addrs) = mdns_target.to_socket_addrs() {
        if let Some(addr) = addrs.next() {
            if is_host_reachable(addr.ip(), 200) {
                return Some(addr);
            }
        }
    }

    if let Ok(ips) = dns_lookup::lookup_host(MDNS_HOST) {
        if let Some(&ip) = ips.first() {
            if is_host_reachable(ip, 200) {
                return Some(SocketAddr::new(ip, ESP32_UDP_PORT));
            }
        }
    }

    if let Ok(fallback_ip) = FALLBACK_IP.parse::<IpAddr>() {
        if is_host_reachable(fallback_ip, 180) {
            return Some(SocketAddr::new(fallback_ip, ESP32_UDP_PORT));
        }
    }

    None
}

pub fn send_esp32_reset_message_fast(cached_ip: Option<&str>) {
    let target = fast_target_from_cache(cached_ip);
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        let _ = sock.set_write_timeout(Some(Duration::from_millis(80)));
        let _ = sock.send_to(b"AMBILIGHT_RESET", target);
    }
}

fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn fetch_scanned_wifis() -> Result<Vec<ScannedWifi>, String> {
    let ap_addr: SocketAddr = format!("{}:{}", AP_IP, AP_HTTP_PORT)
        .parse()
        .map_err(|e| format!("Invalid AP address: {}", e))?;

    let mut stream = TcpStream::connect_timeout(&ap_addr, Duration::from_secs(2))
        .map_err(|e| format!("Unable to reach {} ({}): {}", AP_SSID, AP_IP, e))?;

    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;

    let req = format!(
        "GET /scan HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        AP_IP
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("Failed to request Wi-Fi scan: {}", e))?;

    let mut raw = String::new();
    let _ = stream.read_to_string(&mut raw);

    let body = raw.split("\r\n\r\n").nth(1).unwrap_or("");
    let parsed: ScanResponse =
        serde_json::from_str(body).map_err(|e| format!("Invalid scan response: {}", e))?;

    if !parsed.ok {
        return Err("ESP32 Wi-Fi scan failed".into());
    }
    Ok(parsed.networks)
}

pub fn send_ap_setup(ssid: &str, password: &str, pin: u8) -> Result<String, String> {
    let ap_addr: SocketAddr = format!("{}:{}", AP_IP, AP_HTTP_PORT)
        .parse()
        .map_err(|e| format!("Invalid AP address: {}", e))?;

    let mut stream = TcpStream::connect_timeout(&ap_addr, Duration::from_secs(2)).map_err(|e| {
        format!(
            "Unable to reach {} ({}): {}. Connect PC to '{}' first.",
            AP_SSID, AP_IP, e, AP_SSID
        )
    })?;

    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;

    let body = format!(
        "ssid={}&password={}&pin={}",
        url_encode(ssid),
        url_encode(password),
        pin
    );

    let request = format!(
        "POST /setup HTTP/1.1\r\nHost: {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        AP_IP,
        body.len(),
        body
    );

    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("Failed to send setup data: {}", e))?;

    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    Ok(response)
}