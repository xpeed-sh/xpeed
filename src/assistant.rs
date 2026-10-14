use eframe::egui::{self, RichText};
use serde::{Deserialize, Serialize};
use std::{
    sync::mpsc::{self, Receiver},
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}
pub struct Assistant {
    pub messages: Vec<Message>,
    pub draft: String,
    endpoint: String,
    model: String,
    key: String,
    ollama: bool,
    configuring: bool,
    pending: Option<Receiver<Result<String, String>>>,
    pub error: Option<String>,
}
impl Default for Assistant {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            draft: String::new(),
            endpoint: "http://127.0.0.1:11434/api/chat".into(),
            model: "qwen2.5-coder:7b".into(),
            key: String::new(),
            ollama: true,
            configuring: true,
            pending: None,
            error: None,
        }
    }
}
impl Assistant {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if let Some(result) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            match result {
                Ok(content) => self.messages.push(Message {
                    role: "assistant".into(),
                    content,
                }),
                Err(error) => self.error = Some(error),
            }
            self.pending = None;
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(xpeed_i18n::t("Asistente")).strong());
            if ui.small_button(xpeed_i18n::t("Configurar")).clicked() {
                self.configuring = !self.configuring;
            }
        });
        if self.configuring {
            ui.checkbox(&mut self.ollama, xpeed_i18n::t("Ollama local"));
            ui.label(xpeed_i18n::t("URL completa del chat"));
            ui.add(egui::TextEdit::singleline(&mut self.endpoint).desired_width(f32::INFINITY));
            ui.label(xpeed_i18n::t("Modelo instalado / disponible"));
            ui.add(egui::TextEdit::singleline(&mut self.model).desired_width(f32::INFINITY));
            if !self.ollama {
                ui.label(xpeed_i18n::t("Clave API (solo durante esta ejecución)"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.key)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
            }
            ui.label(RichText::new(xpeed_i18n::t("Para otras APIs: desmarca Ollama e indica una URL compatible con chat/completions.")).small());
            ui.separator();
        }
        if self.messages.is_empty() {
            ui.label(xpeed_i18n::t(
                "Consulta comandos, explica un error o prepara un script.",
            ));
            ui.label(
                RichText::new(xpeed_i18n::t(
                    "Solo se envían los mensajes que escribas aquí. La IA no ejecuta comandos.",
                ))
                .small(),
            );
        }
        let height = (ui.available_height() - 120.0).max(45.0);
        egui::ScrollArea::vertical()
            .id_salt("ai_chat")
            .min_scrolled_height(height)
            .max_height(height)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_height(height);
                for message in &self.messages {
                    ui.label(
                        RichText::new(if message.role == "user" {
                            xpeed_i18n::t("Tú")
                        } else {
                            xpeed_i18n::t("Asistente")
                        })
                        .strong(),
                    );
                    ui.add(egui::Label::new(&message.content).wrap());
                    if message.role == "assistant"
                        && ui.small_button(xpeed_i18n::t("Copiar respuesta")).clicked()
                    {
                        ui.ctx().copy_text(message.content.clone());
                    }
                    ui.add_space(10.0);
                }
            });
        if let Some(error) = &self.error {
            ui.add(egui::Label::new(error).wrap());
        }
        if self.pending.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(xpeed_i18n::t("Esperando respuesta…"));
            });
        }
        ui.add(
            egui::TextEdit::multiline(&mut self.draft)
                .desired_width(f32::INFINITY)
                .desired_rows(3)
                .hint_text(xpeed_i18n::t("Escribe tu consulta…")),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.pending.is_none() && !self.draft.trim().is_empty(),
                    egui::Button::new(xpeed_i18n::t("Enviar")),
                )
                .clicked()
            {
                self.send(ui.ctx());
            }
            if ui
                .add_enabled(
                    self.pending.is_none(),
                    egui::Button::new(xpeed_i18n::t("Limpiar chat")),
                )
                .clicked()
            {
                self.messages.clear();
                self.error = None;
            }
        });
    }
    fn send(&mut self, ctx: &egui::Context) {
        let endpoint = self.endpoint.trim().to_string();
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://"))
            || self.model.trim().is_empty()
        {
            self.error = Some(xpeed_i18n::t("Introduce una URL HTTP(S) y un modelo.").into());
            return;
        }
        let mut messages = self.messages.clone();
        messages.insert(0, Message { role: "system".into(), content: xpeed_i18n::t("Eres el asistente de Xpeed. Responde en español con comandos precisos y explica su efecto. No puedes ejecutar comandos ni acceder a sesiones o archivos.").into() });
        let user = Message {
            role: "user".into(),
            content: std::mem::take(&mut self.draft),
        };
        messages.push(user.clone());
        self.messages.push(user);
        self.error = None;
        let payload = serde_json::json!({ "model": self.model.trim(), "messages": messages, "stream": false });
        let key = self.key.clone();
        let ollama = self.ollama;
        let ctx = ctx.clone();
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let result = (|| {
                let client = reqwest::blocking::Client::builder()
                    .timeout(Duration::from_secs(120))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|e| e.to_string())?;
                let mut request = client.post(endpoint).json(&payload);
                if !key.is_empty() {
                    request = request.bearer_auth(key);
                }
                let response = request.send().map_err(|e| {
                    xpeed_i18n::format(
                        "No se pudo conectar al proveedor: {e}",
                        &[("e", format!("{}", e))],
                    )
                })?;
                if !response.status().is_success() {
                    return Err(xpeed_i18n::format(
                        "El proveedor respondió con HTTP {}. Revisa la URL, el modelo y la clave.",
                        &[("0", format!("{}", response.status()))],
                    ));
                }
                let json: serde_json::Value = response.json().map_err(|e| e.to_string())?;
                extract_response(&json, ollama)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
}
fn extract_response(json: &serde_json::Value, ollama: bool) -> Result<String, String> {
    let content = if ollama {
        json.pointer("/message/content")
    } else {
        json.pointer("/choices/0/message/content")
    };
    content
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| xpeed_i18n::t("El proveedor no devolvió texto de chat.").into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chat_sends_the_typed_message_and_receives_a_response_over_http() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            time::Instant,
        };
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        server.set_nonblocking(true).unwrap();
        let endpoint = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                if let Ok((mut socket, _)) = server.accept() {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 2048];
                    loop {
                        let count = socket.read(&mut buffer).unwrap();
                        assert!(count > 0);
                        request.extend(&buffer[..count]);
                        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                            let length: usize = headers
                                .lines()
                                .find_map(|l| {
                                    l.strip_prefix("content-length:")
                                        .map(|n| n.trim().parse().unwrap())
                                })
                                .unwrap();
                            if request.len() >= end + 4 + length {
                                let payload: serde_json::Value =
                                    serde_json::from_slice(&request[end + 4..end + 4 + length])
                                        .unwrap();
                                assert_eq!(payload["messages"][1]["content"], "explica pwd");
                                assert_eq!(payload["stream"], false);
                                let body =
                                    r#"{"message":{"content":"pwd muestra el directorio actual"}}"#;
                                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                                return;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("No llegó la petición HTTP");
        });
        let mut assistant = Assistant {
            endpoint: format!("http://127.0.0.1:{port}/api/chat"),
            draft: "explica pwd".into(),
            ..Default::default()
        };
        assistant.send(&egui::Context::default());
        assert_eq!(
            assistant
                .pending
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(8))
                .unwrap()
                .unwrap(),
            "pwd muestra el directorio actual"
        );
        endpoint.join().unwrap();
        assert!(assistant.draft.is_empty());
    }
    #[test]
    fn handles_both_provider_formats() {
        assert_eq!(
            extract_response(&serde_json::json!({"message":{"content":"hola"}}), true).unwrap(),
            "hola"
        );
        assert_eq!(
            extract_response(
                &serde_json::json!({"choices":[{"message":{"content":"ok"}}]}),
                false
            )
            .unwrap(),
            "ok"
        );
        assert!(extract_response(&serde_json::json!({"error":"bad"}), false).is_err());
    }
}
