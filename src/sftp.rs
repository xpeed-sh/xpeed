//! SFTP v3 over the user's OpenSSH transport. No remote shell commands.
use crate::connections::SshConfig;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};
#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub directory: bool,
    pub bytes: u64,
}
#[derive(Clone)]
pub struct Listing {
    pub path: String,
    pub entries: Vec<Entry>,
}
#[derive(Clone)]
pub enum Mutation {
    File(String),
    Folder(String),
    Rename(String, String),
    Remove(String, bool),
}
#[derive(Clone)]
struct ListRequest {
    path: String,
    reply: mpsc::Sender<Result<Listing, String>>,
}
struct ListWorker {
    tx: mpsc::Sender<ListRequest>,
    cancel: Arc<AtomicBool>,
}
impl Drop for ListWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
fn listing_worker(
    config: SshConfig,
    password: Option<crate::credentials::Secret>,
    hosts: Option<std::path::PathBuf>,
) -> ListWorker {
    let (tx, rx) = mpsc::channel::<ListRequest>();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    std::thread::spawn(move || {
        let active = Arc::new(AtomicU64::new(1));
        let result = transport_monitored(
            &config,
            worker_cancel.clone(),
            password,
            hosts.as_deref(),
            active.clone(),
            |input, output| {
                init_streams(input, output)?;
                active.store(0, Ordering::Release);
                let mut generation = 1;
                while !worker_cancel.load(Ordering::Acquire) {
                    let mut request = match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    // Directory changes can arrive faster than the network. Only list the latest one.
                    while let Ok(newer) = rx.try_recv() {
                        request = newer;
                    }
                    generation += 1;
                    active.store(generation, Ordering::Release);
                    let result = list_directory_streams(input, output, &request.path);
                    active.store(0, Ordering::Release);
                    let failed = result.is_err();
                    let _ = request.reply.send(result);
                    if failed {
                        return Err(xpeed_i18n::t(
                            "La conexión SFTP terminó; vuelve a refrescar para reconectar.",
                        )
                        .into());
                    }
                }
                Ok(())
            },
        );
        if let Err(error) = result {
            for request in rx.try_iter() {
                let _ = request.reply.send(Err(error.clone()));
            }
        }
    });
    ListWorker { tx, cancel }
}
fn received<T>(receiver: &Receiver<Result<T, String>>) -> Option<Result<T, String>> {
    match receiver.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => Some(Err(xpeed_i18n::t(
            "La conexión SFTP terminó; pulsa Refrescar para reconectar.",
        )
        .into())),
    }
}
fn expect_ok(reply: &[u8]) -> Result<(), String> {
    if reply.first() == Some(&101) && reply.get(5..9) == Some(&0u32.to_be_bytes()) {
        Ok(())
    } else {
        Err(status_error(reply))
    }
}
fn mutate_streams(
    input: &mut impl Write,
    output: &mut impl Read,
    operations: &[Mutation],
) -> Result<(), String> {
    write_packet(input, &[1, 0, 0, 0, 3])?;
    let version = read_packet(output)?;
    if version.get(..5) != Some(&[2, 0, 0, 0, 3]) {
        return Err(xpeed_i18n::t("El servidor no respondió al protocolo SFTP.").into());
    }
    let mut id = 1;
    for operation in operations {
        let (kind, body) = match operation {
            Mutation::File(path) => {
                let mut body = string(path.as_bytes());
                // WRITE | CREAT | EXCL: never truncate an existing file.
                body.extend_from_slice(&42u32.to_be_bytes());
                body.extend_from_slice(&0u32.to_be_bytes());
                let reply = request(input, output, 3, id, &body)?;
                id += 1;
                if reply[0] != 102 {
                    return Err(status_error(&reply));
                }
                let handle = Cursor::new(&reply[5..]).string()?.to_vec();
                (4, string(&handle))
            }
            Mutation::Folder(path) => {
                let mut body = string(path.as_bytes());
                body.extend_from_slice(&0u32.to_be_bytes());
                (14, body)
            }
            Mutation::Rename(from, to) => {
                let mut body = string(from.as_bytes());
                body.extend(string(to.as_bytes()));
                (18, body)
            }
            Mutation::Remove(path, directory) => {
                (if *directory { 15 } else { 13 }, string(path.as_bytes()))
            }
        };
        expect_ok(&request(input, output, kind, id, &body)?)?;
        id += 1;
    }
    Ok(())
}
pub struct RemoteExplorer {
    pub browser: crate::file_explorer::Browser,
    config: SshConfig,
    pub path: String,
    pub entries: Vec<Entry>,
    pub error: Option<String>,
    pub loading: bool,
    pub children: BTreeMap<String, Vec<Entry>>,
    pub expanded: BTreeSet<String>,
    pub authenticated: bool,
    pub follow_terminal: bool,
    pub activity: Vec<String>,
    pub edit_requested: Option<String>,
    pub download_requested: Option<String>,
    pub transferred: Arc<AtomicU64>,
    pub transfer_total: u64,
    cache: BTreeMap<String, (std::time::Instant, Listing)>,
    retry_listing: bool,
    home: Option<String>,
    back: Vec<String>,
    last_directory: Option<String>,
    password: Option<crate::credentials::Secret>,
    requested: String,
    result: Option<Receiver<Result<Listing, String>>>,
    worker: Option<ListWorker>,
    operation: Option<Receiver<Result<(), String>>>,
    operation_cancel: Option<Arc<AtomicBool>>,
}
impl RemoteExplorer {
    pub fn new(config: SshConfig) -> Self {
        let mut explorer = Self::idle(config);
        explorer.refresh();
        explorer
    }
    fn idle(config: SshConfig) -> Self {
        Self {
            browser: Default::default(),
            config,
            path: ".".into(),
            entries: Vec::new(),
            error: None,
            loading: false,
            children: BTreeMap::new(),
            expanded: BTreeSet::new(),
            authenticated: false,
            follow_terminal: true,
            activity: Vec::new(),
            edit_requested: None,
            download_requested: None,
            transferred: Arc::new(AtomicU64::new(0)),
            transfer_total: 0,
            cache: BTreeMap::new(),
            retry_listing: false,
            home: None,
            back: Vec::new(),
            last_directory: None,
            password: None,
            requested: ".".into(),
            result: None,
            worker: None,
            operation: None,
            operation_cancel: None,
        }
    }
    // In-memory fixture for isolated screenshots and UI tests; opens no transport.
    pub fn preview() -> Self {
        let mut explorer = Self::idle(SshConfig::default());
        explorer.config.host = "192.0.2.10".into();
        explorer.path = "/doolpool.com".into();
        explorer.home = Some(explorer.path.clone());
        let entry = |name: &str, directory, bytes| Entry {
            name: name.into(),
            directory,
            bytes,
        };
        explorer.entries = vec![
            entry(".cache", true, 0),
            entry(".well-known", true, 0),
            entry("esta", true, 0),
            entry(".bash_history", false, 172),
            entry("hola.php", false, 0),
            entry("index.php", false, 17200),
            entry("foto.JPG", false, 248000),
            entry("musica.flac", false, 8400000),
            entry("video.mp4", false, 17300000),
        ];
        explorer.children.insert(
            "/doolpool.com/esta".into(),
            vec![
                entry("hola.php", false, 0),
                entry("index.php", false, 17200),
            ],
        );
        explorer.expanded.insert("/doolpool.com/esta".into());
        explorer
    }
    pub fn resume_follow(&mut self) {
        self.last_directory = None;
    }
    pub fn follow(&mut self, directory: &str) {
        if self.home.is_none()
            || !self.follow_terminal
            || self.last_directory.as_deref() == Some(directory)
        {
            return;
        }
        let Some(path) = resolve_directory(directory, self.home.as_deref()) else {
            return;
        };
        // Remember even an unsuccessful listing: do not repeatedly retry every UI frame.
        self.last_directory = Some(directory.into());
        if path != self.path {
            self.navigate(path);
        }
    }
    pub fn navigate(&mut self, path: String) {
        if path != self.path {
            self.back.push(self.path.clone());
        }
        self.browser.selected.clear();
        self.path = path;
        self.children.clear();
        self.expanded.clear();
        self.entries.clear();
        self.load_cached_or_request();
    }
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn at_home(&self) -> bool {
        self.home.as_ref().is_none_or(|home| home == &self.path)
    }
    pub fn go_back(&mut self) {
        if let Some(path) = self.back.pop() {
            self.path = path;
            self.browser.selected.clear();
            self.children.clear();
            self.expanded.clear();
            self.entries.clear();
            self.load_cached_or_request();
        }
    }
    pub fn host(&self) -> &str {
        &self.config.host
    }
    pub fn monitoring_access(&self) -> Option<crate::credentials::Secret> {
        self.password.clone()
    }
    pub fn set_password(&mut self, password: crate::credentials::Secret) {
        self.worker = None;
        self.password = Some(password);
        self.authenticated = false;
        self.refresh();
    }
    pub fn expand(&mut self, path: String) {
        if self.expanded.remove(&path) {
            return;
        }
        self.expanded.insert(path.clone());
        if !self.children.contains_key(&path) {
            self.request(path);
        }
    }
    fn load_cached_or_request(&mut self) {
        if let Some((at, listing)) = self.cache.get(&self.path)
            && at.elapsed() < Duration::from_secs(15)
        {
            self.entries = listing.entries.clone();
            self.error = None;
            self.loading = false;
            self.result = None;
        } else {
            self.request(self.path.clone());
        }
    }
    pub fn connection(&self) -> SshConfig {
        self.config.clone()
    }
    pub fn cancel_operation(&self) {
        if let Some(cancel) = &self.operation_cancel {
            cancel.store(true, Ordering::Release);
        }
    }
    pub fn transfer(&mut self, files: Vec<(std::path::PathBuf, String)>, download: bool) {
        if self.busy() || files.is_empty() {
            return;
        }
        let config = self.config.clone();
        let password = self.password.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.operation_cancel = Some(cancel.clone());
        self.transferred.store(0, Ordering::Release);
        self.transfer_total = if download {
            0
        } else {
            files
                .iter()
                .filter_map(|(p, _)| p.metadata().ok())
                .map(|m| m.len())
                .sum()
        };
        let progress = self.transferred.clone();
        let (tx, rx) = mpsc::channel();
        self.operation = Some(rx);
        self.error = None;
        std::thread::spawn(move || {
            let active = Arc::new(AtomicU64::new(1));
            let heartbeat = active.clone();
            let result = transport_monitored(
                &config,
                cancel.clone(),
                password,
                None,
                active,
                |input, output| {
                    init_streams(input, output)?;
                    for (local, remote) in files {
                        transfer_streams(
                            input, output, &local, &remote, download, &cancel, &progress,
                            &heartbeat,
                        )?;
                    }
                    Ok(())
                },
            );
            let _ = tx.send(result);
        });
    }
    pub fn refresh(&mut self) {
        self.browser.selected.clear();
        self.children.clear();
        self.expanded.clear();
        self.cache.clear();
        self.retry_listing = false;
        self.request(self.path.clone());
    }
    fn request(&mut self, path: String) {
        self.requested = path.clone();
        let (tx, rx) = mpsc::channel();
        self.loading = true;
        self.error = None;
        self.result = Some(rx);
        let request = ListRequest { path, reply: tx };
        if self
            .worker
            .as_ref()
            .is_none_or(|worker| worker.tx.send(request.clone()).is_err())
        {
            self.worker = Some(listing_worker(
                self.config.clone(),
                self.password.clone(),
                None,
            ));
            if let Some(worker) = &self.worker {
                let _ = worker.tx.send(request);
            }
        }
    }
    pub fn poll(&mut self) {
        if let Some(result) = self.operation.as_ref().and_then(received) {
            self.operation = None;
            self.operation_cancel = None;
            self.transfer_total = 0;
            self.transferred.store(0, Ordering::Release);
            match result {
                Ok(()) => {
                    self.activity
                        .push(xpeed_i18n::t("Operación de archivos completada").into());
                    self.browser.selected.clear();
                    self.refresh();
                }
                Err(error) => {
                    self.activity.push(xpeed_i18n::format(
                        "Error de archivos: {error}",
                        &[("error", error.to_string())],
                    ));
                    self.refresh();
                    self.error = Some(error);
                }
            }
        }
        if let Some(result) = self.result.as_ref().and_then(received) {
            match result {
                Ok(listing) => {
                    self.authenticated = true;
                    self.retry_listing = false;
                    if self.cache.len() >= 32 {
                        self.cache.clear();
                    }
                    self.cache.insert(
                        listing.path.clone(),
                        (std::time::Instant::now(), listing.clone()),
                    );
                    if self.requested == self.path {
                        if self.requested == "." && self.home.is_none() {
                            self.home = Some(listing.path.clone());
                        }
                        self.path = listing.path;
                        self.entries = listing.entries;
                    } else {
                        self.children
                            .insert(self.requested.clone(), listing.entries);
                    }
                }
                Err(error) => {
                    self.worker = None;
                    self.error = Some(error);
                    if self.authenticated && !self.retry_listing {
                        self.retry_listing = true;
                        self.request(self.requested.clone());
                        return;
                    }
                }
            }
            self.loading = false;
            self.result = None;
        }
    }
    pub fn operation_running(&self) -> bool {
        self.operation.is_some()
    }
    pub fn busy(&self) -> bool {
        self.loading || self.operation.is_some()
    }
    pub fn mutate(&mut self, operations: Vec<Mutation>) {
        if self.busy() {
            return;
        }
        for operation in &operations {
            self.activity.push(match operation {
                Mutation::File(path) => {
                    xpeed_i18n::format("Crear archivo: {path}", &[("path", path.to_string())])
                }
                Mutation::Folder(path) => {
                    xpeed_i18n::format("Crear carpeta: {path}", &[("path", path.to_string())])
                }
                Mutation::Rename(from, to) => xpeed_i18n::format(
                    "Mover/renombrar: {from} → {to}",
                    &[("from", from.to_string()), ("to", to.to_string())],
                ),
                Mutation::Remove(path, _) => {
                    xpeed_i18n::format("Eliminar: {path}", &[("path", path.to_string())])
                }
            });
        }
        let config = self.config.clone();
        let password = self.password.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.operation_cancel = Some(cancel.clone());
        let (tx, rx) = mpsc::channel();
        self.operation = Some(rx);
        self.error = None;
        std::thread::spawn(move || {
            let result = transport(&config, cancel, password, None, |input, output| {
                mutate_streams(input, output, &operations)
            });
            let _ = tx.send(result);
        });
    }
}
impl Drop for RemoteExplorer {
    fn drop(&mut self) {
        if let Some(cancel) = &self.operation_cancel {
            cancel.store(true, Ordering::Release);
        }
    }
}
struct LocalTemporary(Option<std::path::PathBuf>);
impl Drop for LocalTemporary {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}
// Streaming files: bounded memory, exclusive destinations, progress heartbeat.
#[allow(clippy::too_many_arguments)] // Explicit streams, paths, cancellation, progress and watchdog.
fn transfer_streams(
    input: &mut impl Write,
    output: &mut impl Read,
    local: &std::path::Path,
    remote: &str,
    download: bool,
    cancel: &AtomicBool,
    progress: &AtomicU64,
    heartbeat: &AtomicU64,
) -> Result<(), String> {
    let temporary = if download {
        local
            .with_extension(format!("xpeed-{:016x}.part", rand::random::<u64>()))
            .to_string_lossy()
            .into_owned()
    } else {
        format!("{remote}.xpeed-{:016x}.part", rand::random::<u64>())
    };
    if download && local.exists() {
        return Err(xpeed_i18n::t("Destination already exists; choose another name.").into());
    }
    let _cleanup = LocalTemporary(download.then(|| std::path::PathBuf::from(&temporary)));
    let mut file = if download {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
    } else {
        std::fs::File::open(local)
    }
    .map_err(|e| e.to_string())?;
    let mut data = string(if download {
        remote.as_bytes()
    } else {
        temporary.as_bytes()
    });
    data.extend_from_slice(&(if download { 1u32 } else { 42u32 }).to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes());
    let reply = request(input, output, 3, 1, &data)?;
    if reply[0] != 102 {
        if download {
            let _ = std::fs::remove_file(&temporary);
        }
        return Err(status_error(&reply));
    }
    let handle = Cursor::new(&reply[5..]).string()?.to_vec();
    let result = (|| {
        let mut offset = 0u64;
        let mut id = 2u32;
        let mut buffer = [0u8; 32768];
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(xpeed_i18n::t("Operation cancelled").into());
            }
            let mut data = string(&handle);
            data.extend_from_slice(&offset.to_be_bytes());
            let count = if download {
                data.extend_from_slice(&32768u32.to_be_bytes());
                let reply = request(input, output, 5, id, &data)?;
                if reply[0] == 101 && Cursor::new(&reply[5..]).u32()? == 1 {
                    break;
                }
                if reply[0] != 103 {
                    return Err(status_error(&reply));
                }
                let bytes = Cursor::new(&reply[5..]).string()?;
                if bytes.is_empty() || bytes.len() > 32768 {
                    return Err("Invalid SFTP data length".into());
                }
                file.write_all(bytes).map_err(|e| e.to_string())?;
                bytes.len()
            } else {
                let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                data.extend(string(&buffer[..count]));
                expect_ok(&request(input, output, 6, id, &data)?)?;
                count
            };
            offset += count as u64;
            progress.fetch_add(count as u64, Ordering::Release);
            heartbeat.fetch_add(1, Ordering::Release);
            id = id.wrapping_add(1).max(2);
        }
        Ok(())
    })();
    let close =
        request(input, output, 4, u32::MAX - 2, &string(&handle)).and_then(|r| expect_ok(&r));
    let result = result.and(close).and_then(|()| {
        if cancel.load(Ordering::Acquire) {
            return Err(xpeed_i18n::t("Operation cancelled").into());
        }
        if download {
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            // hard_link never overwrites an existing destination, including a race.
            std::fs::hard_link(&temporary, local).map_err(|e| e.to_string())?;
            std::fs::remove_file(&temporary).map_err(|e| e.to_string())
        } else {
            let mut data = string(temporary.as_bytes());
            data.extend(string(remote.as_bytes()));
            expect_ok(&request(input, output, 18, u32::MAX - 1, &data)?)
        }
    });
    if result.is_err() {
        if download {
            let _ = std::fs::remove_file(&temporary);
        } else {
            let _ = request(input, output, 13, u32::MAX, &string(temporary.as_bytes()));
        }
    }
    result
}
fn list_with_hosts(
    config: &SshConfig,
    path: &str,
    cancel: Arc<AtomicBool>,
    password: Option<crate::credentials::Secret>,
    hosts: Option<&std::path::Path>,
) -> Result<Listing, String> {
    transport(config, cancel, password, hosts, |input, output| {
        list_streams(input, output, path)
    })
}

/// Independent SFTP channel for extension tools; credentials remain in Xpeed.
pub fn tool_list(
    config: &SshConfig,
    password: Option<crate::credentials::Secret>,
    cancel: Arc<AtomicBool>,
    path: &str,
    hosts: Option<&std::path::Path>,
) -> Result<Listing, String> {
    list_with_hosts(config, path, cancel, password, hosts)
}
pub fn tool_read(
    config: &SshConfig,
    password: Option<crate::credentials::Secret>,
    cancel: Arc<AtomicBool>,
    path: &str,
    hosts: Option<&std::path::Path>,
) -> Result<String, String> {
    transport(config, cancel, password, hosts, |input, output| {
        init_streams(input, output)?;
        read_text_streams(input, output, path)
    })
}
pub fn tool_replace(
    config: &SshConfig,
    password: Option<crate::credentials::Secret>,
    cancel: Arc<AtomicBool>,
    path: &str,
    expected: &str,
    replacement: &str,
    hosts: Option<&std::path::Path>,
) -> Result<(), String> {
    transport(config, cancel.clone(), password, hosts, |input, output| {
        init_streams(input, output)?;
        let canonical = tool_canonical(input, output, path)?;
        let path = canonical.as_str();
        if read_text_streams(input, output, path)? != expected {
            return Err("File changed since the proposal; read it again.".into());
        }
        // Write exclusively to a sibling and atomically rename with OpenSSH's
        // extension. Unsupported servers fail before touching the original.
        let attributes = request(input, output, 17, 1, &string(path.as_bytes()))?;
        if attributes[0] != 105 {
            return Err(status_error(&attributes));
        }
        let mut cursor = Cursor::new(&attributes[5..]);
        let flags = cursor.u32()?;
        if flags & 1 != 0 {
            cursor.bytes(8)?;
        }
        if flags & 2 != 0 {
            cursor.bytes(8)?;
        }
        let mode = if flags & 4 != 0 { cursor.u32()? } else { 0o600 };
        let temporary = format!("{path}.xpeed-{:032x}.tmp", rand::random::<u128>());
        let mut data = string(temporary.as_bytes());
        data.extend_from_slice(&0x2au32.to_be_bytes()); // WRITE | CREATE | EXCL
        data.extend_from_slice(&4u32.to_be_bytes());
        data.extend_from_slice(&mode.to_be_bytes());
        let reply = request(input, output, 3, 1, &data)?;
        if reply[0] != 102 {
            return Err(status_error(&reply));
        }
        let handle = Cursor::new(&reply[5..]).string()?.to_vec();
        let result = (|| {
            for (offset, chunk) in replacement.as_bytes().chunks(16384).enumerate() {
                let mut data = string(&handle);
                data.extend_from_slice(&((offset * 16384) as u64).to_be_bytes());
                data.extend(string(chunk));
                expect_ok(&request(input, output, 6, (offset + 2) as u32, &data)?)?;
            }
            Ok(())
        })();
        let close =
            request(input, output, 4, 9999, &string(&handle)).and_then(|reply| expect_ok(&reply));
        let result = result.and(close).and_then(|()| {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err("Cancelled before applying the change.".into());
            }
            if read_text_streams(input, output, path)? != expected {
                return Err("File changed since the proposal; read it again.".into());
            }
            let mut data = string(b"posix-rename@openssh.com");
            data.extend(string(temporary.as_bytes()));
            data.extend(string(path.as_bytes()));
            expect_ok(&request(input, output, 200, 10000, &data)?)
        });
        if result.is_err() {
            let _ = request(input, output, 13, 10001, &string(temporary.as_bytes()));
        }
        result
    })
}
fn read_text_streams(
    input: &mut impl Write,
    output: &mut impl Read,
    path: &str,
) -> Result<String, String> {
    let canonical = tool_canonical(input, output, path)?;
    let path = canonical.as_str();
    let mut data = string(path.as_bytes());
    data.extend_from_slice(&1u32.to_be_bytes()); // READ
    data.extend_from_slice(&0u32.to_be_bytes());
    let reply = request(input, output, 3, 1, &data)?;
    if reply[0] != 102 {
        return Err(status_error(&reply));
    }
    let handle = Cursor::new(&reply[5..]).string()?.to_vec();
    let result = (|| {
        let mut bytes = Vec::new();
        for id in 2..8 {
            let mut data = string(&handle);
            data.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
            data.extend_from_slice(&16384u32.to_be_bytes());
            let reply = request(input, output, 5, id, &data)?;
            if reply[0] == 101 && Cursor::new(&reply[5..]).u32()? == 1 {
                if bytes.contains(&0) {
                    return Err("Binary files cannot be opened in the text editor.".into());
                }
                return String::from_utf8(bytes)
                    .map_err(|_| "Only UTF-8 text files are supported.".into());
            }
            if reply[0] != 103 {
                return Err(status_error(&reply));
            }
            bytes.extend_from_slice(Cursor::new(&reply[5..]).string()?);
            if bytes.len() > 65536 {
                return Err("Text file exceeds 64 KiB.".into());
            }
        }
        Err("Text file exceeds 64 KiB.".into())
    })();
    let _ = request(input, output, 4, 9999, &string(&handle));
    result
}
fn tool_canonical(
    input: &mut impl Write,
    output: &mut impl Read,
    path: &str,
) -> Result<String, String> {
    let reply = request(input, output, 16, 1, &string(path.as_bytes()))?;
    if reply[0] != 104 {
        return Err(status_error(&reply));
    }
    let mut cursor = Cursor::new(&reply[5..]);
    if cursor.u32()? == 0 {
        return Err("SFTP did not return a canonical path.".into());
    }
    let path = String::from_utf8(cursor.string()?.to_vec()).map_err(|e| e.to_string())?;
    if crate::extensions::session_tools::credential_path(&path) {
        return Err("Credential files cannot be read by file tools.".into());
    }
    Ok(path)
}
fn transport<T>(
    config: &SshConfig,
    cancel: Arc<AtomicBool>,
    password: Option<crate::credentials::Secret>,
    hosts: Option<&std::path::Path>,
    operation: impl FnOnce(
        &mut std::process::ChildStdin,
        &mut std::process::ChildStdout,
    ) -> Result<T, String>,
) -> Result<T, String> {
    transport_monitored(
        config,
        cancel,
        password,
        hosts,
        Arc::new(AtomicU64::new(1)),
        operation,
    )
}
fn transport_monitored<T>(
    config: &SshConfig,
    cancel: Arc<AtomicBool>,
    password: Option<crate::credentials::Secret>,
    hosts: Option<&std::path::Path>,
    active: Arc<AtomicU64>,
    operation: impl FnOnce(
        &mut std::process::ChildStdin,
        &mut std::process::ChildStdout,
    ) -> Result<T, String>,
) -> Result<T, String> {
    config.validate()?;
    let mut command = Command::new("ssh");
    if let Some(hosts) = hosts {
        command.args(["-o", &format!("UserKnownHostsFile={}", hosts.display())]);
    }
    command.args([
        "-T",
        "-o",
        if password.is_some() {
            "BatchMode=no"
        } else {
            "BatchMode=yes"
        },
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=4",
        "-p",
        &config.port.to_string(),
    ]);
    let _askpass = if let Some(password) = password {
        let broker = crate::credentials::Askpass::new(password)?;
        command
            .env(
                "SSH_ASKPASS",
                std::env::current_exe().map_err(|e| e.to_string())?,
            )
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", "xpeed:0")
            .env("XPEED_ASKPASS_PORT", broker.port.to_string())
            .env("XPEED_ASKPASS_TOKEN", &broker.token)
            .args([
                "-o",
                "NumberOfPasswordPrompts=1",
                "-o",
                "StrictHostKeyChecking=yes",
            ]);
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
        .args(["-s", "--", &config.host, "sftp"])
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
            "No se pudo iniciar OpenSSH: {e}",
            &[("e", format!("{}", e))],
        )
    })?;
    let mut input = child
        .stdin
        .take()
        .ok_or(xpeed_i18n::t("No se pudo abrir SFTP"))?;
    let mut output = child
        .stdout
        .take()
        .ok_or(xpeed_i18n::t("No se pudo leer SFTP"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or(xpeed_i18n::t("No se pudo leer el diagnóstico SSH"))?;
    let error_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let process = Arc::new(Mutex::new(child));
    let finished = Arc::new(AtomicBool::new(false));
    let timeout_process = process.clone();
    let timeout_finished = finished.clone();
    std::thread::spawn(move || {
        let mut ticks = 0;
        let mut previous = 0;
        loop {
            if timeout_finished.load(Ordering::Acquire) {
                return;
            }
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let current = active.load(Ordering::Acquire);
            ticks = if current == 0 || current != previous {
                0
            } else {
                ticks + 1
            };
            previous = current;
            if ticks >= 100 {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        if !timeout_finished.load(Ordering::Acquire) {
            let _ = timeout_process.lock().unwrap().kill();
        }
    });
    let result = operation(&mut input, &mut output);
    finished.store(true, Ordering::Release);
    {
        let mut child = process.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
    let diagnostic = error_reader.join().unwrap_or_default();
    result.map_err(|error: String| {
        if diagnostic.trim().is_empty() {
            error
        } else {
            diagnostic.trim().to_string()
        }
    })
}
pub fn smoke_test(
    config: &SshConfig,
    password: crate::credentials::Secret,
    hosts: &std::path::Path,
) -> Result<(), String> {
    let listing = list_with_hosts(
        config,
        ".",
        Arc::new(AtomicBool::new(false)),
        Some(password.clone()),
        Some(hosts),
    )?;
    if !listing
        .entries
        .iter()
        .any(|e| e.name == "carpeta ñ" && e.directory)
        || !listing
            .entries
            .iter()
            .any(|e| e.name == "archivo con espacios.txt" && !e.directory)
    {
        return Err("No se obtuvo el listado SFTP esperado".into());
    }
    let child = list_with_hosts(
        config,
        "/carpeta ñ",
        Arc::new(AtomicBool::new(false)),
        Some(password.clone()),
        Some(hosts),
    )?;
    if !child.entries.iter().any(|e| e.name == "hijo.txt") {
        return Err("No se pudo cargar la carpeta hija".into());
    }
    // Reuse one authenticated channel, including an idle interval longer than the
    // operation timeout. Switching to another app must not expire an idle listing channel.
    let worker = listing_worker(config.clone(), Some(password), Some(hosts.to_path_buf()));
    let read = |path: &str| -> Result<Listing, String> {
        let (tx, rx) = mpsc::channel();
        worker
            .tx
            .send(ListRequest {
                path: path.into(),
                reply: tx,
            })
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(Duration::from_secs(25))
            .map_err(|e| e.to_string())?
    };
    let first = read(".")?;
    if !first.entries.iter().any(|e| e.name == "carpeta ñ") {
        return Err("Falló el primer listado persistente".into());
    }
    std::thread::sleep(Duration::from_secs(22));
    let second = read("/carpeta ñ")?;
    if !second.entries.iter().any(|e| e.name == "hijo.txt") {
        return Err("SFTP no conservó la sesión durante la pausa".into());
    }
    drop(worker);
    if list_with_hosts(
        config,
        ".",
        Arc::new(AtomicBool::new(false)),
        Some(crate::credentials::Secret::new(
            "wrong-test-password".into(),
        )),
        Some(hosts),
    )
    .is_ok()
    {
        return Err("SFTP aceptó una contraseña incorrecta".into());
    }
    Ok(())
}
fn list_streams(
    input: &mut impl Write,
    output: &mut impl Read,
    path: &str,
) -> Result<Listing, String> {
    init_streams(input, output)?;
    list_directory_streams(input, output, path)
}
fn init_streams(input: &mut impl Write, output: &mut impl Read) -> Result<(), String> {
    write_packet(input, &[1, 0, 0, 0, 3])?;
    let version = read_packet(output)?;
    if version.len() < 5 || version.first() != Some(&2) || version[1..5] != 3u32.to_be_bytes() {
        return Err(xpeed_i18n::t("El servidor no respondió al protocolo SFTP.").into());
    }
    Ok(())
}
fn list_directory_streams(
    mut input: &mut impl Write,
    mut output: &mut impl Read,
    path: &str,
) -> Result<Listing, String> {
    let reply = request(&mut input, &mut output, 16, 1, &string(path.as_bytes()))?;
    let mut cursor = Cursor::new(&reply[5..]);
    if reply[0] != 104 {
        return Err(status_error(&reply));
    }
    if cursor.u32()? == 0 {
        return Err(xpeed_i18n::t("SFTP no devolvió la ruta.").into());
    }
    let canonical = String::from_utf8_lossy(cursor.string()?).to_string();
    let reply = request(
        &mut input,
        &mut output,
        11,
        2,
        &string(canonical.as_bytes()),
    )?;
    if reply[0] != 102 {
        return Err(status_error(&reply));
    }
    let handle = Cursor::new(&reply[5..]).string()?.to_vec();
    let mut entries = Vec::new();
    for id in 3..1000 {
        let reply = request(&mut input, &mut output, 12, id, &string(&handle))?;
        if reply[0] == 101 {
            if Cursor::new(&reply[5..]).u32()? == 1 {
                break;
            }
            return Err(status_error(&reply));
        }
        if reply[0] != 104 {
            return Err(xpeed_i18n::t("Respuesta SFTP inesperada.").into());
        }
        entries.extend(parse_names(&reply[5..])?);
        if entries.len() > 10_000 {
            return Err(xpeed_i18n::t("La carpeta contiene más de 10000 entradas.").into());
        }
    }
    let _ = request(&mut input, &mut output, 4, 1001, &string(&handle));
    entries.retain(|e| e.name != "." && e.name != "..");
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(Listing {
        path: canonical,
        entries,
    })
}
fn string(bytes: &[u8]) -> Vec<u8> {
    let mut data = (bytes.len() as u32).to_be_bytes().to_vec();
    data.extend(bytes);
    data
}
fn write_packet(writer: &mut impl Write, data: &[u8]) -> Result<(), String> {
    writer
        .write_all(&(data.len() as u32).to_be_bytes())
        .and_then(|_| writer.write_all(data))
        .and_then(|_| writer.flush())
        .map_err(|e| e.to_string())
}
fn read_packet(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut length = [0; 4];
    reader.read_exact(&mut length).map_err(|e| e.to_string())?;
    let size = u32::from_be_bytes(length) as usize;
    if !(5..=16_777_216).contains(&size) {
        return Err(xpeed_i18n::t("Paquete SFTP inválido.").into());
    }
    let mut packet = vec![0; size];
    reader.read_exact(&mut packet).map_err(|e| e.to_string())?;
    Ok(packet)
}
fn request(
    writer: &mut impl Write,
    reader: &mut impl Read,
    kind: u8,
    id: u32,
    body: &[u8],
) -> Result<Vec<u8>, String> {
    let mut data = vec![kind];
    data.extend(id.to_be_bytes());
    data.extend(body);
    write_packet(writer, &data)?;
    let reply = read_packet(reader)?;
    if reply[1..5] != id.to_be_bytes() {
        return Err(xpeed_i18n::t("Identificador de respuesta SFTP inválido.").into());
    }
    Ok(reply)
}
fn status_error(reply: &[u8]) -> String {
    let mut cursor = Cursor::new(&reply[5..]);
    let _ = cursor.u32();
    cursor
        .string()
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_else(|_| xpeed_i18n::t("El servidor rechazó la operación SFTP.").into())
}
struct Cursor<'a> {
    data: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }
    fn bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        let data = self
            .data
            .get(self.position..self.position.saturating_add(count))
            .ok_or(xpeed_i18n::t("Paquete SFTP incompleto"))?;
        self.position += count;
        Ok(data)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_be_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn string(&mut self) -> Result<&'a [u8], String> {
        let size = self.u32()? as usize;
        self.bytes(size)
    }
}
fn parse_names(data: &[u8]) -> Result<Vec<Entry>, String> {
    let mut cursor = Cursor::new(data);
    let count = cursor.u32()?;
    if count > 10_000 {
        return Err(xpeed_i18n::t("Demasiadas entradas SFTP").into());
    }
    let mut entries = Vec::new();
    for _ in 0..count {
        let name = String::from_utf8_lossy(cursor.string()?).to_string();
        let _ = cursor.string()?;
        let flags = cursor.u32()?;
        let bytes = if flags & 1 != 0 {
            u64::from_be_bytes(cursor.bytes(8)?.try_into().unwrap())
        } else {
            0
        };
        if flags & 2 != 0 {
            cursor.bytes(8)?;
        }
        let permissions = if flags & 4 != 0 { cursor.u32()? } else { 0 };
        if flags & 8 != 0 {
            cursor.bytes(8)?;
        }
        if flags & 0x80000000 != 0 {
            let count = cursor.u32()?;
            for _ in 0..count {
                cursor.string()?;
                cursor.string()?;
            }
        }
        entries.push(Entry {
            name,
            directory: permissions & 0o170000 == 0o040000,
            bytes,
        });
    }
    Ok(entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Isolated localhost fixture started by scripts/test-session-tools.py"]
    fn interactive_sftp_transfers_and_editor_conflicts() {
        let data: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("XPEED_TOOL_TEST_CONFIG").unwrap()).unwrap(),
        )
        .unwrap();
        let config: SshConfig = serde_json::from_value(data[0]["connection"].clone()).unwrap();
        let hosts = std::path::PathBuf::from(data[0]["knownHosts"].as_str().unwrap());
        let root =
            std::env::temp_dir().join(format!("xpeed-remote-transfer-{}", rand::random::<u64>()));
        std::fs::create_dir(&root).unwrap();
        let upload = root.join("upload.bin");
        let download = root.join("download.bin");
        let bytes: Vec<_> = (0..262144).map(|n| (n % 256) as u8).collect();
        std::fs::write(&upload, &bytes).unwrap();
        let progress = AtomicU64::new(0);
        let started = std::time::Instant::now();
        transport_monitored(
            &config,
            Arc::new(AtomicBool::new(false)),
            None,
            Some(&hosts),
            Arc::new(AtomicU64::new(1)),
            |input, output| {
                init_streams(input, output)?;
                let heartbeat = AtomicU64::new(1);
                transfer_streams(
                    input,
                    output,
                    &upload,
                    "/test/upload.bin",
                    false,
                    &AtomicBool::new(false),
                    &progress,
                    &heartbeat,
                )?;
                transfer_streams(
                    input,
                    output,
                    &download,
                    "/test/upload.bin",
                    true,
                    &AtomicBool::new(false),
                    &progress,
                    &heartbeat,
                )?;
                assert!(
                    transfer_streams(
                        input,
                        output,
                        &upload,
                        "/test/upload.bin",
                        false,
                        &AtomicBool::new(false),
                        &progress,
                        &heartbeat
                    )
                    .is_err()
                );
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(download).unwrap(), bytes);
        let editor_path = "/test/editor.txt";
        transport(
            &config,
            Arc::new(AtomicBool::new(false)),
            None,
            Some(&hosts),
            |input, output| mutate_streams(input, output, &[Mutation::File(editor_path.into())]),
        )
        .unwrap();
        tool_replace(
            &config,
            None,
            Arc::new(AtomicBool::new(false)),
            editor_path,
            "",
            "first\n",
            Some(&hosts),
        )
        .unwrap();
        assert!(
            tool_replace(
                &config,
                None,
                Arc::new(AtomicBool::new(false)),
                editor_path,
                "",
                "stale\n",
                Some(&hosts)
            )
            .is_err()
        );
        assert_eq!(
            tool_read(
                &config,
                None,
                Arc::new(AtomicBool::new(false)),
                editor_path,
                Some(&hosts)
            )
            .unwrap(),
            "first\n"
        );
        eprintln!(
            "SFTP fixture: 256 KiB upload + download + collision + editor conflict in {:?}",
            started.elapsed()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn transfer_cancel_button_fits_panel_and_cancels_active_operation() {
        let ctx = eframe::egui::Context::default();
        let mut remote = RemoteExplorer::preview();
        let (_tx, rx) = mpsc::channel();
        remote.operation = Some(rx);
        let cancel = Arc::new(AtomicBool::new(false));
        remote.operation_cancel = Some(cancel.clone());
        remote.transfer_total = 100;
        remote.transferred.store(50, Ordering::Release);
        let rect = eframe::egui::Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(300., 620.),
        );
        let mut pos = eframe::egui::Pos2::ZERO;
        for _ in 0..2 {
            let output = ctx.run(
                eframe::egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ctx| {
                    eframe::egui::CentralPanel::default().show(ctx, |ui| {
                        crate::file_explorer::show(
                            ui,
                            &mut remote,
                            "fixture",
                            1,
                            Default::default(),
                        )
                    });
                },
            );
            if let Some(point) = output.shapes.iter().find_map(|shape| match &shape.shape {
                eframe::egui::Shape::Text(text)
                    if text.galley.job.text == xpeed_i18n::t("Cancelar") =>
                {
                    Some(text.pos + eframe::egui::vec2(5., 5.))
                }
                _ => None,
            }) {
                pos = point;
            }
        }
        assert!(
            pos.x > 0. && rect.contains(pos),
            "Cancel must fit inside the panel: {pos:?}"
        );
        for pressed in [true, false] {
            let _ = ctx.run(
                eframe::egui::RawInput {
                    screen_rect: Some(rect),
                    events: vec![
                        eframe::egui::Event::PointerMoved(pos),
                        eframe::egui::Event::PointerButton {
                            pos,
                            button: eframe::egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    eframe::egui::CentralPanel::default().show(ctx, |ui| {
                        crate::file_explorer::show(
                            ui,
                            &mut remote,
                            "fixture",
                            1,
                            Default::default(),
                        )
                    });
                },
            );
        }
        assert!(cancel.load(Ordering::Acquire));
    }
    #[test]
    fn navigation_cache_reuses_recent_listing_and_refresh_invalidates() {
        let mut remote = RemoteExplorer::preview();
        remote.cache.insert(
            "/cached".into(),
            (
                std::time::Instant::now(),
                Listing {
                    path: "/cached".into(),
                    entries: vec![Entry {
                        name: "fresh.txt".into(),
                        directory: false,
                        bytes: 3,
                    }],
                },
            ),
        );
        remote.navigate("/cached".into());
        assert_eq!(remote.entries[0].name, "fresh.txt");
        assert!(!remote.loading);
        assert!(remote.worker.is_none());
        remote.cache.get_mut("/cached").unwrap().0 -= Duration::from_secs(30);
        assert!(remote.cache["/cached"].0.elapsed() >= Duration::from_secs(15));
    }
    #[test]
    fn streaming_download_upload_progress_and_collision() {
        let root = std::env::temp_dir().join(format!("xpeed-transfer-{}", rand::random::<u64>()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("payload.bin");
        let cancel = AtomicBool::new(false);
        let progress = AtomicU64::new(0);
        let heartbeat = AtomicU64::new(1);
        let mut wire = Vec::new();
        let mut handle = vec![102];
        handle.extend(1u32.to_be_bytes());
        handle.extend(string(b"handle"));
        write_packet(&mut wire, &handle).unwrap();
        let mut payload = vec![103];
        payload.extend(2u32.to_be_bytes());
        payload.extend(string(&[0, 1, 255, 10]));
        write_packet(&mut wire, &payload).unwrap();
        status(&mut wire, 3, 1, "EOF");
        status(&mut wire, u32::MAX - 2, 0, "closed");
        transfer_streams(
            &mut Vec::new(),
            &mut wire.as_slice(),
            &path,
            "/test",
            true,
            &cancel,
            &progress,
            &heartbeat,
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), [0, 1, 255, 10]);
        assert_eq!(progress.load(Ordering::Acquire), 4);
        assert!(
            transfer_streams(
                &mut Vec::new(),
                &mut [].as_slice(),
                &path,
                "/test",
                true,
                &cancel,
                &progress,
                &heartbeat
            )
            .is_err()
        );
        let mut wire = Vec::new();
        write_packet(&mut wire, &handle).unwrap();
        status(&mut wire, 2, 0, "written");
        status(&mut wire, u32::MAX - 2, 0, "closed");
        status(&mut wire, u32::MAX - 1, 0, "renamed");
        let mut request_bytes = Vec::new();
        transfer_streams(
            &mut request_bytes,
            &mut wire.as_slice(),
            &path,
            "/test",
            false,
            &cancel,
            &progress,
            &heartbeat,
        )
        .unwrap();
        assert!(request_bytes.windows(4).any(|w| w == [0, 1, 255, 10]));
        assert_eq!(progress.load(Ordering::Acquire), 8);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_download_cleans_temporary_without_replacing_destination() {
        let path = std::env::temp_dir().join(format!("xpeed-cancelled-{}", rand::random::<u64>()));
        let mut wire = Vec::new();
        let mut handle = vec![102];
        handle.extend(1u32.to_be_bytes());
        handle.extend(string(b"handle"));
        write_packet(&mut wire, &handle).unwrap();
        status(&mut wire, u32::MAX - 2, 0, "closed");
        assert!(
            transfer_streams(
                &mut Vec::new(),
                &mut wire.as_slice(),
                &path,
                "/test",
                true,
                &AtomicBool::new(true),
                &AtomicU64::new(0),
                &AtomicU64::new(1)
            )
            .is_err()
        );
        assert!(!path.exists());
    }
    #[test]
    fn terminal_directory_changes_and_listing_results_keep_grid_mode() {
        let mut remote = RemoteExplorer::idle(SshConfig::default());
        remote.home = Some("/root".into());
        remote.path = "/root".into();
        remote.browser.grid = true;
        remote.follow("/root/src");
        assert_eq!(remote.path, "/root/src");
        assert!(remote.browser.grid);
        let (tx, rx) = mpsc::channel();
        remote.result = Some(rx);
        tx.send(Ok(Listing {
            path: "/root/src".into(),
            entries: vec![],
        }))
        .unwrap();
        remote.poll();
        assert!(remote.browser.grid);
        remote.go_back();
        assert_eq!(remote.path, "/root");
        assert!(remote.browser.grid);
    }
    #[test]
    fn back_navigation_only_appears_after_directory_changes() {
        // Empty host prevents any transport from starting in this isolated test.
        let mut remote = RemoteExplorer::idle(SshConfig::default());
        remote.path = "/root".into();
        remote.home = Some("/root".into());
        assert!(remote.at_home());
        assert!(!remote.can_go_back());
        remote.navigate("/root/subfolder".into());
        assert!(!remote.at_home());
        assert!(remote.can_go_back());
        remote.navigate("/root/subfolder".into());
        assert_eq!(remote.back.len(), 1);
        remote.go_back();
        assert_eq!(remote.path, "/root");
        assert!(remote.at_home());
        assert!(!remote.can_go_back());
    }
    fn status(wire: &mut Vec<u8>, id: u32, code: u32, message: &str) {
        let mut packet = vec![101];
        packet.extend(id.to_be_bytes());
        packet.extend(code.to_be_bytes());
        packet.extend(string(message.as_bytes()));
        packet.extend(string(b""));
        write_packet(wire, &packet).unwrap();
    }
    #[test]
    fn creates_exclusively_and_uses_native_sftp_mutations() {
        let mut incoming = Vec::new();
        write_packet(&mut incoming, &[2, 0, 0, 0, 3]).unwrap();
        let mut handle = vec![102];
        handle.extend(1u32.to_be_bytes());
        handle.extend(string(b"file-handle"));
        write_packet(&mut incoming, &handle).unwrap();
        for id in 2..=6 {
            status(&mut incoming, id, 0, "");
        }
        let operations = [
            Mutation::File("/mi archivo ñ".into()),
            Mutation::Folder("/folder".into()),
            Mutation::Rename("/folder".into(), "/renamed".into()),
            Mutation::Remove("/mi archivo ñ".into(), false),
            Mutation::Remove("/renamed".into(), true),
        ];
        let mut outgoing = Vec::new();
        mutate_streams(&mut outgoing, &mut incoming.as_slice(), &operations).unwrap();
        let mut outgoing = outgoing.as_slice();
        assert_eq!(read_packet(&mut outgoing).unwrap(), [1, 0, 0, 0, 3]);
        let open = read_packet(&mut outgoing).unwrap();
        assert_eq!(open[0], 3);
        let mut cursor = Cursor::new(&open[5..]);
        assert_eq!(cursor.string().unwrap(), "/mi archivo ñ".as_bytes());
        assert_eq!(cursor.u32().unwrap(), 42);
        for kind in [4, 14, 18, 13, 15] {
            assert_eq!(read_packet(&mut outgoing).unwrap()[0], kind);
        }
        assert!(outgoing.is_empty());
    }
    #[test]
    fn mutation_stops_on_error_without_sending_remaining_operations() {
        let mut incoming = Vec::new();
        write_packet(&mut incoming, &[2, 0, 0, 0, 3]).unwrap();
        status(&mut incoming, 1, 4, "Directory not empty");
        let mut outgoing = Vec::new();
        let error = mutate_streams(
            &mut outgoing,
            &mut incoming.as_slice(),
            &[
                Mutation::Remove("/folder".into(), true),
                Mutation::Remove("/next".into(), false),
            ],
        )
        .unwrap_err();
        assert!(error.contains("Directory not empty"));
        let mut outgoing = outgoing.as_slice();
        read_packet(&mut outgoing).unwrap();
        assert_eq!(read_packet(&mut outgoing).unwrap()[0], 15);
        assert!(outgoing.is_empty());
    }
    #[test]
    fn directory_navigation_performs_the_sftp_v3_exchange() {
        let mut wire = Vec::new();
        write_packet(&mut wire, &[2, 0, 0, 0, 3]).unwrap();
        let mut canonical = vec![104];
        canonical.extend(1u32.to_be_bytes());
        canonical.extend(1u32.to_be_bytes());
        canonical.extend(string(b"/home/test"));
        canonical.extend(string(b""));
        canonical.extend(0u32.to_be_bytes());
        write_packet(&mut wire, &canonical).unwrap();
        let mut handle = vec![102];
        handle.extend(2u32.to_be_bytes());
        handle.extend(string(b"directory-handle"));
        write_packet(&mut wire, &handle).unwrap();
        let mut names = vec![104];
        names.extend(3u32.to_be_bytes());
        names.extend(1u32.to_be_bytes());
        names.extend(string("mi carpeta ñ".as_bytes()));
        names.extend(string(b"long name"));
        names.extend(4u32.to_be_bytes());
        names.extend(0o040755u32.to_be_bytes());
        write_packet(&mut wire, &names).unwrap();
        for (id, status) in [(4u32, 1u32), (1001, 0)] {
            let mut reply = vec![101];
            reply.extend(id.to_be_bytes());
            reply.extend(status.to_be_bytes());
            reply.extend(string(b""));
            reply.extend(string(b""));
            write_packet(&mut wire, &reply).unwrap();
        }
        let mut outgoing = Vec::new();
        let listing = list_streams(&mut outgoing, &mut wire.as_slice(), ".").unwrap();
        assert_eq!(listing.path, "/home/test");
        assert_eq!(listing.entries[0].name, "mi carpeta ñ");
        assert!(listing.entries[0].directory);
        let mut sent = outgoing.as_slice();
        let mut types = Vec::new();
        while !sent.is_empty() {
            types.push(read_packet(&mut sent).unwrap()[0]);
        }
        assert_eq!(types, vec![1, 16, 11, 12, 12, 4]);
    }
    #[test]
    fn names_support_spaces_unicode_and_protocol_attributes() {
        let mut data = 1u32.to_be_bytes().to_vec();
        data.extend(string("mi carpeta ñ".as_bytes()));
        data.extend(string(b"ignored long listing"));
        data.extend(5u32.to_be_bytes());
        data.extend(123u64.to_be_bytes());
        data.extend(0o040755u32.to_be_bytes());
        let entries = parse_names(&data).unwrap();
        assert_eq!(entries[0].name, "mi carpeta ñ");
        assert!(entries[0].directory);
        assert_eq!(entries[0].bytes, 123);
        assert!(parse_names(&data[..data.len() - 1]).is_err());
    }
    #[test]
    fn packets_reject_invalid_lengths_and_ids() {
        assert!(read_packet(&mut &b"\0\0\0\0"[..]).is_err());
        let mut reply = Vec::new();
        write_packet(&mut reply, &[101, 0, 0, 0, 9, 0, 0, 0, 1]).unwrap();
        assert!(request(&mut Vec::new(), &mut reply.as_slice(), 12, 3, b"").is_err());
    }
}

fn resolve_directory(directory: &str, home: Option<&str>) -> Option<String> {
    if directory.starts_with('/') {
        return Some(directory.into());
    }
    if directory == "~" {
        return home.map(str::to_owned);
    }
    directory
        .strip_prefix("~/")
        .and_then(|suffix| home.map(|home| format!("{}/{suffix}", home.trim_end_matches('/'))))
}
#[cfg(test)]
mod directory_tests {
    use super::*;
    #[test]
    fn home_paths_wait_for_real_sftp_home() {
        assert_eq!(resolve_directory("~/a", None), None);
        assert_eq!(
            resolve_directory("~", Some("/home/user")),
            Some("/home/user".into())
        );
        assert_eq!(
            resolve_directory("~/carpeta ñ", Some("/home/user")),
            Some("/home/user/carpeta ñ".into())
        );
        assert_eq!(resolve_directory("/", None), Some("/".into()));
        assert_eq!(resolve_directory("folder", Some("/home/user")), None);
    }
}
