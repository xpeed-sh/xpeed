//! Host-owned session authority. No secrets or interactive PTY handles cross RPC.
use crate::{
    connections::SshConfig,
    credentials::{Askpass, Secret},
    terminal::Profile,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Target {
    pub id: u64,
    pub incarnation: Instant,
    pub name: String,
    pub cwd: String,
    pub connected: bool,
    pub shell: Profile,
    pub ssh: Option<SshConfig>,
    pub password: Option<Secret>,
    pub hosts: Option<PathBuf>,
    pub console: Option<crate::terminal_agent::Console>,
}
impl Target {
    pub fn public(&self) -> Value {
        json!({"sessionId":self.id,"name":self.name,"type":if self.ssh.is_some(){"ssh"}else{"local"},"state":if self.connected{"connected"}else{"disconnected"},"cwd":self.cwd,"host":self.ssh.as_ref().map(|s| &s.host),"shell":if self.ssh.is_some(){"sh"}else{self.shell.name()},"executionMode":if self.console.is_some(){"visibleTerminal"}else{"separateShell"},"environment":if self.console.is_some(){"current interactive shell; cwd, variables and activated environments are preserved"}else{"separate shell; environment is not inherited"}})
    }
}
#[derive(Default)]
pub struct Bridge {
    pub selected: BTreeSet<u64>,
    pub scope: BTreeSet<u64>,
    pub token: String,
    pub approve_task: bool,
    task_approved: bool,
    revoked: BTreeSet<u64>,
    bindings: BTreeMap<u64, Instant>,
    next: u64,
    jobs: BTreeMap<String, Job>,
    proposals: BTreeMap<String, Proposal>,
    pub actions: Vec<Action>,
    pub attached: String,
    replies: Vec<Value>,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Action {
    #[serde(default)]
    pub turn: String,
    #[serde(skip)]
    pub request: Value,
    pub tool: String,
    pub args: Value,
    pub target: String,
    pub state: String,
    pub result: Option<Value>,
}
struct Proposal {
    session: u64,
    path: String,
    before: String,
    after: String,
}
struct Job {
    session: u64,
    cancel: Arc<AtomicBool>,
    output: Arc<Mutex<Output>>,
    incoming: mpsc::Receiver<Result<Value, String>>,
    result: Option<Value>,
    reply: Option<Value>,
    action: usize,
    tool: String,
    args: Value,
}
#[derive(Default)]
struct Output {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    truncated: bool,
}
const OUTPUT_LIMIT: usize = 65536;
fn failure(id: Value, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":message}})
}
fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
impl Bridge {
    pub fn begin(&mut self, targets: &BTreeMap<u64, Target>) -> Result<Value, String> {
        if self.selected.is_empty() {
            return Err("Select at least one session.".into());
        }
        if self
            .selected
            .iter()
            .any(|id| !targets.get(id).is_some_and(|t| t.connected))
        {
            return Err("A selected session is disconnected.".into());
        }
        let session_reconnected = self.selected.iter().any(|id| {
            self.revoked.contains(id)
                || targets
                    .get(id)
                    .is_some_and(|target| self.bindings.get(id) != Some(&target.incarnation))
        });
        if self.scope != self.selected || self.token.is_empty() || session_reconnected {
            self.stop();
            if session_reconnected {
                self.jobs
                    .retain(|_, job| !self.revoked.contains(&job.session));
            }
            self.scope = self.selected.clone();
            self.revoked.clear();
            self.token = format!("scope-{:032x}", rand::random::<u128>());
            self.bindings = self
                .scope
                .iter()
                .filter_map(|id| targets.get(id).map(|t| (*id, t.incarnation)))
                .collect();
            self.proposals.clear();
        }
        self.task_approved = self.approve_task;
        Ok(
            json!({"scopeToken":self.token,"sessions":self.scope.iter().filter_map(|id| targets.get(id).map(Target::public)).collect::<Vec<_>>()}),
        )
    }
    pub fn revoke_disconnected(&mut self, targets: &BTreeMap<u64, Target>) {
        for &id in &self.scope {
            if !targets
                .get(&id)
                .is_some_and(|t| t.connected && self.bindings.get(&id) == Some(&t.incarnation))
            {
                self.revoked.insert(id);
            }
        }
        for job in self.jobs.values() {
            if self.revoked.contains(&job.session) {
                job.cancel.store(true, Ordering::Release);
            }
        }
        for action in &mut self.actions {
            if action.state == "awaitingApproval"
                && action.args["sessionId"]
                    .as_u64()
                    .is_some_and(|id| self.revoked.contains(&id))
            {
                action.state = "failed".into();
                self.replies.push(failure(
                    action.request.clone(),
                    "The selected session disconnected before approval.",
                ));
            }
        }
    }
    fn authorize<'a>(
        &self,
        args: &Value,
        targets: &'a BTreeMap<u64, Target>,
    ) -> Result<&'a Target, String> {
        let id = args["sessionId"].as_u64().ok_or("sessionId is required.")?;
        if !self.scope.contains(&id) || self.revoked.contains(&id) {
            return Err(
                "Session is outside the authorized conversation scope or was disconnected.".into(),
            );
        }
        targets
            .get(&id)
            .filter(|t| t.connected && self.bindings.get(&id) == Some(&t.incarnation))
            .ok_or_else(|| {
                "The original session is disconnected or reconnected; no fallback is allowed."
                    .into()
            })
    }
    pub fn receive(&mut self, request: Value, targets: &BTreeMap<u64, Target>) {
        let id = request["id"].clone();
        let p = &request["params"];
        if self.token.is_empty() || p["scopeToken"] != self.token {
            self.replies
                .push(failure(id, "Expired conversation scope."));
            return;
        }
        let tool = p["tool"].as_str().unwrap_or_default();
        let args = p["arguments"].clone();
        if args.to_string().len() > 200000 {
            self.replies
                .push(failure(id, "Tool arguments exceed the supported limit."));
            return;
        }
        if tool == "xpeed_list_sessions" {
            self.replies.push(success(id, json!({"sessions":self.scope.iter().filter_map(|id|targets.get(id).map(Target::public)).collect::<Vec<_>>()})));
            return;
        }
        let target = match self.authorize(&args, targets) {
            Ok(t) => t,
            Err(e) => {
                self.replies.push(failure(id, &e));
                return;
            }
        };
        if tool == "xpeed_read_terminal" {
            let result = target
                .console
                .as_ref()
                .ok_or_else(|| "No visible terminal is available for this session.".to_owned())
                .and_then(|console| console.snapshot());
            self.replies.push(match result {
                Ok(text) => success(
                    id,
                    json!({"sessionId":target.id,"text":text,"untrusted":true}),
                ),
                Err(error) => failure(id, &error),
            });
            return;
        }
        if matches!(tool, "xpeed_task_status" | "xpeed_cancel_task") {
            let task = args["taskId"].as_str().unwrap_or_default();
            if let Some(job) = self
                .jobs
                .get_mut(task)
                .filter(|job| job.session == target.id)
            {
                if tool == "xpeed_cancel_task" {
                    job.cancel.store(true, Ordering::Release);
                }
                let out = job.output.lock().unwrap();
                self.replies.push(success(id, job.result.clone().unwrap_or_else(|| json!({"sessionId":target.id,"taskId":task,"state":if job.cancel.load(Ordering::Acquire){"cancelling"}else{"running"},"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr),"exitCode":null,"outputTruncated":out.truncated,"remoteMayBeRunning":target.ssh.is_some() && job.cancel.load(Ordering::Acquire)}))));
            } else {
                self.replies.push(failure(
                    id,
                    "Task does not belong to this session/conversation.",
                ));
            }
            return;
        }
        if !matches!(
            tool,
            "xpeed_exec"
                | "xpeed_read_file"
                | "xpeed_list_directory"
                | "xpeed_propose_change"
                | "xpeed_apply_change"
        ) {
            self.replies.push(failure(id, "Unknown Xpeed tool."));
            return;
        }
        if self.actions.len() >= 256 {
            self.replies.push(failure(
                id,
                "Conversation action limit reached; start a new conversation.",
            ));
            return;
        }
        let mut args = args;
        if tool == "xpeed_apply_change" {
            let proposal = args["proposalId"]
                .as_str()
                .and_then(|id| self.proposals.get(id));
            let Some(proposal) = proposal.filter(|p| p.session == target.id) else {
                self.replies
                    .push(failure(id, "Unknown proposal or mismatched session."));
                return;
            };
            args["path"] = json!(proposal.path);
            args["expectedContent"] = json!(proposal.before);
            args["newContent"] = json!(proposal.after);
        }
        self.actions.push(Action {
            turn: String::new(),
            request: id,
            tool: tool.into(),
            args,
            target: format!("{} · #{}", target.name, target.id),
            state: "awaitingApproval".into(),
            result: None,
        });
        let index = self.actions.len() - 1;
        // Reads are permitted by the explicit session selection. Commands and edits
        // require review, or a user-approved whole task scoped to those same ids.
        if !matches!(tool, "xpeed_exec" | "xpeed_apply_change") || self.task_approved {
            self.approve(index, targets);
        }
    }
    pub fn approve(&mut self, index: usize, targets: &BTreeMap<u64, Target>) {
        let action = &self.actions[index];
        if action.state != "awaitingApproval" {
            return;
        }
        let id = action.request.clone();
        let tool = action.tool.clone();
        let args = action.args.clone();
        let target = match self.authorize(&args, targets) {
            Ok(t) => t.clone(),
            Err(e) => {
                self.actions[index].state = "failed".into();
                self.replies.push(failure(id, &e));
                return;
            }
        };
        if self.jobs.values().filter(|j| j.result.is_none()).count() >= 8 {
            self.replies
                .push(failure(id, "At most 8 operations can run concurrently."));
            self.actions[index].state = "failed".into();
            return;
        }
        self.next += 1;
        let task = format!("{}-{}", self.token, self.next);
        let cancel = Arc::new(AtomicBool::new(false));
        let output = Arc::new(Mutex::new(Output::default()));
        let (tx, rx) = mpsc::channel();
        let job_cancel = cancel.clone();
        let job_output = output.clone();
        let job_tool = tool.clone();
        let job_args = args.clone();
        let job_task = task.clone();
        std::thread::spawn(move || {
            let done = Arc::new(AtomicBool::new(false));
            if job_tool != "xpeed_exec" {
                let finished = done.clone();
                let timeout_cancel = job_cancel.clone();
                std::thread::spawn(move || {
                    let start = Instant::now();
                    while !finished.load(Ordering::Acquire) {
                        if start.elapsed() > Duration::from_secs(25) {
                            timeout_cancel.store(true, Ordering::Release);
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(100));
                    }
                });
            }
            let result = operate(
                &target, &job_tool, &job_args, &job_task, job_cancel, job_output,
            );
            done.store(true, Ordering::Release);
            let _ = tx.send(result);
        });
        let reply = if tool == "xpeed_exec" {
            self.replies.push(success(id,json!({"sessionId":args["sessionId"],"taskId":task,"state":"running","stdout":"","stderr":"","exitCode":null,"outputTruncated":false})));
            None
        } else {
            Some(id)
        };
        self.actions[index].state = "running".into();
        self.jobs.insert(
            task,
            Job {
                session: args["sessionId"].as_u64().unwrap(),
                cancel,
                output,
                incoming: rx,
                result: None,
                reply,
                action: index,
                tool,
                args,
            },
        );
    }
    pub fn decline(&mut self, index: usize) {
        let action = &mut self.actions[index];
        action.state = "declined".into();
        self.replies.push(failure(
            action.request.clone(),
            "The user declined this operation.",
        ));
    }
    pub fn poll(&mut self) {
        for (task, job) in &mut self.jobs {
            if job.result.is_some() {
                continue;
            }
            let Ok(result) = job.incoming.try_recv() else {
                if job.tool == "xpeed_exec" {
                    let out = job.output.lock().unwrap();
                    self.actions[job.action].result = Some(
                        json!({"sessionId":job.session,"taskId":task,"state":if job.cancel.load(Ordering::Acquire){"cancelling"}else{"running"},"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr),"exitCode":null,"outputTruncated":out.truncated}),
                    );
                }
                continue;
            };
            let mut value = match result {
                Ok(v) => v,
                Err(e) => json!({"state":"failed","error":e}),
            };
            value["sessionId"] = json!(job.session);
            value["taskId"] = json!(task);
            if job.tool == "xpeed_propose_change" && value["state"] == "completed" {
                let before = value["content"].as_str().unwrap_or_default().to_owned();
                let after = job.args["newContent"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                value = json!({"sessionId":job.session,"taskId":task,"proposalId":task,"state":"completed","path":job.args["path"],"diff":diff(&before,&after)});
                self.proposals.insert(
                    task.clone(),
                    Proposal {
                        session: job.session,
                        path: job.args["path"].as_str().unwrap().into(),
                        before,
                        after,
                    },
                );
            }
            let action = &mut self.actions[job.action];
            action.state = value["state"].as_str().unwrap_or("completed").into();
            action.result = Some(value.clone());
            if let Some(id) = job.reply.take() {
                self.replies.push(success(id, value.clone()));
            }
            job.result = Some(value);
        }
    }
    pub fn take_replies(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.replies)
    }
    pub fn stop(&mut self) {
        self.task_approved = false;
        for job in self.jobs.values() {
            if job.result.is_none() {
                job.cancel.store(true, Ordering::Release);
            }
        }
        for action in &mut self.actions {
            if action.state == "awaitingApproval" {
                action.state = "cancelled".into();
                self.replies.push(failure(
                    action.request.clone(),
                    "Task cancelled before approval.",
                ));
            }
        }
    }
    pub fn invalidate(&mut self) {
        self.stop();
        self.token.clear();
        self.scope.clear();
        self.proposals.clear();
    }
    pub fn reset(&mut self) {
        self.stop();
        self.jobs.clear();
        self.actions.clear();
        self.proposals.clear();
        self.token.clear();
        self.scope.clear();
        self.revoked.clear();
        self.replies.clear();
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

fn diff(before: &str, after: &str) -> String {
    // A complete replacement diff is intentionally simple and bounded; it never
    // hides removed lines. The exact old text is checked before writing.
    let mut d = String::from("--- before\n+++ after\n");
    for line in before.lines() {
        d.push('-');
        d.push_str(line);
        d.push('\n');
    }
    for line in after.lines() {
        d.push('+');
        d.push_str(line);
        d.push('\n');
    }
    d
}
fn path_arg<'a>(args: &'a Value, target: &Target) -> Result<&'a str, String> {
    let path = args["path"]
        .as_str()
        .ok_or("An absolute path is required.")?;
    if path.contains('\0')
        || if target.ssh.is_some() {
            !path.starts_with('/')
        } else {
            !Path::new(path).is_absolute()
        }
    {
        return Err("An absolute path is required.".into());
    }
    // Explicit attachments and file tools never collect standard credential stores.
    if credential_path(path) {
        return Err("Credential files cannot be attached or read by file tools.".into());
    }
    Ok(path)
}
pub(crate) fn credential_path(path: &str) -> bool {
    path.replace('\\', "/").to_lowercase().split('/').any(|s| {
        matches!(
            s,
            ".ssh" | ".aws" | ".azure" | ".gnupg" | ".codex" | "codex-home"
        ) || s == ".env"
            || s.starts_with(".env.")
    })
}
fn operate(
    target: &Target,
    tool: &str,
    args: &Value,
    task: &str,
    cancel: Arc<AtomicBool>,
    output: Arc<Mutex<Output>>,
) -> Result<Value, String> {
    if cancel.load(Ordering::Acquire) {
        return Err("Task cancelled.".into());
    }
    if tool == "xpeed_exec" {
        return execute(target, args, task, cancel, output);
    }
    let path = path_arg(args, target)?;
    if tool == "xpeed_list_directory" {
        let entries = if let Some(config) = &target.ssh {
            crate::sftp::tool_list(
                config,
                target.password.clone(),
                cancel,
                path,
                target.hosts.as_deref(),
            )?
            .entries
            .into_iter()
            .take(2000)
            .map(|e| json!({"name":e.name,"directory":e.directory,"bytes":e.bytes}))
            .collect::<Vec<_>>()
        } else {
            std::fs::read_dir(path).map_err(|e|e.to_string())?.take(2000).map(|e|{let e=e.map_err(|e|e.to_string())?;let m=e.metadata().map_err(|e|e.to_string())?;Ok(json!({"name":e.file_name().to_string_lossy(),"directory":m.is_dir(),"bytes":m.len()}))}).collect::<Result<Vec<_>,String>>()?
        };
        return Ok(json!({"state":"completed","path":path,"entries":entries,"entryLimit":2000}));
    }
    let content = if let Some(config) = &target.ssh {
        crate::sftp::tool_read(
            config,
            target.password.clone(),
            cancel.clone(),
            path,
            target.hosts.as_deref(),
        )?
    } else {
        read_local(path)?
    };
    if tool == "xpeed_apply_change" {
        if content != args["expectedContent"].as_str().unwrap_or_default() {
            return Err("File changed since the proposal; read it again.".into());
        }
        let after = args["newContent"]
            .as_str()
            .ok_or("newContent is required.")?;
        if after.len() > 65536 {
            return Err("Text file exceeds 64 KiB.".into());
        }
        if cancel.load(Ordering::Acquire) {
            return Err("Task cancelled before writing.".into());
        }
        if let Some(config) = &target.ssh {
            crate::sftp::tool_replace(
                config,
                target.password.clone(),
                cancel,
                path,
                &content,
                after,
                target.hosts.as_deref(),
            )?;
        } else {
            replace_local(path, &content, after)?;
        }
        return Ok(json!({"state":"completed","path":path,"bytes":after.len()}));
    }
    if tool == "xpeed_propose_change"
        && !args["newContent"]
            .as_str()
            .is_some_and(|s| s.len() <= 65536)
    {
        return Err("newContent must be UTF-8 text of at most 64 KiB.".into());
    }
    Ok(json!({"state":"completed","path":path,"content":content,"outputTruncated":false}))
}
fn read_local(path: &str) -> Result<String, String> {
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let dummy = Target {
        id: 0,
        incarnation: Instant::now(),
        name: String::new(),
        cwd: String::new(),
        connected: true,
        shell: Profile::default_shell(),
        ssh: None,
        password: None,
        hosts: None,
        console: None,
    };
    path_arg(&json!({"path":canonical.to_string_lossy()}), &dummy)?;
    let mut bytes = Vec::new();
    std::fs::File::open(canonical)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Text file exceeds 64 KiB.".into());
    }
    String::from_utf8(bytes).map_err(|_| "Only UTF-8 text files are supported.".into())
}
fn replace_local(path: &str, expected: &str, after: &str) -> Result<(), String> {
    let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("File has no parent directory.")?;
    let temporary = parent.join(format!(".xpeed-{:032x}.tmp", rand::random::<u128>()));
    let result = (|| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.set_permissions(
            std::fs::metadata(&path)
                .map_err(|e| e.to_string())?
                .permissions(),
        )
        .map_err(|e| e.to_string())?;
        file.write_all(after.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        if read_local(&path.to_string_lossy())? != expected {
            return Err("File changed since the proposal; read it again.".into());
        }
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
fn collect(mut reader: impl Read, output: Arc<Mutex<Output>>, stderr: bool) {
    let mut buffer = [0u8; 4096];
    while let Ok(n) = reader.read(&mut buffer) {
        if n == 0 {
            break;
        }
        let mut out = output.lock().unwrap();
        let bytes = if stderr {
            &mut out.stderr
        } else {
            &mut out.stdout
        };
        let kept = n.min(OUTPUT_LIMIT.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..kept]);
        out.truncated |= kept < n;
    }
}
fn execute(
    target: &Target,
    args: &Value,
    task: &str,
    cancel: Arc<AtomicBool>,
    output: Arc<Mutex<Output>>,
) -> Result<Value, String> {
    let text = args["command"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 16384 && !s.contains('\0'))
        .ok_or("command must contain 1..16384 characters.")?;
    let cwd = args["cwd"]
        .as_str()
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .ok_or("An explicit cwd is required.")?;
    let timeout = args["timeoutMs"]
        .as_u64()
        .filter(|n| (100..=300000).contains(n))
        .ok_or("timeoutMs must be between 100 and 300000.")?;
    if let Some(console) = &target.console {
        let mut result = console.execute(
            text,
            cwd,
            Duration::from_millis(timeout),
            &cancel,
            |text, truncated| {
                let mut out = output.lock().unwrap();
                out.stdout = text.as_bytes().to_vec();
                out.truncated = truncated;
            },
        )?;
        result["sessionId"] = json!(target.id);
        result["taskId"] = json!(task);
        return Ok(result);
    }
    let mut broker = None;
    let mut command;
    if let Some(config) = &target.ssh {
        config.validate()?;
        if !cwd.starts_with('/') {
            return Err("Remote cwd must be absolute.".into());
        }
        command = Command::new("ssh");
        command.args([
            "-T",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=2",
            "-o",
            "NumberOfPasswordPrompts=1",
            "-o",
            if target.password.is_some() {
                "BatchMode=no"
            } else {
                "BatchMode=yes"
            },
            "-p",
            &config.port.to_string(),
        ]);
        if let Some(hosts) = &target.hosts {
            command.args(["-o", &format!("UserKnownHostsFile={}", hosts.display())]);
        }
        if !config.user.is_empty() {
            command.args(["-l", &config.user]);
        }
        if !config.identity.is_empty() {
            command.args(["-i", &config.identity]);
        }
        if let Some(password) = &target.password {
            let access = Askpass::new(password.clone())?;
            command
                .env(
                    "SSH_ASKPASS",
                    std::env::current_exe().map_err(|e| e.to_string())?,
                )
                .env("SSH_ASKPASS_REQUIRE", "force")
                .env("DISPLAY", "xpeed:0")
                .env("XPEED_ASKPASS_PORT", access.port.to_string())
                .env("XPEED_ASKPASS_TOKEN", &access.token);
            broker = Some(access);
        }
        // Remote GNU/coreutils timeout bounds foreground processes even if the
        // SSH channel disappears. Background/daemonized children can survive.
        let script = format!(
            "cd -- {} && exec timeout -s TERM -k 2 {}s sh -c {}",
            quoted(cwd),
            timeout as f64 / 1000.0,
            quoted(text)
        );
        command.args(["--", &config.host, &script]);
    } else {
        if !Path::new(cwd).is_absolute() || !Path::new(cwd).is_dir() {
            return Err("Local cwd must be an existing absolute directory.".into());
        }
        command = match target.shell {
            Profile::PowerShell => {
                let mut c = Command::new("powershell.exe");
                c.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", text]);
                c
            }
            Profile::Cmd => {
                let mut c = Command::new("cmd.exe");
                c.args(["/D", "/S", "/C", text]);
                c
            }
            Profile::Wsl => {
                return Err(
                    "WSL execution is not supported yet; select a native local or SSH session."
                        .into(),
                );
            }
            Profile::System => {
                let mut c = Command::new("sh");
                c.args(["-c", text]);
                c
            }
        };
        command.current_dir(cwd);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let job = super::rpc::Job::attach(&child)?;
    let out = child.stdout.take().ok_or("Missing stdout")?;
    let err = child.stderr.take().ok_or("Missing stderr")?;
    let o = output.clone();
    let stdout = std::thread::spawn(move || collect(out, o, false));
    let o = output.clone();
    let stderr = std::thread::spawn(move || collect(err, o, true));
    let start = Instant::now();
    let mut state = "completed";
    let status = loop {
        if cancel.load(Ordering::Acquire)
            || start.elapsed()
                > Duration::from_millis(timeout + if target.ssh.is_some() { 10000 } else { 0 })
        {
            state = if cancel.load(Ordering::Acquire) {
                "cancelled"
            } else {
                "timedOut"
            };
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            break child.wait().map_err(|e| e.to_string())?;
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    #[cfg(windows)]
    drop(job);
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    drop(broker);
    let _ = stdout.join();
    let _ = stderr.join();
    let out = output.lock().unwrap();
    if target.ssh.is_some() && status.code() == Some(124) {
        state = "timedOut";
    }
    if state == "completed" && !status.success() {
        state = "failed";
    }
    Ok(
        json!({"sessionId":target.id,"taskId":task,"state":state,"stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr),"exitCode":status.code(),"outputTruncated":out.truncated,"remoteMayBeRunning":target.ssh.is_some() && (matches!(state,"cancelled"|"timedOut") || status.code()==Some(255)) }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires the official local Codex runtime and a signed-in ChatGPT account"]
    fn real_codex_executes_through_host_bridge() {
        let (mut bridge, mut targets, path) = setup();
        std::fs::write(path.join("xpeed-real-tool-marker.txt"), "integration check").unwrap();
        let ssh_fixture = std::env::var("XPEED_TOOL_TEST_CONFIG").ok();
        if let Some(config) = ssh_fixture.as_ref() {
            let config: Value = serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
            for (index, item) in config.as_array().unwrap().iter().enumerate() {
                let t = targets.get_mut(&((index + 1) as u64)).unwrap();
                t.cwd = "/test".into();
                t.ssh = Some(serde_json::from_value(item["connection"].clone()).unwrap());
                t.hosts = Some(PathBuf::from(item["knownHosts"].as_str().unwrap()));
            }
        }
        let cwd = targets[&1].cwd.clone();
        let marker = if ssh_fixture.is_some() {
            "marker.txt"
        } else {
            "xpeed-real-tool-marker.txt"
        };
        let scope = bridge.begin(&targets).unwrap();
        let executable =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/xpeed-codex-extension.exe");
        let process = crate::extensions::rpc::RpcProcess::spawn(
            &executable,
            Path::new(env!("CARGO_MANIFEST_DIR")),
            eframe::egui::Context::default(),
        )
        .unwrap();
        let rpc = |id: u64, method: &str| {
            process
                .send(
                    json!({"jsonrpc":"2.0","id":id,"method":method,"params":{"protocolVersion":1,"clientVersion":"integration-test"}}),
                )
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(40);
            loop {
                if let Some(value) = process.poll() {
                    let value = value.unwrap();
                    if value["id"] == id {
                        assert!(value.get("error").is_none(), "RPC failed: {value}");
                        break value["result"].clone();
                    }
                }
                assert!(Instant::now() < deadline, "RPC timeout");
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        rpc(1, "initialize");
        assert!(
            !rpc(2, "account/read")["account"].is_null(),
            "Sign in with ChatGPT before running this test"
        );
        let models = rpc(3, "models/list");
        let command = if cfg!(windows) && ssh_fixture.is_none() {
            "Get-ChildItem -Name"
        } else {
            "ls"
        };
        let prompt = format!(
            "Integration check: use xpeed_exec on sessionId 1 with command exactly {command:?}, cwd {}, timeoutMs 10000. Query xpeed_task_status until it finishes. Report the filename in stdout. Do not use any other execution or file tool. You must actually execute, not describe a command.",
            cwd
        );
        process.send(json!({"jsonrpc":"2.0","id":4,"method":"chat/start","params":{"text":prompt,"scope":scope,"model":models["models"][0]["id"]}})).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut completed = false;
        while !completed {
            while let Some(value) = process.poll() {
                let value = value.unwrap();
                assert!(value.get("error").is_none(), "Provider error: {value}");
                if value["method"] == "tools/call" {
                    let tool = value["params"]["tool"].as_str().unwrap();
                    assert!(
                        matches!(
                            tool,
                            "xpeed_exec" | "xpeed_task_status" | "xpeed_list_sessions"
                        ),
                        "Unexpected tool {tool}"
                    );
                    if tool == "xpeed_exec" {
                        assert_eq!(value["params"]["arguments"]["command"], command);
                        assert_eq!(value["params"]["arguments"]["cwd"], cwd);
                    }
                    bridge.receive(value, &targets);
                    for index in 0..bridge.actions.len() {
                        if bridge.actions[index].state == "awaitingApproval" {
                            bridge.approve(index, &targets);
                        }
                    }
                } else if value["method"] == "chat/completed" {
                    assert_eq!(
                        value["params"]["status"], "completed",
                        "Turn failed: {value}"
                    );
                    completed = true;
                }
            }
            bridge.poll();
            for reply in bridge.take_replies() {
                process.send(reply).unwrap();
            }
            assert!(Instant::now() < deadline, "Real Codex tool call timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            bridge.actions.iter().any(|a| a.tool == "xpeed_exec"
                && a.result.as_ref().is_some_and(|r| r["exitCode"] == 0
                    && r["stdout"].as_str().is_some_and(|s| s.contains(marker)))),
            "The real model did not execute through XPEED"
        );
        process
            .send(json!({"jsonrpc":"2.0","id":5,"method":"shutdown"}))
            .unwrap();
        drop(process);
        std::fs::remove_dir_all(path).unwrap();
    }
    fn target(id: u64, cwd: &Path) -> Target {
        Target {
            id,
            incarnation: Instant::now(),
            name: format!("Session {id}"),
            cwd: cwd.to_string_lossy().into(),
            connected: true,
            shell: Profile::default_shell(),
            ssh: None,
            password: None,
            hosts: None,
            console: None,
        }
    }
    fn setup() -> (Bridge, BTreeMap<u64, Target>, PathBuf) {
        let path =
            std::env::temp_dir().join(format!("xpeed-tools-test-{:032x}", rand::random::<u128>()));
        std::fs::create_dir(&path).unwrap();
        let targets = BTreeMap::from([(1, target(1, &path)), (2, target(2, &path))]);
        let mut bridge = Bridge::default();
        bridge.selected.insert(1);
        bridge.begin(&targets).unwrap();
        (bridge, targets, path)
    }
    fn call(
        bridge: &mut Bridge,
        targets: &BTreeMap<u64, Target>,
        id: &str,
        tool: &str,
        args: Value,
    ) {
        bridge.receive(
            json!({"id":id,"params":{"scopeToken":bridge.token,"tool":tool,"arguments":args}}),
            targets,
        );
    }
    fn wait(bridge: &mut Bridge) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            bridge.poll();
            if bridge.jobs.values().all(|j| j.result.is_some()) {
                return bridge.take_replies();
            }
            assert!(Instant::now() < deadline, "Tool did not finish");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn scope_rejects_unselected_sessions_and_forged_tokens_but_rebinds_reconnected_targets() {
        let (mut b, mut t, p) = setup();
        call(
            &mut b,
            &t,
            "unauthorized",
            "xpeed_read_file",
            json!({"sessionId":2,"path":p.join("file")}),
        );
        assert!(b.take_replies()[0].get("error").is_some());
        b.receive(
            json!({"id":"expired","params":{"scopeToken":"forged","tool":"xpeed_list_sessions"}}),
            &t,
        );
        assert!(b.take_replies()[0].get("error").is_some());
        let old_token = b.token.clone();
        t.get_mut(&1).unwrap().incarnation = Instant::now();
        b.revoke_disconnected(&t);
        call(
            &mut b,
            &t,
            "revoked",
            "xpeed_list_directory",
            json!({"sessionId":1,"path":p}),
        );
        assert!(b.take_replies()[0].get("error").is_some());
        b.begin(&t).unwrap();
        assert_ne!(b.token, old_token);
        assert!(b.jobs.is_empty());
        call(
            &mut b,
            &t,
            "reconnected",
            "xpeed_list_directory",
            json!({"sessionId":1,"path":p}),
        );
        let replies = wait(&mut b);
        assert!(replies[0].get("result").is_some());
        std::fs::remove_dir(p).unwrap();
    }
    #[test]
    fn approvals_cannot_be_bypassed_and_session_selection_does_not_retarget() {
        let (mut b, t, p) = setup();
        call(
            &mut b,
            &t,
            "command",
            "xpeed_exec",
            json!({"sessionId":1,"cwd":p,"command":"echo scope","timeoutMs":3000}),
        );
        assert!(b.jobs.is_empty());
        assert_eq!(b.actions[0].state, "awaitingApproval");
        b.selected.clear();
        b.selected.insert(2);
        b.approve(0, &t);
        assert_eq!(b.jobs.values().next().unwrap().session, 1);
        wait(&mut b);
        assert_eq!(b.actions[0].result.as_ref().unwrap()["sessionId"], 1);
        std::fs::remove_dir(p).unwrap();
    }
    #[test]
    fn reads_proposals_diffs_edits_and_conflict_detection_use_real_files() {
        let (mut b, t, p) = setup();
        let file = p.join("example.txt");
        std::fs::write(&file, "before\n").unwrap();
        call(
            &mut b,
            &t,
            "read",
            "xpeed_read_file",
            json!({"sessionId":1,"path":file}),
        );
        let r = wait(&mut b);
        assert_eq!(r[0]["result"]["content"], "before\n");
        call(
            &mut b,
            &t,
            "propose",
            "xpeed_propose_change",
            json!({"sessionId":1,"path":file,"newContent":"after\n"}),
        );
        let r = wait(&mut b);
        let proposal = r[0]["result"]["proposalId"].clone();
        assert!(
            r[0]["result"]["diff"]
                .as_str()
                .unwrap()
                .contains("-before\n+after")
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before\n");
        call(
            &mut b,
            &t,
            "apply",
            "xpeed_apply_change",
            json!({"sessionId":1,"proposalId":proposal}),
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "before\n");
        b.approve(2, &t);
        wait(&mut b);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "after\n");
        call(
            &mut b,
            &t,
            "conflict",
            "xpeed_apply_change",
            json!({"sessionId":1,"proposalId":proposal}),
        );
        b.approve(3, &t);
        let r = wait(&mut b);
        assert_eq!(r[0]["result"]["state"], "failed");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "after\n");
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn command_results_are_real_bounded_and_timeout_cancellation_is_separate() {
        let (mut b, t, p) = setup();
        let command = if cfg!(windows) {
            "[Console]::Out.Write(('x' * 70000)); [Console]::Error.Write('failure'); exit 7"
        } else {
            "head -c 70000 /dev/zero | tr '\\0' x; echo failure >&2; exit 7"
        };
        call(
            &mut b,
            &t,
            "long",
            "xpeed_exec",
            json!({"sessionId":1,"cwd":p,"command":command,"timeoutMs":8000}),
        );
        b.approve(0, &t);
        wait(&mut b);
        let r = b.actions[0].result.as_ref().unwrap();
        assert_eq!(r["exitCode"], 7);
        assert_eq!(r["outputTruncated"], true);
        assert_eq!(r["stdout"].as_str().unwrap().len(), OUTPUT_LIMIT);
        assert!(r["stderr"].as_str().unwrap().contains("failure"));
        let command = if cfg!(windows) {
            "Start-Sleep -Seconds 20"
        } else {
            "sleep 20"
        };
        call(
            &mut b,
            &t,
            "timeout",
            "xpeed_exec",
            json!({"sessionId":1,"cwd":p,"command":command,"timeoutMs":100}),
        );
        b.approve(1, &t);
        wait(&mut b);
        assert_eq!(b.actions[1].state, "timedOut");
        call(
            &mut b,
            &t,
            "stop",
            "xpeed_exec",
            json!({"sessionId":1,"cwd":p,"command":command,"timeoutMs":30000}),
        );
        b.approve(2, &t);
        b.stop();
        wait(&mut b);
        assert!(matches!(
            b.actions[2].state.as_str(),
            "cancelled" | "failed"
        ));
        std::fs::remove_dir(p).unwrap();
    }
    #[test]
    fn disconnected_scope_and_other_session_task_cannot_gain_authority() {
        let (mut b, mut t, p) = setup();
        b.selected.insert(2);
        b.begin(&t).unwrap();
        call(
            &mut b,
            &t,
            "exec",
            "xpeed_exec",
            json!({"sessionId":1,"cwd":p,"command":"echo task","timeoutMs":3000}),
        );
        b.approve(0, &t);
        let task = b.take_replies()[0]["result"]["taskId"].clone();
        call(
            &mut b,
            &t,
            "wrong-session",
            "xpeed_task_status",
            json!({"sessionId":2,"taskId":task}),
        );
        assert!(b.take_replies()[0].get("error").is_some());
        t.get_mut(&1).unwrap().connected = false;
        b.revoke_disconnected(&t);
        call(
            &mut b,
            &t,
            "disconnected",
            "xpeed_read_file",
            json!({"sessionId":1,"path":p.join("file")}),
        );
        assert!(b.take_replies()[0].get("error").is_some());
        wait(&mut b);
        std::fs::remove_dir(p).unwrap();
    }
    #[test]
    #[ignore = "Started by scripts/test-session-tools.py with isolated localhost SSH servers"]
    fn two_real_ssh_transports() {
        let config = std::fs::read(std::env::var("XPEED_TOOL_TEST_CONFIG").unwrap()).unwrap();
        let config: Value = serde_json::from_slice(&config).unwrap();
        let mut targets = BTreeMap::new();
        for (index, item) in config.as_array().unwrap().iter().enumerate() {
            let mut t = target((index + 1) as u64, &std::env::temp_dir());
            t.cwd = "/test".into();
            t.ssh = Some(serde_json::from_value(item["connection"].clone()).unwrap());
            t.hosts = Some(PathBuf::from(item["knownHosts"].as_str().unwrap()));
            targets.insert(t.id, t);
        }
        let mut b = Bridge::default();
        b.selected.extend([1, 2]);
        b.begin(&targets).unwrap();
        for session in [1, 2] {
            call(
                &mut b,
                &targets,
                "exec",
                "xpeed_exec",
                json!({"sessionId":session,"cwd":"/test","command":"cat marker.txt; echo diagnostic >&2","timeoutMs":3000}),
            );
            b.approve(b.actions.len() - 1, &targets);
            wait(&mut b);
            let result = b.actions.last().unwrap().result.as_ref().unwrap();
            assert_eq!(result["exitCode"], 0, "{result}");
            assert!(
                result["stdout"]
                    .as_str()
                    .unwrap()
                    .contains(if session == 1 { "SERVER-A" } else { "SERVER-B" })
            );
            assert!(result["stderr"].as_str().unwrap().contains("diagnostic"));
        }
        call(
            &mut b,
            &targets,
            "list",
            "xpeed_list_directory",
            json!({"sessionId":1,"path":"/test"}),
        );
        let r = wait(&mut b);
        assert!(
            r[0]["result"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["name"] == "marker.txt")
        );
        call(
            &mut b,
            &targets,
            "propose",
            "xpeed_propose_change",
            json!({"sessionId":1,"path":"/test/edit.txt","newContent":"updated on A\n"}),
        );
        let proposal = wait(&mut b)[0]["result"]["proposalId"].clone();
        assert!(proposal.is_string());
        call(
            &mut b,
            &targets,
            "apply",
            "xpeed_apply_change",
            json!({"sessionId":1,"proposalId":proposal}),
        );
        b.approve(b.actions.len() - 1, &targets);
        let r = wait(&mut b);
        assert_eq!(r[0]["result"]["state"], "completed", "{r:?}");
        for session in [1, 2] {
            call(
                &mut b,
                &targets,
                "verify",
                "xpeed_read_file",
                json!({"sessionId":session,"path":"/test/edit.txt"}),
            );
            assert_eq!(
                wait(&mut b)[0]["result"]["content"],
                if session == 1 {
                    "updated on A\n"
                } else {
                    "original\n"
                }
            );
        }
        call(
            &mut b,
            &targets,
            "timeout",
            "xpeed_exec",
            json!({"sessionId":1,"cwd":"/test","command":"sleep 20","timeoutMs":200}),
        );
        b.approve(b.actions.len() - 1, &targets);
        wait(&mut b);
        assert_eq!(b.actions.last().unwrap().state, "timedOut");
        call(
            &mut b,
            &targets,
            "cancel",
            "xpeed_exec",
            json!({"sessionId":2,"cwd":"/test","command":"sleep 20","timeoutMs":3000}),
        );
        b.approve(b.actions.len() - 1, &targets);
        std::thread::sleep(Duration::from_millis(300));
        b.stop();
        wait(&mut b);
        assert_eq!(b.actions.last().unwrap().state, "cancelled");
        assert_eq!(
            b.actions.last().unwrap().result.as_ref().unwrap()["remoteMayBeRunning"],
            true
        );
    }
    #[test]
    fn metadata_and_file_tools_do_not_disclose_credentials() {
        let (mut b, mut t, p) = setup();
        t.get_mut(&1).unwrap().password = Some(Secret::new("test-secret".into()));
        assert!(!t[&1].public().to_string().contains("test-secret"));
        assert!(credential_path("C:/user/.ssh/id_ed25519"));
        assert!(credential_path("/app/.env.production"));
        call(
            &mut b,
            &t,
            "forged",
            "xpeed_apply_change",
            json!({"sessionId":1,"proposalId":"not-real","path":p.join("arbitrary"),"newContent":"bad"}),
        );
        assert!(b.take_replies()[0].get("error").is_some());
        std::fs::remove_dir(p).unwrap();
    }
}
