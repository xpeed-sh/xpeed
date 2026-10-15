//! Git-backed catalog metadata and installations. No release assets or archives.
use super::*;
use pulldown_cmark::{Event, Parser, Tag};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CatalogSource {
    pub repository: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default = "catalog_path")]
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    fn temp() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("xpeed-git-test-{:016x}", rand::random::<u64>()));
        std::fs::create_dir(&path).unwrap();
        path
    }
    fn manifest() -> Manifest {
        let mut m: Manifest = serde_json::from_str(
            include_str!("fixtures/codex.json").trim_start_matches('\u{feff}'),
        )
        .unwrap();
        m.id = "example.remote".into();
        m.name = "Extensión remota".into();
        m.icon = Some("assets/icon.png".into());
        m
    }
    fn api(responses: Vec<(String, Vec<u8>)>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for (path, bytes) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(
                    request.starts_with(&format!("GET {path} HTTP/1.1")),
                    "{request}"
                );
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                stream.write_all(&bytes).unwrap();
            }
        });
        (address, worker)
    }
    #[test]
    fn repository_links_reject_options_local_files_and_path_escapes() {
        assert_eq!(
            Repository::parse("https://github.com/xpeed-sh/codex.git/", "main", "")
                .unwrap()
                .url(),
            "https://github.com/xpeed-sh/codex"
        );
        for url in [
            "--upload-pack=bad",
            "file:///tmp/repo",
            "http://github.com/a/b",
            "https://user:secret@github.com/a/b",
            "https://github.com/a/b/tree/main",
            "https://github.com/a/b?token=secret",
            "https://evil.example/a/b",
        ] {
            assert!(Repository::parse(url, "", "").is_err());
        }
        for (reference, folder) in [
            ("--bad", ""),
            ("../main", ""),
            ("", "../outside"),
            ("", "C:/outside"),
        ] {
            assert!(Repository::parse("https://github.com/a/b", reference, folder).is_err());
        }
        let item: CatalogItem = serde_json::from_str(
            r#"{"id":"example.remote","repository":"https://github.com/a/b"}"#,
        )
        .unwrap();
        assert!(Repository::from_item(&item).is_ok());
    }
    #[test]
    fn repository_metadata_reads_manifest_readme_image_and_keeps_offline_cache() {
        let m = manifest();
        let png = include_bytes!("../../assets/img/app-icon.png").to_vec();
        let (address, server) = api(vec![
            (
                "/repos/example/repo/contents/adapter/extension.json?ref=main".into(),
                serde_json::to_vec(&m).unwrap(),
            ),
            (
                "/repos/example/repo/contents/adapter/README.md?ref=main".into(),
                b"# Remote README\nDocumentation\n\n![Preview](assets/preview.png)".to_vec(),
            ),
            (
                "/repos/example/repo/contents/adapter/README.es.md?ref=main".into(),
                b"# Documentacion en espanol".to_vec(),
            ),
            (
                "/repos/example/repo/contents/adapter/assets/preview.png?ref=main".into(),
                png.clone(),
            ),
            (
                "/repos/example/repo/contents/adapter/assets/icon.png?ref=main".into(),
                png,
            ),
        ]);
        let cache = temp();
        let repo = Repository::parse("https://github.com/example/repo", "main", "adapter").unwrap();
        let entry = metadata(
            &client().unwrap(),
            &address,
            &repo,
            "example.remote",
            "Git",
            &cache,
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(entry.item.name, "Extensión remota");
        assert_eq!(Manifest::read(&entry.folder).unwrap().id, "example.remote");
        assert!(
            std::fs::read_to_string(entry.folder.join("README.es.md"))
                .unwrap()
                .contains("Documentacion en espanol")
        );
        assert!(
            std::fs::read_to_string(entry.folder.join("README.md"))
                .unwrap()
                .contains("Remote README")
        );
        assert!(entry.folder.join("assets/preview.png").is_file());
        let mut extensions = Extensions::new(&cache, true);
        assert!(
            extensions
                .image(&m, &entry.folder, &egui::Context::default())
                .is_some()
        );
        let before = std::fs::read(entry.folder.join("extension.json")).unwrap();
        assert!(
            metadata(
                &client().unwrap(),
                &address,
                &repo,
                "example.remote",
                "Git",
                &cache
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read(entry.folder.join("extension.json")).unwrap(),
            before
        );
        std::fs::remove_dir_all(cache).unwrap();
    }
    #[test]
    fn mismatched_repo_identity_and_oversized_metadata_are_rejected() {
        let m = manifest();
        let cache = temp();
        let repo = Repository::parse("https://github.com/example/repo", "", "").unwrap();
        let (address, server) = api(vec![(
            "/repos/example/repo/contents/extension.json".into(),
            serde_json::to_vec(&m).unwrap(),
        )]);
        assert!(
            metadata(
                &client().unwrap(),
                &address,
                &repo,
                "other.extension",
                "",
                &cache
            )
            .is_err()
        );
        server.join().unwrap();
        assert!(!repo.cache_folder(&cache).exists());
        let (address, server) = api(vec![(
            "/repos/example/repo/contents/extension.json".into(),
            vec![b'a'; 65537],
        )]);
        assert!(file(&client().unwrap(), &address, &repo, "extension.json", 65536).is_err());
        server.join().unwrap();
        std::fs::remove_dir_all(cache).unwrap();
    }
    #[test]
    fn actual_git_checkout_installs_project_without_git_history_or_running_provider() {
        let root = temp();
        let source = root.join("source");
        std::fs::create_dir_all(source.join("bin")).unwrap();
        let mut m = manifest();
        m.icon = None;
        std::fs::write(
            source.join("extension.json"),
            serde_json::to_vec(&m).unwrap(),
        )
        .unwrap();
        std::fs::write(
            source.join(m.entrypoints.get(std::env::consts::OS).unwrap()),
            b"fixture, never executed",
        )
        .unwrap();
        std::fs::write(source.join("README.md"), "# Repository documentation").unwrap();
        for args in [
            vec!["init", "-b", "main"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "commit",
                "-m",
                "Fixture",
            ],
        ] {
            assert!(
                git_command()
                    .current_dir(&source)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let repo = Repository::parse("https://github.com/example/repo", "main", "").unwrap();
        let file_url =
            reqwest::Url::from_directory_path(std::fs::canonicalize(&source).unwrap()).unwrap();
        let mut command = git_command();
        command.args([
            "-c",
            "protocol.file.allow=always",
            "-c",
            &format!("url.{file_url}.insteadOf={}", repo.url()),
        ]);
        let cache = root.join("cache");
        let checkout = checkout_using(&repo, &cache, command).unwrap();
        let job = checkout.job.clone();
        assert!(checkout.package.join(".git").exists());
        let mut extensions = Extensions::new(&root, true);
        extensions.root = root.join("installed");
        extensions.ephemeral = false;
        extensions.install(&checkout.package).unwrap();
        let installed = &extensions.runtimes["example.remote"];
        assert!(installed.process.is_none());
        assert!(!installed.folder.join(".git").exists());
        assert!(installed.folder.join(".xpeed-source.json").exists());
        drop(checkout);
        assert!(!job.exists());
        extensions.uninstall("example.remote").unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn official_catalog_sources_migrate_to_the_root_index() {
        let mut source = CatalogSource {
            repository: "https://github.com/xpeed-sh/xpeed".into(),
            reference: String::new(),
            path: "extensions/catalog.json".into(),
        };
        migrate_official_catalog_path(&mut source);
        assert_eq!(source.path, "extensions.json");

        let mut third_party = CatalogSource {
            repository: "https://github.com/example/catalog".into(),
            reference: String::new(),
            path: "extensions/catalog.json".into(),
        };
        migrate_official_catalog_path(&mut third_party);
        assert_eq!(third_party.path, "extensions/catalog.json");
    }
}
fn catalog_path() -> String {
    "extensions.json".into()
}

pub(super) fn migrate_official_catalog_path(source: &mut CatalogSource) {
    if source.path != "extensions/catalog.json" {
        return;
    }
    let Ok(repository) = Repository::parse(&source.repository, &source.reference, "") else {
        return;
    };
    if repository.owner == "xpeed-sh" && repository.name == "xpeed" {
        source.path = catalog_path();
    }
}

#[derive(Default)]
pub(super) struct State {
    pub repository_input: String,
    pub reference_input: String,
    pub path_input: String,
    pub catalog_input: String,
    pub status: String,
    pub refresh_pending: bool,
    incoming: Option<Receiver<Result<Completion, String>>>,
}
impl State {
    pub fn busy(&self) -> bool {
        self.incoming.is_some()
    }
    fn start(
        &mut self,
        ctx: &egui::Context,
        status: &str,
        work: impl FnOnce() -> Result<Completion, String> + Send + 'static,
    ) {
        let (sender, incoming) = mpsc::channel();
        self.incoming = Some(incoming);
        self.status = status.into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = work();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
}

#[derive(Clone)]
pub(super) struct Repository {
    owner: String,
    name: String,
    pub reference: String,
    pub package_path: String,
}
impl Repository {
    pub fn parse(value: &str, reference: &str, package_path: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(value.trim())
            .map_err(|_| xpeed_i18n::t("Introduce un enlace HTTPS de GitHub"))?;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(xpeed_i18n::t(
                "Usa https://github.com/organización/repositorio, sin credenciales",
            )
            .into());
        }
        let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
        if parts.len() != 2 {
            return Err(xpeed_i18n::t(
                "El enlace debe apuntar al repositorio, no a una página o archivo",
            )
            .into());
        }
        let name = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
        let valid = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        };
        if !valid(parts[0])
            || !valid(name)
            || reference.len() > 128
            || reference.starts_with('-')
            || reference.contains("..")
            || reference
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
            || (!package_path.is_empty() && !relative_path(package_path))
        {
            return Err(xpeed_i18n::t("Repositorio, referencia o subcarpeta inválidos").into());
        }
        Ok(Self {
            owner: parts[0].into(),
            name: name.into(),
            reference: reference.into(),
            package_path: package_path.into(),
        })
    }
    pub fn from_item(item: &CatalogItem) -> Result<Self, String> {
        Self::parse(
            item.repository
                .as_deref()
                .ok_or(xpeed_i18n::t("Entrada sin repositorio"))?,
            &item.reference,
            &item.package_path,
        )
    }
    fn url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }
    pub fn cache_folder(&self, root: &Path) -> PathBuf {
        let input = format!("{}:{}:{}", self.url(), self.reference, self.package_path);
        let key = input.bytes().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
        root.join("catalog").join(format!("{key:016x}"))
    }
    fn file_path(&self, name: &str) -> String {
        if self.package_path.is_empty() {
            name.into()
        } else {
            format!("{}/{}", self.package_path, name)
        }
    }
}

struct Entry {
    item: CatalogItem,
    folder: PathBuf,
}
enum Completion {
    Catalog(Vec<Entry>, Vec<String>),
    Repository(Entry),
    Checkout(Entry, Checkout),
}
pub(super) struct Checkout {
    job: PathBuf,
    cache: PathBuf,
    package: PathBuf,
}
impl Drop for Checkout {
    fn drop(&mut self) {
        // Only remove our own freshly created job directory, never a repo/user path.
        if let (Ok(job), Ok(cache)) = (
            std::fs::canonicalize(&self.job),
            std::fs::canonicalize(&self.cache),
        ) && job.parent() == Some(cache.join("jobs").as_path())
        {
            let _ = std::fs::remove_dir_all(job);
        }
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("Xpeed/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}
fn file(
    client: &reqwest::blocking::Client,
    api: &str,
    repo: &Repository,
    path: &str,
    limit: u64,
) -> Result<Vec<u8>, String> {
    if !relative_path(path) {
        return Err(xpeed_i18n::t("Ruta de archivo inválida").into());
    }
    let mut url = reqwest::Url::parse(api).map_err(|e| e.to_string())?;
    url.path_segments_mut()
        .map_err(|_| xpeed_i18n::t("URL de API inválida"))?
        .extend(["repos", &repo.owner, &repo.name, "contents"])
        .extend(path.split('/'));
    if !repo.reference.is_empty() {
        url.query_pairs_mut().append_pair("ref", &repo.reference);
    }
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github.raw+json")
        .send()
        .map_err(|e| {
            xpeed_i18n::format(
                "No se pudo leer {}: {e}",
                &[("0", repo.url().to_string()), ("e", format!("{}", e))],
            )
        })?;
    if !response.status().is_success() {
        return Err(xpeed_i18n::format(
            "{}: HTTP {} al leer {path}",
            &[
                ("0", repo.url().to_string()),
                ("1", format!("{}", response.status().as_u16())),
                ("path", path.to_string()),
            ],
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(xpeed_i18n::format(
            "{path} supera el tamaño permitido",
            &[("path", path.to_string())],
        ));
    }
    Ok(bytes)
}
fn metadata(
    client: &reqwest::blocking::Client,
    api: &str,
    repo: &Repository,
    expected: &str,
    requirements: &str,
    cache: &Path,
) -> Result<Entry, String> {
    let bytes = file(client, api, repo, &repo.file_path("extension.json"), 65536)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|e| e.to_string())?
        .trim_start_matches('\u{feff}');
    let m: Manifest = serde_json::from_str(text).map_err(|e| {
        xpeed_i18n::format("extension.json inválido: {e}", &[("e", format!("{}", e))])
    })?;
    m.validate()?;
    if !expected.is_empty() && expected != m.id {
        return Err(xpeed_i18n::format(
            "El catálogo esperaba {expected}, pero el repositorio declara {}",
            &[("expected", expected.to_string()), ("0", m.id.to_string())],
        ));
    }
    let folder = repo.cache_folder(cache);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    // The manifest is committed last so failed mandatory metadata never replaces a valid cache.
    let localized_readmes: std::collections::BTreeSet<_> = m
        .localizations
        .values()
        .filter_map(|l| l.readme.as_deref())
        .filter(|path| *path != "README.md")
        .collect();
    let readmes = std::iter::once("README.md").chain(localized_readmes);
    let mut documents = Vec::new();
    for readme in readmes {
        if let Ok(content) = file(client, api, repo, &repo.file_path(readme), 65536) {
            let target = folder.join(readme);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&target, &content).map_err(|e| e.to_string())?;
            if let Ok(text) = std::str::from_utf8(&content) {
                documents.push((
                    Path::new(readme)
                        .parent()
                        .unwrap_or(Path::new(""))
                        .to_path_buf(),
                    text.to_owned(),
                ));
            }
        }
    }
    let images: std::collections::BTreeSet<_> = documents
        .iter()
        .flat_map(|(parent, text)| {
            Parser::new(text).filter_map(move |event| {
                let Event::Start(Tag::Image { dest_url, .. }) = event else {
                    return None;
                };
                let extension = Path::new(dest_url.as_ref())
                    .extension()
                    .and_then(|value| value.to_str())?
                    .to_ascii_lowercase();
                let path = parent.join(dest_url.as_ref());
                let path = path.to_string_lossy().replace('\\', "/");
                (relative_path(&path)
                    && matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "gif" | "svg"))
                .then_some(path)
            })
        })
        .take(12)
        .collect();
    for image_path in images {
        if let Ok(image) = file(
            client,
            api,
            repo,
            &repo.file_path(&image_path),
            4 * 1024 * 1024,
        ) {
            let target = folder.join(&image_path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(target, image).map_err(|e| e.to_string())?;
        }
    }
    if let Some(icon) = &m.icon
        && let Ok(image) = file(client, api, repo, &repo.file_path(icon), 2 * 1024 * 1024)
    {
        let target = folder.join(icon);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(target, image).map_err(|e| e.to_string())?;
    }
    std::fs::write(folder.join("extension.json"), bytes).map_err(|e| e.to_string())?;
    Ok(Entry {
        item: CatalogItem {
            id: m.id,
            name: m.name,
            description: m.description,
            requirements: requirements.into(),
            repository: Some(repo.url()),
            reference: repo.reference.clone(),
            package_path: repo.package_path.clone(),
            ..Default::default()
        },
        folder,
    })
}

fn git_command() -> Command {
    let mut command = Command::new("git");
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null);
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "init.templateDir=",
            "-c",
            "credential.helper=",
            "-c",
            "protocol.file.allow=never",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}
fn checkout(repo: &Repository, cache: &Path) -> Result<Checkout, String> {
    checkout_using(repo, cache, git_command())
}
fn checkout_using(
    repo: &Repository,
    cache: &Path,
    mut command: Command,
) -> Result<Checkout, String> {
    let job = cache
        .join("jobs")
        .join(format!("{:016x}", rand::random::<u64>()));
    std::fs::create_dir_all(&job).map_err(|e| e.to_string())?;
    let directory = job.join("repository");
    let mut result = Checkout {
        job,
        cache: cache.into(),
        package: directory.clone(),
    };
    command.args([
        "clone",
        "--quiet",
        "--depth",
        "1",
        "--no-recurse-submodules",
    ]);
    if !repo.reference.is_empty() {
        command.args(["--branch", &repo.reference]);
    }
    command.arg("--").arg(repo.url()).arg(&directory);
    let mut child = command.spawn().map_err(|e| {
        xpeed_i18n::format(
            "No se pudo ejecutar Git. Instala Git y comprueba que esté en PATH: {e}",
            &[("e", format!("{}", e))],
        )
    })?;
    #[cfg(windows)]
    let _job = match rpc::Job::attach(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(xpeed_i18n::t("Git no pudo descargar el repositorio. Revisa el enlace, la rama y el acceso público.").into());
            }
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(xpeed_i18n::t("La descarga Git superó dos minutos").into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    result.package = directory.join(&repo.package_path);
    let root = std::fs::canonicalize(&directory).map_err(|e| e.to_string())?;
    let package = std::fs::canonicalize(&result.package).map_err(|e| e.to_string())?;
    if !package.starts_with(&root) {
        return Err(xpeed_i18n::t("La carpeta de extensión sale del repositorio").into());
    }
    let m = Manifest::read(&package)?;
    m.executable(&package).map_err(|_| xpeed_i18n::t("El repositorio no contiene el ejecutable para este sistema en la ruta de extension.json. El autor debe incluirlo en bin/; Xpeed no compila código automáticamente.").to_string())?;
    let mut commit = git_command();
    let output = commit
        .arg("-C")
        .arg(&directory)
        .args(["rev-parse", "HEAD"])
        .stdout(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(xpeed_i18n::t("No se pudo identificar el commit descargado").into());
    }
    std::fs::write(package.join(".xpeed-source.json"), serde_json::to_vec_pretty(&json!({"repository":repo.url(),"reference":repo.reference,"commit":String::from_utf8_lossy(&output.stdout).trim()})).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(result)
}

impl Extensions {
    pub(super) fn git_cache(&self) -> PathBuf {
        self.root
            .parent()
            .unwrap_or(&self.cwd)
            .join("extension-cache")
    }
    pub fn poll_git(&mut self, ctx: &egui::Context) {
        if self.git.refresh_pending && !self.git.busy() && !self.ephemeral {
            self.git.refresh_pending = false;
            self.refresh_git(ctx);
        }
        let completion = self
            .git
            .incoming
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(xpeed_i18n::t(
                    "Se interrumpió la conexión Git; vuelve a intentarlo",
                )
                .into())),
            });
        let Some(completion) = completion else {
            return;
        };
        self.git.incoming = None;
        match completion {
            Ok(Completion::Catalog(entries, errors)) => {
                for entry in entries {
                    self.preferences
                        .repositories
                        .retain(|c| c.id != entry.item.id);
                    self.preferences.repositories.push(entry.item.clone());
                    self.merge_git_entry(entry);
                }
                self.save();
                self.git.status = if errors.is_empty() {
                    xpeed_i18n::t("Catálogo actualizado")
                } else {
                    xpeed_i18n::t("Se conserva el catálogo disponible; algunas conexiones fallaron")
                }
                .into();
                self.error = None;
            }
            Ok(Completion::Repository(entry)) => {
                let id = entry.item.id.clone();
                self.preferences.repositories.retain(|c| c.id != id);
                self.preferences.repositories.push(entry.item.clone());
                self.merge_git_entry(entry);
                self.save();
                self.request_details(&id);
                self.git.status = xpeed_i18n::t("Repositorio conectado").into();
                self.error = None;
            }
            Ok(Completion::Checkout(entry, checkout)) => {
                self.review_install(&checkout.package);
                if self.error.is_none() {
                    self.merge_git_entry(entry);
                    self.git.status = xpeed_i18n::t("Extensión instalada desde Git").into();
                } else {
                    self.git.status.clear();
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.git.status.clear();
            }
        }
    }
    fn merge_git_entry(&mut self, entry: Entry) {
        self.detail_documents.clear();
        self.images.clear();
        if let Some(old) = self.catalog.iter_mut().find(|(c, _)| c.id == entry.item.id) {
            *old = (entry.item, entry.folder);
        } else {
            self.catalog.push((entry.item, entry.folder));
        }
    }
    pub(super) fn connect_repository(&mut self, ctx: &egui::Context) {
        if self.git.busy() || self.ephemeral {
            return;
        }
        let repo = match Repository::parse(
            &self.git.repository_input,
            &self.git.reference_input,
            &self.git.path_input,
        ) {
            Ok(repo) => repo,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let cache = self.git_cache();
        self.error = None;
        self.git
            .start(ctx, xpeed_i18n::t("Leyendo repositorio…"), move || {
                metadata(&client()?, "https://api.github.com", &repo, "", "", &cache)
                    .map(Completion::Repository)
            });
    }
    pub(super) fn install_git(&mut self, item: &CatalogItem, ctx: &egui::Context) {
        if self.git.busy() || self.ephemeral {
            return;
        }
        let repo = match Repository::from_item(item) {
            Ok(repo) => repo,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let cache = self.git_cache();
        let expected = item.id.clone();
        let requirements = item.requirements.clone();
        self.error = None;
        self.git.start(
            ctx,
            xpeed_i18n::t("Descargando proyecto Git…"),
            move || {
                let checkout = checkout(&repo, &cache)?;
                let m = Manifest::read(&checkout.package)?;
                if m.id != expected {
                    return Err(
                        xpeed_i18n::t("El repositorio descargado cambió de identidad").into(),
                    );
                }
                let entry = Entry {
                    item: CatalogItem {
                        id: m.id,
                        name: m.name,
                        description: m.description,
                        repository: Some(repo.url()),
                        reference: repo.reference.clone(),
                        package_path: repo.package_path.clone(),
                        requirements,
                        ..Default::default()
                    },
                    folder: repo.cache_folder(&cache),
                };
                Ok(Completion::Checkout(entry, checkout))
            },
        );
    }
    pub(super) fn refresh_git(&mut self, ctx: &egui::Context) {
        if self.git.busy() || self.ephemeral {
            return;
        }
        let items: Vec<_> = self
            .catalog
            .iter()
            .filter(|(c, _)| c.repository.is_some())
            .map(|(c, _)| c.clone())
            .collect();
        let sources = self.preferences.catalog_sources.clone();
        let cache = self.git_cache();
        self.error = None;
        self.git.start(
            ctx,
            xpeed_i18n::t("Actualizando catálogo Git…"),
            move || {
                let client = client()?;
                let api = "https://api.github.com";
                let mut items = items;
                let mut errors = Vec::new();
                for source in sources {
                    let parsed = Repository::parse(&source.repository, &source.reference, "")
                        .and_then(|repo| file(&client, api, &repo, &source.path, 65536))
                        .and_then(|bytes| {
                            serde_json::from_str::<Vec<CatalogItem>>(
                                std::str::from_utf8(&bytes)
                                    .map_err(|e| e.to_string())?
                                    .trim_start_matches('\u{feff}'),
                            )
                            .map_err(|e| e.to_string())
                        });
                    match parsed {
                        Ok(mut added) => {
                            if added.len() > 32 {
                                errors.push(
                                    xpeed_i18n::t("El catálogo supera 32 extensiones").into(),
                                );
                            } else {
                                added.append(&mut items);
                                items = added;
                            }
                        }
                        Err(e) => errors.push(e),
                    }
                }
                let mut entries = Vec::new();
                let mut seen = std::collections::BTreeSet::new();
                let deadline = Instant::now() + Duration::from_secs(90);
                for item in items.into_iter().take(32) {
                    if !valid_id(&item.id) || !seen.insert(item.id.clone()) {
                        continue;
                    }
                    if Instant::now() > deadline {
                        errors.push(
                            xpeed_i18n::t(
                                "Actualización parcial: vuelve a actualizar para reintentar",
                            )
                            .into(),
                        );
                        break;
                    }
                    match Repository::from_item(&item).and_then(|repo| {
                        metadata(&client, api, &repo, &item.id, &item.requirements, &cache)
                    }) {
                        Ok(entry) => entries.push(entry),
                        Err(e) => errors.push(e),
                    }
                }
                Ok(Completion::Catalog(entries, errors))
            },
        );
    }
    pub(super) fn connect_catalog(&mut self, ctx: &egui::Context) {
        let source = CatalogSource {
            repository: self.git.catalog_input.trim().into(),
            reference: String::new(),
            path: catalog_path(),
        };
        if let Err(e) = Repository::parse(&source.repository, "", "") {
            self.error = Some(e);
            return;
        }
        if !self
            .preferences
            .catalog_sources
            .iter()
            .any(|s| s.repository == source.repository)
        {
            self.preferences.catalog_sources.push(source);
            self.save();
        }
        self.refresh_git(ctx);
    }
    pub(super) fn image(
        &mut self,
        manifest: &Manifest,
        folder: &Path,
        ctx: &egui::Context,
    ) -> Option<egui::TextureHandle> {
        let path = folder.join(manifest.icon.as_ref()?);
        self.images
            .entry(path.clone())
            .or_insert_with(|| {
                let size = std::fs::metadata(&path).ok()?.len();
                if size > 2 * 1024 * 1024 {
                    return None;
                }
                let bytes = std::fs::read(&path).ok()?;
                let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
                decoder.set_transformations(
                    png::Transformations::EXPAND | png::Transformations::STRIP_16,
                );
                let mut reader = decoder.read_info().ok()?;
                if reader.info().width > 1024 || reader.info().height > 1024 {
                    return None;
                }
                let mut pixels = vec![0; reader.output_buffer_size()?];
                let info = reader.next_frame(&mut pixels).ok()?;
                let channels = match info.color_type {
                    png::ColorType::Rgba => 4,
                    png::ColorType::Rgb => 3,
                    png::ColorType::GrayscaleAlpha => 2,
                    png::ColorType::Grayscale => 1,
                    _ => return None,
                };
                let rgba: Vec<_> = pixels[..info.buffer_size()]
                    .chunks_exact(channels)
                    .flat_map(|p| match channels {
                        4 => [p[0], p[1], p[2], p[3]],
                        3 => [p[0], p[1], p[2], 255],
                        2 => [p[0], p[0], p[0], p[1]],
                        _ => [p[0], p[0], p[0], 255],
                    })
                    .collect();
                Some(ctx.load_texture(
                    path.to_string_lossy(),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [info.width as usize, info.height as usize],
                        &rgba,
                    ),
                    egui::TextureOptions::LINEAR,
                ))
            })
            .clone()
    }
}
