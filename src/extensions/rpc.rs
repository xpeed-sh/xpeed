//! JSONL transport. Extensions execute outside the graphical application.
use eframe::egui;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

pub struct RpcProcess {
    child: Option<Child>,
    outgoing: Option<SyncSender<Value>>,
    incoming: Receiver<Result<Value, String>>,
    #[cfg(windows)]
    job: Job,
}
impl RpcProcess {
    pub fn spawn(
        executable: &std::path::Path,
        cwd: &std::path::Path,
        ctx: egui::Context,
    ) -> Result<Self, String> {
        let mut command = Command::new(executable);
        command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|e| {
            xpeed_i18n::format(
                "No se pudo iniciar la extensión: {e}",
                &[("e", format!("{}", e))],
            )
        })?;
        #[cfg(windows)]
        let job = match Job::attach(&child) {
            Ok(job) => job,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let mut input = child
            .stdin
            .take()
            .ok_or(xpeed_i18n::t("Extensión sin stdin"))?;
        let output = child
            .stdout
            .take()
            .ok_or(xpeed_i18n::t("Extensión sin stdout"))?;
        let (outgoing, rx) = mpsc::sync_channel::<Value>(64);
        let (tx, incoming) = mpsc::sync_channel(256);
        let writer_events = tx.clone();
        let writer_ctx = ctx.clone();
        std::thread::spawn(move || {
            for message in rx {
                if serde_json::to_writer(&mut input, &message)
                    .and_then(|_| {
                        input.write_all(b"\n").map_err(serde_json::Error::io)?;
                        input.flush().map_err(serde_json::Error::io)
                    })
                    .is_err()
                {
                    let _ = writer_events
                        .send(Err(
                            xpeed_i18n::t("Se cerró la entrada de la extensión.").into()
                        ));
                    writer_ctx.request_repaint();
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let result = read_message(&mut reader);
                let end = result.is_err();
                if tx.send(result).is_err() {
                    break;
                }
                ctx.request_repaint();
                if end {
                    break;
                }
            }
        });
        Ok(Self {
            child: Some(child),
            outgoing: Some(outgoing),
            incoming,
            #[cfg(windows)]
            job,
        })
    }
    pub fn send(&self, value: Value) -> Result<(), String> {
        self.outgoing
            .as_ref()
            .ok_or(xpeed_i18n::t("Extensión desconectada"))?
            .try_send(value)
            .map_err(|e| {
                xpeed_i18n::format(
                    "No se pudo enviar a la extensión: {e}",
                    &[("e", format!("{}", e))],
                )
            })
    }
    pub fn poll(&self) -> Option<Result<Value, String>> {
        self.incoming.try_recv().ok()
    }
}
pub fn read_message(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut bytes = Vec::new();
    // Bound frames before allocation; malformed output never grows memory forever.
    loop {
        let buffer = reader.fill_buf().map_err(|e| e.to_string())?;
        if buffer.is_empty() {
            return Err(xpeed_i18n::t("El proceso de la extensión terminó.").into());
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let count = end.map_or(buffer.len(), |i| i + 1);
        if bytes.len() + count > 2 * 1024 * 1024 {
            return Err(xpeed_i18n::t("Mensaje de extensión demasiado grande.").into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if end.is_some() {
            break;
        }
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| xpeed_i18n::t("La extensión envió un mensaje JSON inválido.").into())
}
impl Drop for RpcProcess {
    fn drop(&mut self) {
        let _ = self.send(json!({"jsonrpc":"2.0","id":0,"method":"shutdown","params":{}}));
        self.outgoing.take();
        let Some(mut child) = self.child.take() else {
            return;
        };
        #[cfg(windows)]
        let job = self.job.take();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_millis(750);
            while Instant::now() < deadline {
                if child.try_wait().ok().flatten().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            #[cfg(windows)]
            drop(job);
            let _ = child.kill();
            let _ = child.wait();
        });
    }
}
#[cfg(windows)]
pub(super) struct Job(Option<isize>);
#[cfg(windows)]
impl Job {
    pub(super) fn attach(child: &Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(xpeed_i18n::t(
                    "No se pudo crear el grupo de procesos de la extensión.",
                )
                .into());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE) == 0
            {
                CloseHandle(handle);
                return Err(
                    xpeed_i18n::t("No se pudo aislar el ciclo de vida de la extensión.").into(),
                );
            }
            Ok(Self(Some(handle as isize)))
        }
    }
    fn take(&mut self) -> Self {
        Self(self.0.take())
    }
}
#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle as _);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_rejects_invalid_and_oversized_json() {
        let mut input = &b"{\"id\":1}\n{\"id\":2}\n"[..];
        assert_eq!(read_message(&mut input).unwrap()["id"], 1);
        assert_eq!(read_message(&mut input).unwrap()["id"], 2);
        assert!(read_message(&mut input).is_err());
        assert!(read_message(&mut &b"not json\n"[..]).is_err());
        let bytes = vec![b'a'; 2 * 1024 * 1024 + 1];
        assert!(read_message(&mut bytes.as_slice()).is_err());
    }
}
