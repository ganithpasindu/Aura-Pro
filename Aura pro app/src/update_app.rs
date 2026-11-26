use futures_util::StreamExt;
use semver::Version;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{exit, Command};
use std::sync::mpsc::Sender;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub const GITHUB_OWNER: &str = "ganithpasindu";
pub const GITHUB_REPO: &str = "Aura-Pro";
pub const SETUP_ASSET_NAME: &str = "Aura_pro_Setup.exe";

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
pub enum AppUpdateEvent {
    Status(String),
    Progress(f32),
    ReadyToInstall(PathBuf),
    Error(String),
}

pub fn launch_installer_and_exit(installer_path: &PathBuf) -> Result<(), String> {
    Command::new(installer_path)
        .spawn()
        .map_err(|e| format!("Failed to start installer: {}", e))?;
    exit(0);
}

pub async fn run_app_update_flow(tx: Sender<AppUpdateEvent>) {
    if let Err(err) = inner_update_flow(&tx).await {
        let _ = tx.send(AppUpdateEvent::Error(format!("App update failed: {}", err)));
    }
}

async fn inner_update_flow(
    tx: &Sender<AppUpdateEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = tx.send(AppUpdateEvent::Status("Checking GitHub releases...".into()));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let api_url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    );

    let release: GithubRelease = client
        .get(&api_url)
        .header(
            "User-Agent",
            format!("{}/{}", GITHUB_REPO, env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let latest_clean = release.tag_name.trim_start_matches('v').trim();
    let latest_ver = Version::parse(latest_clean)?;
    let current_ver = Version::parse(env!("CARGO_PKG_VERSION"))?;

    if latest_ver <= current_ver {
        let _ = tx.send(AppUpdateEvent::Status(format!(
            "Already up to date (v{}).",
            current_ver
        )));
        return Ok(());
    }

    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == SETUP_ASSET_NAME)
        .ok_or_else(|| {
            format!(
                "Asset '{}' not found in release {}",
                SETUP_ASSET_NAME, release.tag_name
            )
        })?;

    let _ = tx.send(AppUpdateEvent::Status(format!(
        "Downloading v{}...",
        latest_ver
    )));

    let resp = client
        .get(&asset.browser_download_url)
        .header(
            "User-Agent",
            format!("{}/{}", GITHUB_REPO, env!("CARGO_PKG_VERSION")),
        )
        .send()
        .await?
        .error_for_status()?;

    let total_size = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let temp_dir = std::env::temp_dir();
    let final_path = temp_dir.join(&asset.name);
    let part_path = temp_dir.join(format!("{}.part", asset.name));

    let mut file = File::create(&part_path).await?;
    let mut stream = resp.bytes_stream();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let _ = tx.send(AppUpdateEvent::Progress(downloaded as f32 / total_size as f32));
        }
    }

    file.flush().await?;
    drop(file);

    if final_path.exists() {
        let _ = tokio::fs::remove_file(&final_path).await;
    }
    tokio::fs::rename(&part_path, &final_path).await?;

    let _ = tx.send(AppUpdateEvent::ReadyToInstall(final_path));
    Ok(())
}