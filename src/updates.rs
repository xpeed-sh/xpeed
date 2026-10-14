use eframe::egui;
use serde_json::Value;
use std::{io::Read, sync::mpsc, time::Duration};

const RELEASES: &str = "https://github.com/xpeed-sh/xpeed/releases";
const API: &str = "https://api.github.com/repos/xpeed-sh/xpeed/releases/latest";

#[derive(Default)]
pub struct Updates {
    pending: Option<mpsc::Receiver<Result<Option<Release>, String>>>,
    result: Option<Result<Option<Release>, String>>,
}

#[derive(Debug)]
struct Release {
    version: String,
    notes: String,
    installer: Option<String>,
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
            // Only offer the exact asset from the configured official repository.
            (url == format!("{prefix}{expected}")).then(|| url.to_owned())
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

fn check() -> Result<Option<Release>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("Xpeed/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "Unable to check updates.".to_string())?;
    let response = client
        .get(API)
        .header("Accept", "application/vnd.github+json")
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

impl Updates {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let t = xpeed_i18n::t;
        ui.label(format!(
            "{} {}",
            t("Installed version:"),
            env!("CARGO_PKG_VERSION")
        ));
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
                    ui.ctx().request_repaint_after(Duration::from_millis(150));
                }
            }
        }
        if ui
            .add_enabled(
                self.pending.is_none(),
                egui::Button::new(t("Check for updates")),
            )
            .clicked()
        {
            let (sender, receiver) = mpsc::channel();
            self.pending = Some(receiver);
            self.result = None;
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                let _ = sender.send(check());
                ctx.request_repaint();
            });
        }
        if self.pending.is_some() {
            ui.spinner();
            ui.label(t("Checking for updates…"));
        }
        if let Some(result) = &self.result {
            match result {
                Err(message) => {
                    ui.label(t(message));
                }
                Ok(None) => {
                    ui.label(t("You have the latest stable version."));
                }
                Ok(Some(release)) => {
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
                    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
                        if let Some(url) = &release.installer {
                            ui.hyperlink_to(t("Download Windows installer"), url);
                        } else {
                            ui.label(t("This release has no Windows installer attached."));
                        }
                    }
                }
            }
        }
        ui.hyperlink_to(t("View releases on GitHub"), RELEASES);
        ui.add(egui::Label::new(t("Download and run the installer to update. Close Xpeed when you are ready; your settings are preserved. No compilation is required.")).wrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn release() -> Value {
        json!({"draft":false,"prerelease":false,"tag_name":"v0.10.8","assets":[{"state":"uploaded","name":"Xpeed-Setup-0.10.8-x64.exe","browser_download_url":"https://github.com/xpeed-sh/xpeed/releases/download/v0.10.8/Xpeed-Setup-0.10.8-x64.exe"}]})
    }
    #[test]
    fn compares_numbers_and_never_offers_a_downgrade() {
        assert!(version("0.10.10") > version("0.10.9"));
        assert!(parse_release(&release(), "0.10.8").unwrap().is_none());
        assert!(parse_release(&release(), "0.11.0").unwrap().is_none());
        assert!(version("v0.10.9-preview").is_none());
    }
    #[test]
    fn offers_only_exact_official_installer() {
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
        data["prerelease"] = json!(true);
        assert!(parse_release(&data, "0.10.7").is_err());
    }
}
