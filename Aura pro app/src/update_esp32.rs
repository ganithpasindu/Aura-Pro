use crate::provision::{FALLBACK_IP, MDNS_HOST};
use serde::Deserialize;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum EspUpdateEvent {
    Status(String),
    ElapsedSeconds(u64),
    DownloadProgress(f32),
    FlashProgress(f32),
    Done(String),
    Error(String),
}

#[derive(Deserialize)]
struct EspOtaStatusResponse {
    state: String,
    progress: f32,
    running: bool,
    done: bool,
    elapsed_sec: u64,
    message: String,
}

fn can_reach_version_endpoint(client: &reqwest::blocking::Client, host: &str) -> bool {
    client
        .get(format!("http://{}/version", host))
        .send()
        .and_then(|r| r.error_for_status())
        .is_ok()
}

fn find_esp32_http_host(client: &reqwest::blocking::Client, cached_ip: Option<&str>) -> String {
    if let Some(ip) = cached_ip {
        let clean = ip.trim();
        if !clean.is_empty() && can_reach_version_endpoint(client, clean) {
            return clean.to_string();
        }
    }

    if can_reach_version_endpoint(client, MDNS_HOST) {
        return MDNS_HOST.to_string();
    }

    FALLBACK_IP.to_string()
}

pub fn run_esp32_ota_flow(tx: Sender<EspUpdateEvent>, cached_ip: Option<String>) {
    if let Err(e) = inner_esp32_ota(&tx, cached_ip.as_deref()) {
        let _ = tx.send(EspUpdateEvent::Error(format!("ESP32 update failed: {}", e)));
    }
}

fn inner_esp32_ota(
    tx: &Sender<EspUpdateEvent>,
    cached_ip: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = tx.send(EspUpdateEvent::ElapsedSeconds(0));
    let _ = tx.send(EspUpdateEvent::Status("Finding ESP32...".into()));

    let probe_client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()?;
    let host = find_esp32_http_host(&probe_client, cached_ip);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let _ = tx.send(EspUpdateEvent::Status(
        "Asking ESP32 to check GitHub release...".into(),
    ));

    client
        .post(format!("http://{}/ota-pull", host))
        .send()?
        .error_for_status()?;

    let poll_start = Instant::now();
    let max_wait = Duration::from_secs(240);

    loop {
        if poll_start.elapsed() > max_wait {
            return Err("ESP32 OTA timed out".into());
        }

        thread::sleep(Duration::from_millis(250));

        match client.get(format!("http://{}/ota-status", host)).send() {
            Ok(resp) => {
                if !resp.status().is_success() {
                    continue;
                }
                let st: EspOtaStatusResponse = match resp.json() {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let elapsed = st.elapsed_sec.max(poll_start.elapsed().as_secs());
                let prog = st.progress.clamp(0.0, 1.0);

                let _ = tx.send(EspUpdateEvent::ElapsedSeconds(elapsed));
                let _ = tx.send(EspUpdateEvent::Status(st.message.clone()));

                if st.state == "downloading" {
                    let _ = tx.send(EspUpdateEvent::DownloadProgress(prog));
                } else if st.state == "flashing" || st.state == "done" {
                    let _ = tx.send(EspUpdateEvent::DownloadProgress(1.0));
                    let _ = tx.send(EspUpdateEvent::FlashProgress(prog));
                }

                if st.done {
                    if st.state == "error" {
                        return Err(st.message.into());
                    }
                    let _ = tx.send(EspUpdateEvent::DownloadProgress(1.0));
                    let _ = tx.send(EspUpdateEvent::FlashProgress(1.0));
                    let _ = tx.send(EspUpdateEvent::Done(st.message));
                    return Ok(());
                }

                if !st.running && st.state == "idle" {
                    return Err("ESP32 OTA stopped unexpectedly".into());
                }
            }
            Err(_) => {
                let elapsed = poll_start.elapsed().as_secs();
                let _ = tx.send(EspUpdateEvent::ElapsedSeconds(elapsed));
                if elapsed >= 5 && self_check_version_ready(&client, &host) {
                    let _ = tx.send(EspUpdateEvent::DownloadProgress(1.0));
                    let _ = tx.send(EspUpdateEvent::FlashProgress(1.0));
                    let _ = tx.send(EspUpdateEvent::Done(
                        "ESP32 updated and rebooted.".into(),
                    ));
                    return Ok(());
                }
            }
        }
    }
}

fn self_check_version_ready(client: &reqwest::blocking::Client, host: &str) -> bool {
    client
        .get(format!("http://{}/version", host))
        .send()
        .and_then(|r| r.error_for_status())
        .is_ok()
}