use std::{
    env, fmt, fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use rake_config::RakeConfig;
use url::Url;

mod build;
pub use build::{BuildOptions, BuildResult, BuildStep, build};
mod config_file;
pub use config_file::{load_config, save_config};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as host;
#[cfg(target_os = "macos")]
pub use macos::{ICON_FILE, assemble, install, package_dmg, prepare_icon, sign};
#[cfg(any(target_os = "linux", test))]
mod linux;
#[cfg(target_os = "linux")]
use linux as host;
#[cfg(target_os = "linux")]
pub use linux::assemble;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Target {
    #[default]
    App,
    Dmg,
    Tarball,
    Deb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Macos,
    Linux,
}

impl Platform {
    pub fn current() -> Result<Self, Error> {
        Ok(TemplateTarget::current()?.platform())
    }

    pub fn template_name(self) -> &'static str {
        match self {
            Self::Macos => "Rake.app",
            Self::Linux => "Rake",
        }
    }

    fn complete_bundle(self, path: &Path) -> bool {
        match self {
            Self::Macos => {
                path.join("Contents/MacOS/rake-runtime").is_file()
                    && path.join("Contents/Info.plist").is_file()
            }
            Self::Linux => path.join("rake-runtime").is_file() && path.join("icon.png").is_file(),
        }
    }

    fn validate(self, target: Target, install: bool) -> Result<(), Error> {
        if !matches!(
            (self, target),
            (_, Target::App)
                | (Self::Macos, Target::Dmg)
                | (Self::Linux, Target::Tarball | Target::Deb)
        ) {
            return Err(Error::InvalidInput(format!(
                "{target:?} output is not supported on {self:?}"
            )));
        }
        if self == Self::Linux && install {
            return Err(Error::InvalidInput(
                "--install is macOS-only; install the .deb with apt instead".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Icon {
    Path(PathBuf),
    Url(String),
}

#[derive(Debug)]
pub struct AssemblyResult {
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateTarget {
    MacosArm64,
    MacosX86_64,
    MacosUniversal,
    LinuxArm64,
    LinuxX86_64,
}

impl TemplateTarget {
    pub fn current() -> Result<Self, Error> {
        Self::resolve(env::consts::OS, env::consts::ARCH)
    }

    pub fn resolve(os: &str, arch: &str) -> Result<Self, Error> {
        match (os, arch) {
            ("macos", "aarch64") => Ok(Self::MacosArm64),
            ("macos", "x86_64") => Ok(Self::MacosX86_64),
            ("macos", "universal") => Ok(Self::MacosUniversal),
            ("linux", "aarch64" | "arm64") => Ok(Self::LinuxArm64),
            ("linux", "x86_64") => Ok(Self::LinuxX86_64),
            ("macos", "arm64") => Ok(Self::MacosArm64),
            _ => Err(Error::UnsupportedTarget(format!("{os}-{arch}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MacosArm64 => "macos-arm64",
            Self::MacosX86_64 => "macos-x86_64",
            Self::MacosUniversal => "macos-universal",
            Self::LinuxArm64 => "linux-arm64",
            Self::LinuxX86_64 => "linux-x86_64",
        }
    }

    pub fn platform(self) -> Platform {
        match self {
            Self::MacosArm64 | Self::MacosX86_64 | Self::MacosUniversal => Platform::Macos,
            Self::LinuxArm64 | Self::LinuxX86_64 => Platform::Linux,
        }
    }
}

#[derive(Debug)]
pub struct TemplateOptions {
    pub target: Option<TemplateTarget>,
    pub refresh: bool,
    /// Rake's cache root, before templates/<version>/<target>.
    pub cache_root: Option<PathBuf>,
    /// Release download root; /v<version>/<asset> is appended.
    pub base_url: String,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            target: None,
            refresh: false,
            cache_root: None,
            base_url: "https://github.com/SanderSpiegelaar/rake/releases/download".into(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Unexpected(String),
    MissingEnvironment(String),
    MissingTemplate(String),
    UnsupportedTarget(String),
    MissingCacheDirectory,
    Io(io::Error),
    Command {
        program: &'static str,
        detail: String,
    },
    InvalidTemplate(String),
    ChecksumMismatch,
    InvalidInput(String),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Unexpected(_) => 1,
            Self::InvalidInput(_) => 2,
            Self::Io(_) | Self::Command { .. } => 3,
            Self::MissingEnvironment(_)
            | Self::MissingTemplate(_)
            | Self::UnsupportedTarget(_)
            | Self::MissingCacheDirectory
            | Self::InvalidTemplate(_)
            | Self::ChecksumMismatch => 4,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unexpected(detail) => write!(f, "{detail}"),
            Self::MissingEnvironment(detail) => write!(f, "missing environment: {detail}"),
            Self::MissingTemplate(detail) => write!(f, "template unavailable: {detail}"),
            Self::UnsupportedTarget(target) => write!(f, "unsupported template target: {target}"),
            Self::MissingCacheDirectory => write!(f, "cannot find the user cache directory"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Command { program, detail } => write!(f, "{program}: {detail}"),
            Self::InvalidTemplate(detail) => write!(f, "invalid template: {detail}"),
            Self::ChecksumMismatch => write!(f, "template checksum mismatch"),
            Self::InvalidInput(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn fetch_icon(source: &str, destination: &Path) -> Result<String, Error> {
    let url = Url::parse(source)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .ok_or_else(|| Error::InvalidInput("icon: expected an HTTP or HTTPS URL".into()))?;
    run(Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--connect-timeout",
            "3",
            "--max-time",
            "5",
            "--max-filesize",
            "5242880",
            "--write-out",
            "%{url_effective}",
            "--output",
        ])
        .arg(destination)
        .arg(url.as_str()))
}

fn icon_candidates(page: &str, html: &str) -> Vec<String> {
    let Ok(base) = Url::parse(page) else {
        return Vec::new();
    };
    let mut touch = Vec::new();
    let mut icons = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if rest.starts_with("!--") {
            rest = rest.split_once("-->").map_or("", |(_, tail)| tail);
            continue;
        }
        let mut quote = None;
        let Some(end) = rest.find(|c| {
            if matches!(c, '\'' | '"') {
                if quote == Some(c) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(c);
                }
            }
            c == '>' && quote.is_none()
        }) else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        if !tag[..name_end].eq_ignore_ascii_case("link") {
            continue;
        }
        let attributes = link_attributes(&tag[name_end..]);
        let Some(href) = attributes.iter().find(|(key, _)| key == "href") else {
            continue;
        };
        let Some((_, rel)) = attributes.iter().find(|(key, _)| key == "rel") else {
            continue;
        };
        let Ok(url) = base.join(&href.1.replace("&amp;", "&")) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        if rel
            .split_ascii_whitespace()
            .any(|rel| rel.eq_ignore_ascii_case("apple-touch-icon"))
        {
            touch.push(url.to_string());
        } else if rel
            .split_ascii_whitespace()
            .any(|rel| rel.eq_ignore_ascii_case("icon"))
        {
            icons.push(url.to_string());
        }
    }
    touch.extend(icons);
    if let Ok(favicon) = base.join("/favicon.ico") {
        touch.push(favicon.to_string());
    }
    let mut candidates = Vec::new();
    for candidate in touch {
        if !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

fn link_attributes(mut text: &str) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    while !text.is_empty() {
        text = text.trim_start();
        let end = text
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '/'))
            .unwrap_or(text.len());
        if end == 0 {
            break;
        }
        let key = text[..end].to_ascii_lowercase();
        text = text[end..].trim_start();
        if !text.starts_with('=') {
            continue;
        }
        text = text[1..].trim_start();
        let value;
        if let Some(quote) = text.chars().next().filter(|c| matches!(c, '\'' | '"')) {
            text = &text[1..];
            let end = text.find(quote).unwrap_or(text.len());
            value = text[..end].to_owned();
            text = text.get(end + 1..).unwrap_or("");
        } else {
            let end = text.find(char::is_whitespace).unwrap_or(text.len());
            value = text[..end].to_owned();
            text = &text[end..];
        }
        attributes.push((key, value));
    }
    attributes
}

/// Fetch and verify the version-matched template, or reuse the complete cached bundle.
pub fn ensure_template(options: &TemplateOptions) -> Result<PathBuf, Error> {
    let target = match options.target {
        Some(target) => target,
        None => TemplateTarget::current()?,
    };
    ensure_template_for(options, target)
}

fn ensure_template_for(
    options: &TemplateOptions,
    target: TemplateTarget,
) -> Result<PathBuf, Error> {
    let root = match &options.cache_root {
        Some(root) => root.clone(),
        None => cache_root()?,
    };
    let directory = root
        .join("templates")
        .join(env!("CARGO_PKG_VERSION"))
        .join(target.as_str());
    let platform = target.platform();
    let bundle = directory.join(platform.template_name());
    if !options.refresh && platform.complete_bundle(&bundle) {
        return Ok(bundle);
    }
    fs::create_dir_all(&directory)?;
    let staging = Staging::new(&directory)?;
    let asset = format!(
        "rake-template-{}-{}.tar.gz",
        env!("CARGO_PKG_VERSION"),
        target.as_str()
    );
    let release = format!(
        "{}/v{}",
        options.base_url.trim_end_matches('/'),
        env!("CARGO_PKG_VERSION")
    );
    let archive = staging.0.join(&asset);
    let sums = staging.0.join("SHA256SUMS");
    download(&format!("{release}/{asset}"), &archive)?;
    download(&format!("{release}/SHA256SUMS"), &sums)?;
    let sums = fs::read_to_string(sums)?;
    let expected = sums
        .lines()
        .filter_map(|line| line.split_once("  "))
        .find(|(_, filename)| *filename == asset)
        .map(|(hash, _)| hash)
        .filter(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| Error::InvalidTemplate("missing or invalid checksum".into()))?;
    let actual = checksum(&archive)?;
    if actual.split_whitespace().next() != Some(expected) {
        return Err(Error::ChecksumMismatch);
    }
    let entries = run(Command::new("tar").arg("-tzf").arg(&archive))?;
    for entry in entries.lines() {
        let mut components = Path::new(entry).components();
        if components.next() != Some(Component::Normal(platform.template_name().as_ref()))
            || components.any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(Error::InvalidTemplate(format!(
                "archive must contain only {}",
                platform.template_name()
            )));
        }
    }
    let extracted = staging.0.join("extracted");
    fs::create_dir(&extracted)?;
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&extracted))?;
    let fresh = extracted.join(platform.template_name());
    if !platform.complete_bundle(&fresh) {
        return Err(Error::InvalidTemplate(
            "bundle is missing its executable or plist".into(),
        ));
    }
    // Keep the old bundle usable until the replacement has passed verification.
    let old = staging.0.join("old.app");
    if bundle.exists() {
        fs::rename(&bundle, &old)?;
    }
    if let Err(error) = fs::rename(&fresh, &bundle) {
        if old.exists() {
            fs::rename(&old, &bundle)?;
        }
        return Err(error.into());
    }
    Ok(bundle)
}

fn checksum(path: &Path) -> Result<String, Error> {
    if env::consts::OS == "macos" {
        run(Command::new("shasum").args(["-a", "256"]).arg(path))
    } else {
        run(Command::new("sha256sum").arg(path))
    }
}

fn cache_root() -> Result<PathBuf, Error> {
    if env::consts::OS == "linux"
        && let Some(root) = env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(root).join("rake"));
    }
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or(Error::MissingCacheDirectory)?;
    Ok(PathBuf::from(home)
        .join(if env::consts::OS == "macos" {
            "Library/Caches"
        } else {
            ".cache"
        })
        .join("rake"))
}

fn download(url: &str, destination: &Path) -> Result<(), Error> {
    run(Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "15",
            "--max-time",
            "300",
            "--output",
        ])
        .arg(destination)
        .arg(url))?;
    Ok(())
}

fn run(command: &mut Command) -> Result<String, Error> {
    let program = match command.get_program().to_str() {
        Some("curl") => "curl",
        Some("shasum") => "shasum",
        Some("sha256sum") => "sha256sum",
        Some("cp") => "cp",
        Some("dpkg-deb") => "dpkg-deb",
        Some("chmod") => "chmod",
        Some("gdk-pixbuf-thumbnailer") => "gdk-pixbuf-thumbnailer",
        Some("/usr/bin/ditto") => "ditto",
        Some("/usr/bin/plutil") => "plutil",
        Some("/usr/bin/codesign") => "codesign",
        Some("/usr/bin/hdiutil") => "hdiutil",
        Some("sips") => "sips",
        Some("iconutil") => "iconutil",
        _ => "tar",
    };
    let output = command.output().map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Error::MissingEnvironment(format!("{program}: {error}"))
        } else {
            Error::Command {
                program,
                detail: error.to_string(),
            }
        }
    })?;
    if !output.status.success() {
        return Err(Error::Command {
            program,
            detail: String::from_utf8_lossy(&output.stderr).trim().into(),
        });
    }
    String::from_utf8(output.stdout)
        .map_err(|_| Error::InvalidTemplate(format!("{program} returned non-UTF-8 output")))
}

struct Staging(PathBuf);

impl Staging {
    fn new(parent: &Path) -> Result<Self, io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = parent.join(format!(
                ".fetch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_config(config: &RakeConfig, resources: &Path) -> Result<(), Error> {
    fs::create_dir_all(resources.join("inject"))?;
    let mut written = config.clone();
    written.inject.clear();
    for source in &config.inject {
        let filename = source.file_name().ok_or_else(|| {
            Error::InvalidInput(format!("inject: missing file name: {}", source.display()))
        })?;
        let target = resources.join("inject").join(filename);
        if target.exists() {
            return Err(Error::InvalidInput(format!(
                "inject: duplicate file name: {}",
                filename.to_string_lossy()
            )));
        }
        fs::copy(source, target)?;
        written.inject.push(PathBuf::from(filename));
    }
    let json = serde_json::to_vec_pretty(&written)
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    fs::write(resources.join("config.json"), json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, atomic::AtomicBool},
        thread,
        time::Duration,
    };
    pub(crate) struct Server {
        pub(crate) url: String,
        requests: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl Server {
        pub(crate) fn start(root: PathBuf) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(AtomicU64::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let count = Arc::clone(&requests);
            let stopped = Arc::clone(&stop);
            let thread = thread::spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream.set_nonblocking(false).unwrap();
                            stream
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .unwrap();
                            let mut request = [0; 4096];
                            let length = stream.read(&mut request).unwrap();
                            count.fetch_add(1, Ordering::Relaxed);
                            let request = String::from_utf8_lossy(&request[..length]);
                            let path = request.split_whitespace().nth(1).unwrap();
                            let (status, body) =
                                match fs::read(root.join(path.trim_start_matches('/'))) {
                                    Ok(body) => ("200 OK", body),
                                    Err(_) => ("404 Not Found", Vec::new()),
                                };
                            write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                            stream.write_all(&body).unwrap();
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("{error}"),
                    }
                }
            });
            Self {
                url,
                requests,
                stop,
                thread: Some(thread),
            }
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            let result = self.thread.take().unwrap().join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }

    fn fixture() -> (Staging, PathBuf, String) {
        let temp = Staging::new(&env::temp_dir()).unwrap();
        let release = temp.0.join(format!("v{}", env!("CARGO_PKG_VERSION")));
        let mut sums = String::new();
        for target in [
            TemplateTarget::MacosArm64,
            TemplateTarget::MacosX86_64,
            TemplateTarget::MacosUniversal,
            TemplateTarget::LinuxArm64,
            TemplateTarget::LinuxX86_64,
        ] {
            let bundle = release.join(target.platform().template_name());
            let (executable, metadata) = fixture_paths(target.platform());
            fs::create_dir_all(bundle.join(executable).parent().unwrap()).unwrap();
            fs::write(bundle.join(executable), target.as_str()).unwrap();
            fs::write(bundle.join(metadata), "fake metadata").unwrap();
            let asset = format!(
                "rake-template-{}-{}.tar.gz",
                env!("CARGO_PKG_VERSION"),
                target.as_str()
            );
            run(Command::new("tar")
                .current_dir(&release)
                .env("COPYFILE_DISABLE", "1")
                .args(["-czf", &asset, target.platform().template_name()]))
            .unwrap();
            let hash = checksum(&release.join(&asset)).unwrap();
            sums.push_str(&format!(
                "{}  {asset}\n",
                hash.split_whitespace().next().unwrap()
            ));
        }
        fs::write(release.join("SHA256SUMS"), sums).unwrap();
        let asset = format!(
            "rake-template-{}-{}.tar.gz",
            env!("CARGO_PKG_VERSION"),
            TemplateTarget::current().unwrap().as_str()
        );
        (temp, release, asset)
    }

    fn fixture_paths(platform: Platform) -> (&'static str, &'static str) {
        match platform {
            Platform::Macos => ("Contents/MacOS/rake-runtime", "Contents/Info.plist"),
            Platform::Linux => ("rake-runtime", "icon.png"),
        }
    }

    #[test]
    fn downloads_extracts_reuses_offline_and_refreshes() {
        for target in [
            None,
            Some(TemplateTarget::MacosArm64),
            Some(TemplateTarget::MacosX86_64),
            Some(TemplateTarget::MacosUniversal),
            Some(TemplateTarget::LinuxArm64),
            Some(TemplateTarget::LinuxX86_64),
        ] {
            let (temp, _, _) = fixture();
            let server = Server::start(temp.0.clone());
            let mut options = TemplateOptions {
                target,
                cache_root: Some(temp.0.join("cache")),
                base_url: server.url.clone(),
                ..Default::default()
            };
            let target = target.unwrap_or(TemplateTarget::current().unwrap());
            let (executable, metadata) = fixture_paths(target.platform());
            let bundle = ensure_template(&options).unwrap();
            assert!(bundle.ends_with(format!(
                "templates/{}/{}/{}",
                env!("CARGO_PKG_VERSION"),
                target.as_str(),
                target.platform().template_name()
            )));
            assert_eq!(
                fs::read_to_string(bundle.join(executable)).unwrap(),
                target.as_str()
            );
            assert_eq!(server.requests.load(Ordering::Relaxed), 2);
            assert_eq!(ensure_template(&options).unwrap(), bundle);
            assert_eq!(server.requests.load(Ordering::Relaxed), 2);
            fs::write(bundle.join(metadata), "changed").unwrap();
            options.refresh = true;
            assert_eq!(ensure_template(&options).unwrap(), bundle);
            assert_eq!(server.requests.load(Ordering::Relaxed), 4);
            assert_eq!(
                fs::read_to_string(bundle.join(metadata)).unwrap(),
                "fake metadata"
            );
            drop(server);
            options.refresh = false;
            assert_eq!(ensure_template(&options).unwrap(), bundle);
            assert_eq!(fs::read_dir(bundle.parent().unwrap()).unwrap().count(), 1);
        }
    }

    #[test]
    fn checksum_mismatch_deletes_download_and_returns_exit_code_four() {
        let (temp, release, asset) = fixture();
        fs::write(
            release.join("SHA256SUMS"),
            format!("{}  {asset}\n", "0".repeat(64)),
        )
        .unwrap();
        let server = Server::start(temp.0.clone());
        let options = TemplateOptions {
            cache_root: Some(temp.0.join("cache")),
            base_url: server.url.clone(),
            ..Default::default()
        };
        let error = ensure_template(&options).unwrap_err();
        assert!(matches!(error, Error::ChecksumMismatch));
        assert_eq!(error.exit_code(), 4);
        let target = temp
            .0
            .join("cache/templates")
            .join(env!("CARGO_PKG_VERSION"))
            .join(TemplateTarget::current().unwrap().as_str());
        assert_eq!(fs::read_dir(target).unwrap().count(), 0);
        assert_eq!(server.requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn failed_refresh_preserves_verified_cache() {
        let (temp, release, asset) = fixture();
        let server = Server::start(temp.0.clone());
        let mut options = TemplateOptions {
            cache_root: Some(temp.0.join("cache")),
            base_url: server.url.clone(),
            ..Default::default()
        };
        let bundle = ensure_template(&options).unwrap();
        fs::write(release.join(asset), "corrupt download").unwrap();
        options.refresh = true;
        assert!(matches!(
            ensure_template(&options),
            Err(Error::ChecksumMismatch)
        ));
        drop(server);
        options.refresh = false;
        assert_eq!(ensure_template(&options).unwrap(), bundle);
    }

    #[test]
    fn ignores_interrupted_download_and_replaces_incomplete_cache() {
        let (temp, _, _) = fixture();
        let server = Server::start(temp.0.clone());
        let target = temp
            .0
            .join("cache/templates")
            .join(env!("CARGO_PKG_VERSION"))
            .join(TemplateTarget::current().unwrap().as_str());
        let platform = Platform::current().unwrap();
        let partial = target.join(platform.template_name());
        fs::create_dir_all(&partial).unwrap();
        fs::write(partial.join("partial"), "partial").unwrap();
        fs::create_dir(target.join(".fetch-interrupted")).unwrap();
        fs::write(target.join(".fetch-interrupted/partial.tar.gz"), "partial").unwrap();
        let options = TemplateOptions {
            cache_root: Some(temp.0.join("cache")),
            base_url: server.url.clone(),
            ..Default::default()
        };
        let bundle = ensure_template(&options).unwrap();
        assert!(platform.complete_bundle(&bundle));
        assert_eq!(server.requests.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn target_resolution_rejects_unpublished_platforms() {
        assert_eq!(
            TemplateTarget::resolve("macos", "aarch64")
                .unwrap()
                .as_str(),
            "macos-arm64"
        );
        assert_eq!(
            TemplateTarget::resolve("macos", "x86_64").unwrap().as_str(),
            "macos-x86_64"
        );
        for (os, arch) in [("macos", "arm"), ("linux", "arm"), ("windows", "x86_64")] {
            assert_eq!(
                TemplateTarget::resolve(os, arch).unwrap_err().exit_code(),
                4
            );
        }
    }
}
