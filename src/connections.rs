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
    pub fn validate(&self) -> Result<(), String> {
        if self.host.is_empty()
            || self.host.starts_with('-')
            || !self
                .host
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
        if self.user.starts_with('-')
            || !self
                .user
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        {
            return Err(
                xpeed_i18n::t("El nombre de usuario contiene caracteres no válidos.").into(),
            );
        }
        if !self.identity.is_empty() && !PathBuf::from(&self.identity).is_file() {
            return Err(xpeed_i18n::t("No se encontró el archivo de clave privada.").into());
        }
        Ok(())
    }
    pub fn label(&self) -> String {
        if self.user.is_empty() {
            self.host.clone()
        } else {
            format!("{}@{}", self.user, self.host)
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
        if !self.user.is_empty() {
            command.args(["-l", &self.user]);
        }
        if !self.identity.is_empty() {
            command.args(["-i", &self.identity]);
        }
        command.args(["--", &self.host]);
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
        for host in ["server.example.org", "127.0.0.1", "::1", "my-ssh-alias"] {
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
}
