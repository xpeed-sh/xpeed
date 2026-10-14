//! Optional provider adapter; the Xpeed executable contains no Codex protocol code.
use serde_json::{Value, json};
mod session_tools;
use std::{
    collections::BTreeMap,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
};

enum Input {
    Host(Value),
    Codex(Value),
    Closed(&'static str),
    Invalid(&'static str),
}
enum Pending {
    Initialize(Value),
    Relay(Value, String),
    Thread(Value, Value),
    Turn(Value),
}
struct Server {
    child: Child,
    input: ChildStdin,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Adapter {
    scope: Value,
    tool_calls: BTreeMap<String, Value>,
    runtime_cwd: PathBuf,
    server: Option<Server>,
    tx: mpsc::SyncSender<Input>,
    next: u64,
    pending: BTreeMap<u64, Pending>,
    thread: Option<String>,
    turn: Option<String>,
    approvals: BTreeMap<String, (Value, String, Value)>,
}
fn emit(value: Value) {
    let mut out = io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, &value);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}
fn response(id: Value, result: Value) {
    emit(json!({"jsonrpc":"2.0","id":id,"result":result}));
}
fn error(id: Value, message: &str) {
    emit(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":xpeed_i18n::t(message)}}));
}
fn event(method: &str, params: Value) {
    emit(json!({"jsonrpc":"2.0","method":method,"params":params}));
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_owned()
}
fn reader<R: io::Read + Send + 'static>(input: R, tx: mpsc::SyncSender<Input>, host: bool) {
    std::thread::spawn(move || {
        let mut input = BufReader::new(input);
        loop {
            let mut bytes = Vec::new();
            let mut invalid = false;
            loop {
                let Ok(buffer) = input.fill_buf() else {
                    invalid = true;
                    break;
                };
                if buffer.is_empty() {
                    let _ = tx.send(Input::Closed(if host { "host" } else { "codex" }));
                    return;
                }
                let end = buffer.iter().position(|b| *b == b'\n');
                let count = end.map_or(buffer.len(), |i| i + 1);
                if bytes.len() + count > 2 * 1024 * 1024 {
                    invalid = true;
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
                input.consume(count);
                if end.is_some() {
                    break;
                }
            }
            if invalid {
                let _ = tx.send(Input::Invalid(if host { "host" } else { "codex" }));
                return;
            }
            let Ok(value) = serde_json::from_slice(&bytes) else {
                let _ = tx.send(Input::Invalid(if host { "host" } else { "codex" }));
                return;
            };
            if tx
                .send(if host {
                    Input::Host(value)
                } else {
                    Input::Codex(value)
                })
                .is_err()
            {
                return;
            }
        }
    });
}
fn discover(configured: &str) -> Result<PathBuf, String> {
    if !configured.trim().is_empty() {
        let p = PathBuf::from(configured.trim());
        return if p.is_file() {
            Ok(p)
        } else {
            Err(xpeed_i18n::t("La ruta de Codex no apunta a un ejecutable.").into())
        };
    }
    let executable = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(executable);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or(
        xpeed_i18n::t("No se encontró Codex. Instala Codex CLI o indica su ejecutable."),
    )?;
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x86_64",
        ("windows", "aarch64") => "windows-aarch64",
        ("macos", "aarch64") => "darwin-aarch64",
        ("macos", _) => "darwin-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        _ => "linux-x86_64",
    };
    for editor in [".vscode", ".vscode-insiders"] {
        let Ok(entries) = std::fs::read_dir(Path::new(&home).join(editor).join("extensions"))
        else {
            continue;
        };
        let mut entries: Vec<_> = entries
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("openai.chatgpt-")
            })
            .collect();
        entries.sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));
        for entry in entries {
            let p = entry.path().join("bin").join(platform).join(executable);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    Err(xpeed_i18n::t("No se encontró Codex. Instala Codex CLI o la extensión oficial en VS Code; también puedes indicar la ruta de codex.exe.").into())
}
impl Adapter {
    fn start(&mut self, path: &str) -> Result<(), String> {
        if self.server.is_some() {
            return Ok(());
        }
        let path = discover(path)?;
        let mut version_command = Command::new(&path);
        version_command.arg("--version");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            version_command.creation_flags(0x08000000);
        }
        let version = version_command.output().map_err(|e| e.to_string())?;
        let version = String::from_utf8_lossy(&version.stdout);
        if !session_tools::supports_runtime(&version) {
            return Err(xpeed_i18n::format(
                "Codex requires Codex {version} for scoped session tools. Detected: {detected}. Configure its executable in Extensions.",
                &[
                    ("version", session_tools::SUPPORTED_RUNTIMES.join(" / ")),
                    ("detected", version.trim().into()),
                ],
            ));
        }
        // Separate official credential/config storage: no inherited MCP servers,
        // plugins, hooks, or credentials from other Codex installations.
        let data = std::env::var_os(if cfg!(windows) {
            "APPDATA"
        } else {
            "XDG_DATA_HOME"
        })
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|home| {
                PathBuf::from(home).join(if cfg!(windows) {
                    "AppData/Roaming"
                } else {
                    ".local/share"
                })
            })
        })
        .ok_or_else(|| {
            xpeed_i18n::t("Cannot find a private user directory for Codex sign-in.").to_owned()
        })?;
        let home = data.join("xpeed/providers/codex-home");
        self.runtime_cwd = home.join("empty-workspace");
        std::fs::create_dir_all(&self.runtime_cwd).map_err(|e| e.to_string())?;
        let mut command = Command::new(path);
        command
            .args(["app-server", "--listen", "stdio://"])
            .args([
                "-c",
                "features.shell_tool=false",
                "-c",
                "features.unified_exec=false",
                "-c",
                "features.view_image=false",
                "-c",
                "features.hooks=false",
                "-c",
                "features.plugins=false",
                "-c",
                "features.apps=false",
                "-c",
                "features.multi_agent=false",
                "-c",
                "features.browser_use=false",
                "-c",
                "features.in_app_browser=false",
                "-c",
                "features.code_mode_host=true",
                "-c",
                "web_search=\"disabled\"",
                "-c",
                "forced_login_method=\"chatgpt\"",
            ])
            .env("CODEX_HOME", home)
            .env_remove("OPENAI_API_KEY")
            .env_remove("CODEX_API_KEY")
            .current_dir(&self.runtime_cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| {
            xpeed_i18n::format("No se pudo iniciar Codex: {e}", &[("e", format!("{}", e))])
        })?;
        let input = child
            .stdin
            .take()
            .ok_or(xpeed_i18n::t("Codex sin entrada"))?;
        reader(
            child
                .stdout
                .take()
                .ok_or(xpeed_i18n::t("Codex sin salida"))?,
            self.tx.clone(),
            false,
        );
        self.server = Some(Server { child, input });
        Ok(())
    }
    fn send(&mut self, message: Value) -> Result<(), String> {
        let server = self
            .server
            .as_mut()
            .ok_or(xpeed_i18n::t("Codex no está conectado"))?;
        serde_json::to_writer(&mut server.input, &message).map_err(|e| e.to_string())?;
        server
            .input
            .write_all(b"\n")
            .and_then(|_| server.input.flush())
            .map_err(|e| e.to_string())
    }
    fn call(&mut self, method: &str, params: Value, pending: Pending) -> Result<(), String> {
        self.next += 1;
        let id = self.next;
        self.send(json!({"id":id,"method":method,"params":params}))?;
        self.pending.insert(id, pending);
        Ok(())
    }
    fn host(&mut self, v: Value) -> Result<bool, String> {
        if let Some(key) = v["id"].as_str() {
            if let Some(provider_id) = self.tool_calls.remove(key) {
                let success = v.get("error").is_none() && v["result"]["state"] != "failed";
                let value = if success {
                    v["result"].clone()
                } else {
                    json!({"error":v.get("error").cloned().unwrap_or_else(||v["result"].clone())})
                };
                self.send(json!({"id":provider_id,"result":{"success":success,"contentItems":[{"type":"inputText","text":serde_json::to_string(&value).map_err(|e|e.to_string())?}]}}))?;
            }
            return Ok(false);
        }
        let id = v["id"].clone();
        let params = v["params"].clone();
        let method = v["method"].as_str().unwrap_or_default();
        match method {
            "initialize" => {
                xpeed_i18n::set_language(xpeed_i18n::Language::from_code(
                    params["language"].as_str().unwrap_or("en"),
                ));
                if params["protocolVersion"] != 1 {
                    error(id, xpeed_i18n::t("Versión de protocolo incompatible"));
                    return Ok(false);
                }
                self.start(params["providerPath"].as_str().unwrap_or_default())?;
                self.call("initialize",json!({"clientInfo":{"name":"xpeed_codex","title":"Codex","version":params["clientVersion"]},"capabilities":{"experimentalApi":true}}),Pending::Initialize(id))?;
            }
            "locale/change" => {
                xpeed_i18n::set_language(xpeed_i18n::Language::from_code(
                    params["language"].as_str().unwrap_or("en"),
                ));
                if !id.is_null() {
                    response(id, Value::Null);
                }
            }
            "account/read" => self.call(
                "account/read",
                json!({"refreshToken":false}),
                Pending::Relay(id, method.into()),
            )?,
            "account/login" => self.call(
                "account/login/start",
                json!({"type":"chatgpt"}),
                Pending::Relay(id, method.into()),
            )?,
            "account/login/cancel" => self.call(
                "account/login/cancel",
                params,
                Pending::Relay(id, method.into()),
            )?,
            "account/logout" => {
                self.tool_calls.clear();
                self.thread = None;
                self.scope = Value::Null;
                self.call(
                    "account/logout",
                    json!({}),
                    Pending::Relay(id, method.into()),
                )?;
            }
            "models/list" => {
                self.call("model/list", json!({}), Pending::Relay(id, method.into()))?
            }
            "account/limits" => self.call(
                "account/rateLimits/read",
                json!({}),
                Pending::Relay(id, method.into()),
            )?,
            "conversation/reset" => {
                if self.turn.is_some() {
                    error(
                        id,
                        xpeed_i18n::t("Detén el turno antes de abrir otra conversación"),
                    );
                } else {
                    self.thread = None;
                    self.scope = Value::Null;
                    self.tool_calls.clear();
                    response(id, json!({}));
                }
            }
            "chat/start" => {
                if self.turn.is_some()
                    || self
                        .pending
                        .values()
                        .any(|p| matches!(p, Pending::Thread(..) | Pending::Turn(..)))
                {
                    error(id, xpeed_i18n::t("Ya hay un turno en curso"));
                    return Ok(false);
                }
                if text(&params, "text").trim().is_empty() {
                    error(id, xpeed_i18n::t("Escribe un mensaje"));
                    return Ok(false);
                }
                if params["scope"]["scopeToken"].as_str().is_none()
                    || !params["scope"]["sessions"]
                        .as_array()
                        .is_some_and(|s| !s.is_empty())
                {
                    return Err(
                        "Select an authorized Xpeed session before sending a message.".into(),
                    );
                }
                let requested = params["conversationId"].as_str().map(str::to_owned);
                let scope_changed = self.scope["scopeToken"] != params["scope"]["scopeToken"];
                let resume = requested.is_some() && (requested != self.thread || scope_changed);
                if params.get("conversationId").is_some() {
                    self.thread = requested.clone();
                }
                self.scope = params["scope"].clone();
                if self.thread.is_none() || resume {
                    let mut start = session_tools::thread_params(
                        &self.runtime_cwd.to_string_lossy(),
                        &self.scope,
                    );
                    if !text(&params, "model").is_empty() {
                        start["model"] = params["model"].clone();
                    }
                    let method = if resume {
                        start["threadId"] = json!(requested);
                        for key in ["dynamicTools", "environments", "serviceName"] {
                            start.as_object_mut().unwrap().remove(key);
                        }
                        "thread/resume"
                    } else {
                        "thread/start"
                    };
                    self.call(method, start, Pending::Thread(id, params))?;
                } else {
                    self.start_turn(id, params)?;
                }
            }
            "chat/interrupt" => {
                if let (Some(thread), Some(turn)) = (&self.thread, &self.turn) {
                    self.call(
                        "turn/interrupt",
                        json!({"threadId":thread,"turnId":turn}),
                        Pending::Relay(id, method.into()),
                    )?;
                } else {
                    response(id, json!({}));
                }
            }
            "approval/respond" => {
                let key = text(&params, "requestId");
                let Some((request, kind, original)) = self.approvals.remove(&key) else {
                    error(id, xpeed_i18n::t("Solicitud de aprobación caducada"));
                    return Ok(false);
                };
                let accept = params["decision"] == "accept";
                let result = approval_result(&kind, accept, &original);
                self.send(json!({"id":request,"result":result}))?;
                response(id, json!({}));
            }
            "input/respond" => {
                let key = text(&params, "requestId");
                let Some((request, _, _)) = self.approvals.remove(&key) else {
                    error(id, xpeed_i18n::t("Pregunta caducada"));
                    return Ok(false);
                };
                self.send(json!({"id":request,"result":{"answers":params["answers"]}}))?;
                response(id, json!({}));
            }
            "shutdown" => {
                self.server.take();
                response(id, json!({}));
                return Ok(true);
            }
            _ => error(id, xpeed_i18n::t("Método de extensión no soportado")),
        }
        Ok(false)
    }
    fn start_turn(&mut self, id: Value, params: Value) -> Result<(), String> {
        let mut request = json!({"threadId":self.thread,"input":[{"type":"text","text":params["text"]}],"approvalPolicy":"never","approvalsReviewer":"user"});
        if !text(&params, "model").is_empty() {
            request["model"] = params["model"].clone();
        }
        self.call("turn/start", request, Pending::Turn(id))
    }
    fn provider(&mut self, v: Value) -> Result<(), String> {
        if let Some(method) = v["method"].as_str() {
            let p = v["params"].clone();
            if !v["id"].is_null() {
                let request = v["id"].clone();
                let key = request.to_string();
                match method{
                    "item/tool/call" => {
                        if p["threadId"].as_str() != self.thread.as_deref() || p["turnId"].as_str() != self.turn.as_deref() { self.send(json!({"id":request,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Expired conversation or turn."}]}}))?; }
                        else {
                            let key=format!("xpeed-tool-{}",p["callId"].as_str().unwrap_or_default());
                            self.tool_calls.insert(key.clone(),request);
                            emit(json!({"jsonrpc":"2.0","id":key,"method":"tools/call","params":{"tool":p["tool"],"arguments":p["arguments"],"scopeToken":self.scope["scopeToken"]}}));
                        }
                    }
                    "item/commandExecution/requestApproval"|"item/fileChange/requestApproval"|"item/permissions/requestApproval"=>{
                        self.send(json!({"id":request,"result":approval_result(method,false,&p)}))?;
                        event("status",json!({"text":"Native local tool approval denied: use Xpeed session tools."}));
                    }
                    "item/tool/requestUserInput"=>{self.approvals.insert(key.clone(),(request,method.into(),p.clone()));event("input/request",json!({"requestId":key,"questions":p["questions"]}));}
                    "mcpServer/elicitation/request"=>{self.send(json!({"id":request,"result":{"action":"decline","content":null}}))?;event("status",json!({"text":xpeed_i18n::t("Esta versión no admite formularios de MCP; la solicitud se rechazó.")}));}
                    _=>self.send(json!({"id":request,"error":{"code":-32601,"message":"Xpeed does not support this server request"}}))?,
                }
                return Ok(());
            }
            match method {
                "account/updated" | "account/login/completed" => {
                    if method == "account/login/completed" && p["success"] == false {
                        event("error", json!({"message":p["error"]}));
                    }
                    self.call(
                        "account/read",
                        json!({"refreshToken":false}),
                        Pending::Relay(Value::Null, "account/read".into()),
                    )?;
                    event("account/login/completed", p);
                }
                "item/agentMessage/delta" => event(
                    "chat/delta",
                    json!({"messageId":p["itemId"],"delta":p["delta"]}),
                ),
                "account/rateLimits/updated" => event("account/limits", p),
                "item/completed" => {
                    let item = &p["item"];
                    match item["type"].as_str() {
                        Some("agentMessage") => event(
                            "chat/message",
                            json!({"messageId":item["id"],"role":"assistant","text":item["text"]}),
                        ),
                        Some("commandExecution") => event(
                            "status",
                            json!({"text":format!("{} · {}",text(item,"command"),text(item,"status"))}),
                        ),
                        Some("fileChange") => event(
                            "status",
                            json!({"text":xpeed_i18n::format("Cambios en archivos · {}", &[("0", text(item,"status").to_string())])}),
                        ),
                        _ => {}
                    }
                }
                "turn/started" => {
                    self.turn = p["turn"]["id"].as_str().map(str::to_owned);
                }
                "turn/completed" => {
                    for (_, request) in std::mem::take(&mut self.tool_calls) {
                        self.send(json!({"id":request,"result":{"success":false,"contentItems":[{"type":"inputText","text":"Turn ended."}]}}))?;
                    }
                    self.turn = None;
                    self.approvals.clear();
                    event(
                        "chat/completed",
                        json!({"status":p["turn"]["status"],"error":p["turn"]["error"]}),
                    );
                }
                "serverRequest/resolved" => {
                    let key = p["requestId"].to_string();
                    self.approvals.remove(&key);
                    event("approval/resolved", json!({"requestId":key}));
                }
                "error" => event("error", json!({"message":p["error"]["message"]})),
                _ => {}
            }
            return Ok(());
        }
        let Some(id) = v["id"].as_u64() else {
            return Ok(());
        };
        let Some(pending) = self.pending.remove(&id) else {
            return Ok(());
        };
        if let Some(e) = v.get("error") {
            let host = match pending {
                Pending::Initialize(id)
                | Pending::Relay(id, _)
                | Pending::Thread(id, _)
                | Pending::Turn(id) => id,
            };
            if host.is_null() {
                event("error", json!({"message":e["message"]}));
            } else {
                error(
                    host,
                    e["message"]
                        .as_str()
                        .unwrap_or(xpeed_i18n::t("Error de Codex")),
                );
            }
            return Ok(());
        }
        let result = v["result"].clone();
        match pending {
            Pending::Initialize(host) => {
                self.send(json!({"method":"initialized"}))?;
                response(
                    host,
                    json!({"protocolVersion":1,"name":"Codex","capabilities":["chat","login","models","approvals"]}),
                );
            }
            Pending::Relay(host, method) => {
                let result = if method == "account/read" {
                    json!({"account":result["account"],"requiresAuth":result["requiresOpenaiAuth"]})
                } else if method == "models/list" {
                    json!({"models":result["data"].as_array().map(|models|models.iter().filter(|m|m["hidden"]!=true).map(|m|json!({"id":m["model"],"name":m["displayName"]})).collect::<Vec<_>>()).unwrap_or_default()})
                } else {
                    result
                };
                if host.is_null() {
                    event("account/updated", result);
                } else {
                    response(host, result);
                }
            }
            Pending::Thread(host, params) => {
                self.thread = result["thread"]["id"].as_str().map(str::to_owned);
                if self.thread.is_none() {
                    error(host, xpeed_i18n::t("Codex no devolvió una conversación"));
                } else {
                    event(
                        "conversation/updated",
                        json!({"conversationId":self.thread}),
                    );
                    self.start_turn(host, params)?;
                }
            }
            Pending::Turn(host) => {
                self.turn = result["turn"]["id"].as_str().map(str::to_owned);
                response(
                    host,
                    json!({"conversationId":self.thread,"turnId":self.turn}),
                );
            }
        }
        Ok(())
    }
}
fn approval_result(kind: &str, accept: bool, params: &Value) -> Value {
    if kind == "item/permissions/requestApproval" {
        json!({"permissions":if accept{params["permissions"].clone()}else{json!({})},"scope":"turn"})
    } else {
        json!({"decision":if accept{"accept"}else{"decline"}})
    }
}
fn main() {
    let (tx, rx) = mpsc::sync_channel(256);
    reader(io::stdin(), tx.clone(), true);
    let mut adapter = Adapter {
        scope: Value::Null,
        tool_calls: BTreeMap::new(),
        runtime_cwd: PathBuf::new(),
        server: None,
        tx,
        next: 0,
        pending: BTreeMap::new(),
        thread: None,
        turn: None,
        approvals: BTreeMap::new(),
    };
    while let Ok(input) = rx.recv() {
        match input {
            Input::Host(v) => {
                let id = v["id"].clone();
                match adapter.host(v) {
                    Ok(true) => break,
                    Ok(false) => {}
                    Err(e) => error(id, &e),
                }
            }
            Input::Codex(v) => {
                if let Err(e) = adapter.provider(v) {
                    event("error", json!({"message":e}));
                    break;
                }
            }
            Input::Closed("host") => break,
            Input::Closed(_) => {
                event(
                    "error",
                    json!({"message":xpeed_i18n::t("El proceso de Codex terminó. Vuelve a conectar la extensión.")}),
                );
                break;
            }
            Input::Invalid(source) => {
                event(
                    "error",
                    json!({"message":xpeed_i18n::format("JSON inválido o demasiado grande desde {source}", &[("source", source.to_string())])}),
                );
                break;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approvals_never_expand_the_requested_permissions() {
        let p = json!({"permissions":{"network":{"enabled":true}}});
        assert_eq!(
            approval_result("item/permissions/requestApproval", false, &p)["permissions"],
            json!({})
        );
        assert_eq!(
            approval_result("item/permissions/requestApproval", true, &p)["permissions"],
            p["permissions"]
        );
        assert_eq!(
            approval_result("item/fileChange/requestApproval", false, &p)["decision"],
            "decline"
        );
    }
    #[test]
    fn missing_configured_binary_is_not_replaced_with_another_provider() {
        assert!(discover("C:/nonexistent/xpeed-test/codex.exe").is_err());
    }
}
