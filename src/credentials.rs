//! Secrets live in the OS vault or in memory, never in connection JSON or argv.
use crate::connections::SshConfig;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub type Secret = Zeroizing<String>;
fn entry(config: &SshConfig) -> Result<keyring::Entry, String> {
    keyring::Entry::new(
        "xpeed.ssh",
        &format!("{}:{}:{}", config.host, config.port, config.user),
    )
    .map_err(|e| e.to_string())
}
pub fn load(config: &SshConfig) -> Option<Secret> {
    entry(config).ok()?.get_password().ok().map(Zeroizing::new)
}
pub fn save(config: &SshConfig, password: &str) -> Result<(), String> {
    entry(config)?.set_password(password).map_err(|e| {
        xpeed_i18n::format(
            "No se pudo guardar el acceso en el almacén del sistema: {e}",
            &[("e", format!("{}", e))],
        )
    })
}
pub fn forget(config: &SshConfig) -> Result<(), String> {
    match entry(config)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn password_prompt(prompt: &str) -> bool {
    let prompt = prompt.trim().to_lowercase();
    prompt.ends_with("password:") && !prompt.contains("passphrase") && !prompt.contains("sudo")
}

/// OpenSSH's askpass helper receives the secret through an authenticated, local,
/// short-lived socket. It only answers password requests, never host-key trust.
pub struct Askpass {
    pub port: u16,
    pub token: String,
    stop: Arc<AtomicBool>,
}
impl Askpass {
    pub fn new(password: Secret) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let token = rand::random::<[u8; 32]>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let expected = token.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            while !stopped.load(Ordering::Acquire) && started.elapsed() < Duration::from_secs(20) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                    let mut request = String::new();
                    if (&mut stream)
                        .take(4096)
                        .read_to_string(&mut request)
                        .is_ok()
                        && let Some((token, prompt)) = request.split_once('\n')
                        && token == expected
                        && password_prompt(prompt)
                    {
                        let _ = stream.write_all(password.as_bytes());
                        break;
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        });
        Ok(Self { port, token, stop })
    }
}
impl Drop for Askpass {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}
pub fn helper() -> Option<i32> {
    let port = std::env::var("XPEED_ASKPASS_PORT").ok()?;
    let result = (|| -> Result<(), String> {
        let token = std::env::var("XPEED_ASKPASS_TOKEN").map_err(|e| e.to_string())?;
        let prompt = std::env::args().nth(1).unwrap_or_default();
        if !password_prompt(&prompt) {
            return Err("Unsupported prompt".into());
        }
        let port = port.parse::<u16>().map_err(|e| e.to_string())?;
        let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        stream
            .write_all(format!("{token}\n{prompt}").as_bytes())
            .map_err(|e| e.to_string())?;
        stream
            .shutdown(std::net::Shutdown::Write)
            .map_err(|e| e.to_string())?;
        let mut secret = Secret::new(String::new());
        stream
            .take(4096)
            .read_to_string(&mut secret)
            .map_err(|e| e.to_string())?;
        if secret.is_empty() {
            return Err("No credential".into());
        }
        let mut output = std::io::stdout().lock();
        output
            .write_all(secret.as_bytes())
            .and_then(|_| output.write_all(b"\n"))
            .and_then(|_| output.flush())
            .map_err(|e| e.to_string())
    })();
    Some(if result.is_ok() { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn windows_vault_roundtrip_is_separate_from_profile_json() {
        let config = SshConfig {
            host: format!("xpeed-test-{}.invalid", rand::random::<u64>()),
            user: "test-only".into(),
            ..Default::default()
        };
        save(&config, "test-only-secret-ñ").unwrap();
        let loaded = load(&config);
        let removed = forget(&config);
        assert_eq!(
            loaded.as_deref().map(|s| s.as_str()),
            Some("test-only-secret-ñ")
        );
        removed.unwrap();
        assert!(load(&config).is_none());
        assert!(!serde_json::to_string(&config).unwrap().contains("secret"));
    }
    #[test]
    fn broker_rejects_wrong_token_and_host_trust_and_serves_password() {
        let broker = Askpass::new(Secret::new("test-only-password".into())).unwrap();
        for request in [
            "bad\nuser@host's password:".to_string(),
            format!("{}\nContinue connecting (yes/no)?", broker.token),
        ] {
            let mut client = TcpStream::connect(("127.0.0.1", broker.port)).unwrap();
            client.write_all(request.as_bytes()).unwrap();
            client.shutdown(std::net::Shutdown::Write).unwrap();
            let mut answer = String::new();
            client.read_to_string(&mut answer).unwrap();
            assert!(answer.is_empty());
        }
        let mut client = TcpStream::connect(("127.0.0.1", broker.port)).unwrap();
        client
            .write_all(format!("{}\nuser@host's password:", broker.token).as_bytes())
            .unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let mut answer = String::new();
        client.read_to_string(&mut answer).unwrap();
        assert_eq!(answer, "test-only-password");
    }
}
