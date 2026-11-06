use crate::terminal::Profile;
use portable_pty::CommandBuilder;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub identity: String,
}
impl Default for SshConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 22,
            user: String::new(),
            identity: String::new(),
        }
    }
}
impl SshConfig {
    fn clean(value: &str) -> &str {
        value.trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '\u{200B}' | '\u{FEFF}')
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let host = Self::clean(&self.host);
        let user = Self::clean(&self.user);
        let identity = Self::clean(&self.identity);
        if host.is_empty()
            || host.starts_with('-')
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".:_-".contains(c))
        {
            return Err(xpeed_i18n::t(
                "Introduce un host, una dirección IP o un alias SSH válido.",
            )
            .into());
        }
        if self.port == 0 {
            return Err(xpeed_i18n::t("El puerto debe estar entre 1 y 65535.").into());
        }
        if user.starts_with('-')
            || !user
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        {
            return Err(
                xpeed_i18n::t("El nombre de usuario contiene caracteres no válidos.").into(),
            );
        }
        if !identity.is_empty() && !PathBuf::from(identity).is_file() {
            return Err(xpeed_i18n::t("No se encontró el archivo de clave privada.").into());
        }
        Ok(())
    }
    pub fn label(&self) -> String {
        let host = Self::clean(&self.host);
        let user = Self::clean(&self.user);
        if user.is_empty() {
            host.into()
        } else {
            format!("{user}@{host}")
        }
    }
    pub fn known_host_target(&self) -> String {
        let host = Self::clean(&self.host);
        if self.port == 22 {
            host.into()
        } else {
            format!("[{host}]:{}", self.port)
        }
    }
    pub fn forget_known_host(&self) -> Result<(), String> {
        self.validate()?;
        let mut command = std::process::Command::new("ssh-keygen");
        command.args(["-R", &self.known_host_target()]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let output = command.output().map_err(|error| {
            xpeed_i18n::format(
                "No se pudo iniciar ssh-keygen: {error}",
                &[("error", error.to_string())],
            )
        })?;
        if output.status.success() {
            Ok(())
        } else {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(if detail.is_empty() {
                xpeed_i18n::t("No se pudo quitar la identidad anterior del servidor.").into()
            } else {
                detail
            })
        }
    }
    pub fn command(&self) -> Result<CommandBuilder, String> {
        self.command_with_hosts(None)
    }
    pub fn command_with_hosts(
        &self,
        hosts: Option<&std::path::Path>,
    ) -> Result<CommandBuilder, String> {
        self.validate()?;
        let mut command = CommandBuilder::new("ssh");
        if let Some(hosts) = hosts {
            command.args([
                "-o",
                &format!("UserKnownHostsFile={}", hosts.display()),
                "-o",
                "StrictHostKeyChecking=yes",
            ]);
        }
        command.args([
            "-t",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=4",
            "-p",
            &self.port.to_string(),
        ]);
        let user = Self::clean(&self.user);
        let identity = Self::clean(&self.identity);
        if !user.is_empty() {
            command.args(["-l", user]);
        }
        if !identity.is_empty() {
            command.args(["-i", identity]);
        }
        command.args(["--", Self::clean(&self.host)]);
        Ok(command)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Target {
    Local { shell: Profile, directory: String },
    Ssh(SshConfig),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    pub target: Target,
}

impl Connection {
    pub fn description(&self) -> String {
        match &self.target {
            Target::Local { shell, directory } => {
                if directory.is_empty() {
                    shell.name().into()
                } else {
                    directory.clone()
                }
            }
            Target::Ssh(config) => format!("{}:{}", config.label(), config.port),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err(xpeed_i18n::t("Dale un nombre a la conexión.").into());
        }
        match &self.target {
            Target::Ssh(config) => config.validate(),
            Target::Local { directory, .. } => {
                if !directory.is_empty() && !PathBuf::from(directory).is_dir() {
                    return Err(xpeed_i18n::t("El directorio local no existe.").into());
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ConnectionStore {
    pub saved: Vec<Connection>,
    pub recent: Vec<Connection>,
    pub groups: std::collections::BTreeMap<String, String>,
}
impl ConnectionStore {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("xpeed").join("connections.json"))
    }
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let path =
            Self::path().ok_or(xpeed_i18n::t("No se encontró la carpeta de configuración"))?;
        std::fs::create_dir_all(path.parent().ok_or(xpeed_i18n::t("Ruta inválida"))?)
            .map_err(|e| e.to_string())?;
        let temporary = path.with_extension("tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_validates_hosts_and_does_not_accept_options_as_destinations() {
        for host in [
            "-oProxyCommand=calc",
            "host;touch x",
            "user@host",
            "host\nother",
            "",
        ] {
            assert!(
                SshConfig {
                    host: host.into(),
                    ..Default::default()
                }
                .command()
                .is_err()
            );
        }
        for host in [
            "server.example.org",
            "127.0.0.1",
            "37.60.231.67",
            "::1",
            "my-ssh-alias",
            " 37.60.231.67\u{200B} ",
        ] {
            assert!(
                SshConfig {
                    host: host.into(),
                    ..Default::default()
                }
                .command()
                .is_ok()
            );
        }
        assert!(
            SshConfig {
                host: "server".into(),
                port: 0,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn connection_roundtrip_preserves_connection_options_without_credentials() {
        let connection = Connection {
            name: "Servidor de pruebas".into(),
            target: Target::Ssh(SshConfig {
                host: "localhost".into(),
                port: 2222,
                user: "developer".into(),
                identity: String::new(),
            }),
        };
        let json = serde_json::to_string(&connection).unwrap();
        let loaded: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.description(), "developer@localhost:2222");
        assert!(!json.contains("password"));
    }
    #[test]
    fn known_host_target_includes_nonstandard_ports() {
        assert_eq!(
            SshConfig {
                host: " server.example.org ".into(),
                port: 22,
                ..Default::default()
            }
            .known_host_target(),
            "server.example.org"
        );
        assert_eq!(
            SshConfig {
                host: "2001:db8::1".into(),
                port: 2222,
                ..Default::default()
            }
            .known_host_target(),
            "[2001:db8::1]:2222"
        );
    }
}
