use eframe::egui;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc,
    time::Duration,
};

const RELEASES: &str = "https://github.com/xpeed-sh/xpeed/releases";
const API: &str = "https://api.github.com/repos/xpeed-sh/xpeed/releases/latest";
const MAX_INSTALLER_SIZE: u64 = 512 * 1024 * 1024;

#[derive(Default)]
pub struct Updates {
    pending: Option<mpsc::Receiver<Result<Option<Release>, String>>>,
    result: Option<Result<Option<Release>, String>>,
    download: Option<Download>,
    auto_started: bool,
}

#[derive(Clone, Debug)]
struct Release {
    version: String,
    notes: String,
    installer: Option<Installer>,
}

#[derive(Clone, Debug)]
struct Installer {
    name: String,
    url: String,
    size: u64,
    sha256: String,
}

struct Download {
    receiver: mpsc::Receiver<DownloadEvent>,
    received: u64,
    total: u64,
    ready: Option<PathBuf>,
    error: Option<String>,
}

enum DownloadEvent {
    Progress(u64, u64),
    Ready(PathBuf),
    Error(String),
}

fn version(value: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = value
        .strip_prefix('v')
        .unwrap_or(value)
        .split('.')
        .collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn parse_digest(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:")?;
    (digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| digest.to_ascii_lowercase())
}

fn parse_release(data: &Value, installed: &str) -> Result<Option<Release>, String> {
    if data["draft"].as_bool() != Some(false) || data["prerelease"].as_bool() != Some(false) {
        return Err("No stable release is available.".into());
    }
    let tag = data["tag_name"]
        .as_str()
        .ok_or("Invalid release metadata.")?;
    let candidate = version(tag).ok_or("Invalid release version.")?;
    if candidate <= version(installed).ok_or("Invalid installed version.")? {
        return Ok(None);
    }
    let number = tag.strip_prefix('v').unwrap_or(tag);
    let expected = format!("Xpeed-Setup-{number}-x64.exe");
    let prefix = format!("{RELEASES}/download/{tag}/");
    let installer = data["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|asset| {
            if asset["name"].as_str()? != expected || asset["state"].as_str()? != "uploaded" {
                return None;
            }
            let url = asset["browser_download_url"].as_str()?;
            let size = asset["size"].as_u64()?;
            let sha256 = parse_digest(asset["digest"].as_str()?)?;
            (url == format!("{prefix}{expected}") && size > 0 && size <= MAX_INSTALLER_SIZE).then(
                || Installer {
                    name: expected.clone(),
                    url: url.to_owned(),
                    size,
                    sha256,
                },
            )
        });
    Ok(Some(Release {
        version: number.into(),
        notes: data["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(16000)
            .collect(),
        installer,
    }))
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("Xpeed/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Unable to check updates.".to_string())
}

fn check() -> Result<Option<Release>, String> {
    let response = client()?
        .get(API)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .send()
        .map_err(|_| "Unable to check updates. Check your connection and retry.".to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("No public stable release is available yet.".into());
    }
    let response = response
        .error_for_status()
        .map_err(|_| "GitHub could not complete the update check. Retry later.".to_string())?;
    let mut bytes = Vec::new();
    response
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| "Unable to read release metadata.".to_string())?;
    if bytes.len() > 1_048_576 {
        return Err("Release metadata is too large.".into());
    }
    let data =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid release metadata.".to_string())?;
    parse_release(&data, env!("CARGO_PKG_VERSION"))
}

fn download(installer: Installer, sender: mpsc::Sender<DownloadEvent>) -> Result<(), String> {
    let folder = std::env::temp_dir().join("xpeed").join("updates");
    std::fs::create_dir_all(&folder).map_err(|_| "Unable to prepare the update folder.")?;
    let destination = folder.join(&installer.name);
    let temporary = folder.join(format!("{}.part", installer.name));
    let _ = std::fs::remove_file(&temporary);
    let mut response = client()?
        .get(&installer.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|_| "Unable to download the update.")?;
    if response
        .content_length()
        .is_some_and(|size| size != installer.size)
    {
        return Err("The downloaded installer size is invalid.".into());
    }
    let mut file = File::create(&temporary).map_err(|_| "Unable to save the update.")?;
    let mut hash = Sha256::new();
    let mut received = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|_| "The update download was interrupted.")?;
        if count == 0 {
            break;
        }
        received += count as u64;
        if received > installer.size || received > MAX_INSTALLER_SIZE {
            let _ = std::fs::remove_file(&temporary);
            return Err("The downloaded installer size is invalid.".into());
        }
        file.write_all(&buffer[..count])
            .map_err(|_| "Unable to save the update.")?;
        hash.update(&buffer[..count]);
        let _ = sender.send(DownloadEvent::Progress(received, installer.size));
    }
    file.flush().map_err(|_| "Unable to save the update.")?;
    if received != installer.size || format!("{:x}", hash.finalize()) != installer.sha256 {
        let _ = std::fs::remove_file(&temporary);
        return Err("The update checksum does not match the published release.".into());
    }
    let _ = std::fs::remove_file(&destination);
    std::fs::rename(&temporary, &destination).map_err(|_| "Unable to save the update.")?;
    let _ = sender.send(DownloadEvent::Ready(destination));
    Ok(())
}

impl Updates {
    fn start_check(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.pending = Some(receiver);
        self.result = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(check());
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if !self.auto_started {
            self.auto_started = true;
            self.start_check(ctx);
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.result = Some(result);
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.result = Some(Err("Unable to check updates.".into()));
                    self.pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(150));
                }
            }
        }
        if let Some(download) = &mut self.download {
            while let Ok(event) = download.receiver.try_recv() {
                match event {
                    DownloadEvent::Progress(received, total) => {
                        download.received = received;
                        download.total = total;
                    }
                    DownloadEvent::Ready(path) => download.ready = Some(path),
                    DownloadEvent::Error(error) => download.error = Some(error),
                }
            }
            if download.ready.is_none() && download.error.is_none() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }

    pub fn available(&self) -> bool {
        matches!(self.result, Some(Ok(Some(_))))
    }

    fn start_download(&mut self, installer: Installer, ctx: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        self.download = Some(Download {
            receiver,
            received: 0,
            total: installer.size,
            ready: None,
            error: None,
        });
        let wake = ctx.clone();
        std::thread::spawn(move || {
            if let Err(error) = download(installer, sender.clone()) {
                let _ = sender.send(DownloadEvent::Error(error));
            }
            wake.request_repaint();
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll(ui.ctx());
        let t = xpeed_i18n::t;
        ui.label(format!(
            "{} {}",
            t("Installed version:"),
            env!("CARGO_PKG_VERSION")
        ));
        if ui
            .add_enabled(
                self.pending.is_none(),
                egui::Button::new(t("Check for updates")),
            )
            .clicked()
        {
            self.start_check(ui.ctx());
        }
        if self.pending.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(t("Checking for updates…"));
            });
        }
        let release = self
            .result
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .and_then(Clone::clone);
        match &self.result {
            Some(Err(message)) => {
                ui.colored_label(ui.visuals().error_fg_color, t(message));
            }
            Some(Ok(None)) => {
                ui.label(t("You have the latest stable version."));
            }
            Some(Ok(Some(release))) => {
                ui.strong(format!("{} {}", t("Available version:"), release.version));
                if !release.notes.is_empty() {
                    egui::CollapsingHeader::new(t("Release notes")).show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                ui.add(egui::Label::new(&release.notes).wrap());
                            });
                    });
                }
            }
            None => {}
        }
        if let Some(download) = &self.download {
            if let Some(error) = &download.error {
                ui.colored_label(ui.visuals().error_fg_color, t(error));
            } else if let Some(path) = &download.ready {
                ui.label(t("The update is downloaded and verified."));
                if ui.button(t("Install update")).clicked() {
                    match std::process::Command::new(path).arg("/SP-").spawn() {
                        Ok(_) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                        Err(_) => {
                            self.download.as_mut().unwrap().error =
                                Some("Unable to start the installer.".into());
                        }
                    }
                }
            } else {
                let fraction = if download.total == 0 {
                    0.0
                } else {
                    download.received as f32 / download.total as f32
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .show_percentage()
                        .text(t("Downloading and verifying update…")),
                );
            }
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64"))
            && let Some(installer) = release.and_then(|release| release.installer)
            && ui.button(t("Download update")).clicked()
        {
            self.start_download(installer, ui.ctx());
        } else if self.available() && cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            ui.label(t(
                "This release has no verified Windows installer attached.",
            ));
        }
        ui.hyperlink_to(t("View releases on GitHub"), RELEASES);
        ui.add(
            egui::Label::new(t("Xpeed downloads the official installer, verifies its SHA-256 checksum, and opens it. Your settings and extensions are preserved.")).wrap(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release() -> Value {
        json!({"draft":false,"prerelease":false,"tag_name":"v0.10.8","assets":[{"state":"uploaded","name":"Xpeed-Setup-0.10.8-x64.exe","size":9482532,"digest":"sha256:fdd1e175a3f51cc1a62af2d8c415862575080f90f47a1b249b38973015c0a2d8","browser_download_url":"https://github.com/xpeed-sh/xpeed/releases/download/v0.10.8/Xpeed-Setup-0.10.8-x64.exe"}]})
    }

    #[test]
    fn compares_numbers_and_never_offers_a_downgrade() {
        assert!(version("0.10.10") > version("0.10.9"));
        assert!(parse_release(&release(), "0.10.8").unwrap().is_none());
        assert!(parse_release(&release(), "0.11.0").unwrap().is_none());
        assert!(version("v0.10.9-preview").is_none());
    }

    #[test]
    fn offers_only_exact_official_verified_installer() {
        let mut data = release();
        assert!(
            parse_release(&data, "0.10.7")
                .unwrap()
                .unwrap()
                .installer
                .is_some()
        );
        data["assets"][0]["browser_download_url"] = json!("https://example.com/installer.exe");
        assert!(
            parse_release(&data, "0.10.7")
                .unwrap()
                .unwrap()
                .installer
                .is_none()
        );
        data = release();
        data["assets"][0]["digest"] = json!("sha256:bad");
        assert!(
            parse_release(&data, "0.10.7")
                .unwrap()
                .unwrap()
                .installer
                .is_none()
        );
        data["prerelease"] = json!(true);
        assert!(parse_release(&data, "0.10.7").is_err());
    }
}
