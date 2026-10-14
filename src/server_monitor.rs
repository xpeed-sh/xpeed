//! A bounded, read-only metrics transport. Authentication stays in the host.
use crate::{
    connections::SshConfig,
    credentials::{Askpass, Secret},
};
use eframe::egui;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub struct Worker {
    result: mpsc::Receiver<Result<String, String>>,
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            std::thread::spawn(move || {
                let _ = thread.join();
            });
        }
    }
}
impl Worker {
    pub fn start(
        config: SshConfig,
        password: Option<Secret>,
        ctx: egui::Context,
        hosts: Option<std::path::PathBuf>,
    ) -> Self {
        let (tx, result) = mpsc::sync_channel(2);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let thread = std::thread::spawn(move || {
            let result = run(config, password, stopped, hosts, |sample| {
                let _ = tx.try_send(Ok(sample));
                ctx.request_repaint();
            });
            if let Err(error) = result {
                let _ = tx.try_send(Err(error));
                ctx.request_repaint();
            }
        });
        Self {
            result,
            cancel,
            thread: Some(thread),
        }
    }
    pub fn poll(&self) -> Option<Result<String, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err(xpeed_i18n::t("El canal del monitor terminó.").into()))
            }
        }
    }
}
pub fn smoke_test(
    config: SshConfig,
    password: Secret,
    hosts: std::path::PathBuf,
) -> Result<(), String> {
    let worker = Worker::start(
        config,
        Some(password),
        egui::Context::default(),
        Some(hosts),
    );
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut count = 0;
    while Instant::now() < deadline && count < 3 {
        if let Some(sample) = worker.poll() {
            let sample = sample?;
            if !sample.contains("CPU\n") || !sample.contains("PROCESSES\n") {
                return Err(xpeed_i18n::t("Muestra incompleta").into());
            }
            count += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(worker);
    if count == 3 {
        Ok(())
    } else {
        Err("El monitor no recibió tres muestras".into())
    }
}

fn line(reader: &mut impl BufRead) -> Result<Option<String>, String> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().map_err(|e| e.to_string())?;
        if buffer.is_empty() {
            return if bytes.is_empty() {
                Ok(None)
            } else {
                Err(xpeed_i18n::t("Muestra incompleta").into())
            };
        }
        let n = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buffer.len(), |i| i + 1);
        if bytes.len() + n > 65536 {
            return Err(xpeed_i18n::t("Línea de métricas demasiado grande").into());
        }
        bytes.extend_from_slice(&buffer[..n]);
        reader.consume(n);
        if bytes.last() == Some(&b'\n') {
            return Ok(Some(String::from_utf8_lossy(&bytes).trim_end().into()));
        }
    }
}
pub fn samples(reader: &mut impl BufRead, mut emit: impl FnMut(String)) -> Result<(), String> {
    let mut sample = String::new();
    let mut collecting = false;
    while let Some(line) = line(reader)? {
        if line == "BEGIN" {
            sample.clear();
            collecting = true;
        } else if line == "END" && collecting {
            emit(std::mem::take(&mut sample));
            collecting = false;
        } else if collecting {
            if sample.len() + line.len() + 1 > 1024 * 1024 {
                return Err(xpeed_i18n::t("Muestra de métricas demasiado grande").into());
            }
            sample.push_str(&line);
            sample.push('\n');
        }
    }
    Err(xpeed_i18n::t("Se cerró el canal de métricas SSH.").into())
}
fn run(
    config: SshConfig,
    password: Option<Secret>,
    cancel: Arc<AtomicBool>,
    hosts: Option<std::path::PathBuf>,
    mut emit: impl FnMut(String),
) -> Result<(), String> {
    config.validate()?;
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=4",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "NumberOfPasswordPrompts=1",
        "-o",
        if password.is_some() {
            "BatchMode=no"
        } else {
            "BatchMode=yes"
        },
        "-p",
        &config.port.to_string(),
    ]);
    if let Some(hosts) = hosts {
        command.args(["-o", &format!("UserKnownHostsFile={}", hosts.display())]);
    }
    let _broker = if let Some(password) = password {
        let broker = Askpass::new(password)?;
        command
            .env(
                "SSH_ASKPASS",
                std::env::current_exe().map_err(|e| e.to_string())?,
            )
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", "xpeed:0")
            .env("XPEED_ASKPASS_PORT", broker.port.to_string())
            .env("XPEED_ASKPASS_TOKEN", &broker.token);
        Some(broker)
    } else {
        None
    };
    if !config.user.is_empty() {
        command.args(["-l", &config.user]);
    }
    if !config.identity.is_empty() {
        command.args(["-i", &config.identity]);
    }
    command
        .args(["--", &config.host, "sh -s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| {
        xpeed_i18n::format(
            "No se pudo iniciar el monitor SSH: {e}",
            &[("e", format!("{}", e))],
        )
    })?;
    let output = child
        .stdout
        .take()
        .ok_or(xpeed_i18n::t("Monitor sin stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(xpeed_i18n::t("Monitor sin stderr"))?;
    let mut input = child
        .stdin
        .take()
        .ok_or(xpeed_i18n::t("Monitor sin stdin"))?;
    let script = include_str!("../assets/monitor-collector.sh");
    if let Err(error) = input.write_all(script.as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.to_string());
    }
    drop(input);
    let process = Arc::new(Mutex::new(child));
    let done = Arc::new(AtomicBool::new(false));
    let last = Arc::new(Mutex::new(Instant::now()));
    let watch_process = process.clone();
    let watch_done = done.clone();
    let watch_last = last.clone();
    let watch = std::thread::spawn(move || {
        while !watch_done.load(Ordering::Acquire) {
            if cancel.load(Ordering::Acquire)
                || watch_last.lock().unwrap().elapsed() > Duration::from_secs(20)
            {
                let _ = watch_process.lock().unwrap().kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    let diagnostic = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.take(8192).read_to_string(&mut text);
        text
    });
    let result = samples(&mut BufReader::new(output), |sample| {
        *last.lock().unwrap() = Instant::now();
        emit(sample);
    });
    done.store(true, Ordering::Release);
    {
        let mut child = process.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = watch.join();
    let error = diagnostic.join().unwrap_or_default();
    result.map_err(|e| {
        if error.trim().is_empty() {
            e
        } else {
            error.trim().into()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_are_bounded_and_only_complete_samples_are_published() {
        let mut emitted = Vec::new();
        assert!(
            samples(
                &mut &b"banner\nBEGIN\nCPU\ncpu 1 2\nEND\nBEGIN\npartial\n"[..],
                |s| emitted.push(s)
            )
            .is_err()
        );
        assert_eq!(emitted, ["CPU\ncpu 1 2\n"]);
        let data = format!("BEGIN\n{}\nEND\n", "x".repeat(65537));
        assert!(samples(&mut data.as_bytes(), |_| panic!("Oversized data published")).is_err());
    }
}
