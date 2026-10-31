use std::{
    env, fmt, fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use rake_config::RakeConfig;
use url::Url;

pub const ICON_FILE: &str = "icon.icns";

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
}

impl TemplateTarget {
    pub fn current() -> Result<Self, Error> {
        Self::resolve(env::consts::OS, env::consts::ARCH)
    }

    fn resolve(os: &str, arch: &str) -> Result<Self, Error> {
        match (os, arch) {
            ("macos", "aarch64") => Ok(Self::MacosArm64),
            _ => Err(Error::UnsupportedTarget(format!("{os}-{arch}"))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::MacosArm64 => "macos-arm64",
        }
    }
}

#[derive(Debug)]
pub struct TemplateOptions {
    pub refresh: bool,
    /// Rake's cache root, before templates/<version>/<target>.
    pub cache_root: Option<PathBuf>,
    /// Release download root; /v<version>/<asset> is appended.
    pub base_url: String,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        Self {
            refresh: false,
            cache_root: None,
            base_url: "https://github.com/SanderSpiegelaar/rake/releases/download".into(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
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
    Assembly(String),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) => 2,
            Self::Io(_) | Self::Assembly(_) => 3,
            _ => 4,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTarget(target) => write!(f, "unsupported template target: {target}"),
            Self::MissingCacheDirectory => write!(f, "cannot find the user cache directory"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Command { program, detail } => write!(f, "{program}: {detail}"),
            Self::InvalidTemplate(detail) => write!(f, "invalid template: {detail}"),
            Self::ChecksumMismatch => write!(f, "template checksum mismatch"),
            Self::InvalidInput(detail) => write!(f, "{detail}"),
            Self::Assembly(detail) => write!(f, "app assembly: {detail}"),
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

/// Copy a template into <output_dir>/<Name>.app without modifying the template.
/// Inject paths are resolved from the caller's working directory.
pub fn assemble(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
    icon: Option<&Icon>,
) -> Result<AssemblyResult, Error> {
    config
        .validate()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    let name = config.app_name();
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
    {
        return Err(Error::InvalidInput(
            "name: must be a nonempty file name".into(),
        ));
    }
    let destination = output_dir.join(format!("{name}.app"));
    let check_destination = || -> Result<bool, Error> {
        match fs::symlink_metadata(&destination) {
            Ok(_) if !force => Err(Error::InvalidInput(format!(
                "{} already exists; use --force to replace it",
                destination.display()
            ))),
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    };
    check_destination()?;
    if !complete_bundle(template) {
        return Err(Error::InvalidTemplate(
            "bundle is missing its executable or plist".into(),
        ));
    }
    fs::create_dir_all(output_dir)?;
    let canonical_template = fs::canonicalize(template)?;
    if fs::canonicalize(output_dir)?.starts_with(&canonical_template)
        || fs::canonicalize(&destination).is_ok_and(|path| path == canonical_template)
    {
        return Err(Error::InvalidInput(
            "output must not replace or be inside the template".into(),
        ));
    }
    let staging = Staging::new(output_dir)?;
    let bundle = staging.0.join(format!("{name}.app"));
    run(Command::new("/usr/bin/ditto").arg(template).arg(&bundle))
        .map_err(|error| Error::Assembly(error.to_string()))?;
    let contents = bundle.join("Contents");
    fs::rename(
        contents.join("MacOS/rake-runtime"),
        contents.join("MacOS").join(&name),
    )?;
    let identifier = config.identifier();
    let version = env!("CARGO_PKG_VERSION");
    let metadata = [
        ("CFBundleName", name.as_str()),
        ("CFBundleDisplayName", name.as_str()),
        ("CFBundleIdentifier", identifier.as_str()),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", version),
        ("CFBundleExecutable", name.as_str()),
        ("CFBundleIconFile", ICON_FILE),
    ];
    for (key, value) in metadata {
        run(Command::new("/usr/bin/plutil")
            .args(["-replace", key, "-string", value])
            .arg(contents.join("Info.plist")))
        .map_err(|error| Error::Assembly(error.to_string()))?;
    }
    let resources = contents.join("Resources");
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
    let warnings = install_icon(&config.url, icon, &resources, &staging.0)?;
    sign(&bundle)?;
    let old = staging.0.join("old.app");
    let exists = check_destination()?;
    if exists {
        fs::rename(&destination, &old)?;
    }
    if let Err(error) = fs::rename(&bundle, &destination) {
        if exists {
            fs::rename(&old, &destination)?;
        }
        return Err(error.into());
    }
    Ok(AssemblyResult {
        path: destination,
        warnings,
    })
}

pub fn sign(bundle: &Path) -> Result<(), Error> {
    run(Command::new("/usr/bin/codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(bundle))?;
    Ok(())
}

fn install_icon(
    page: &str,
    icon: Option<&Icon>,
    resources: &Path,
    staging: &Path,
) -> Result<Vec<String>, Error> {
    let downloaded = staging.join("downloaded-icon");
    let converted = staging.join(ICON_FILE);
    if let Some(icon) = icon {
        let source = match icon {
            Icon::Path(path) => fs::canonicalize(path)?,
            Icon::Url(url) => {
                fetch_icon(url, &downloaded)?;
                downloaded
            }
        };
        convert_icon(&source, &converted, staging)?;
        fs::copy(converted, resources.join(ICON_FILE))?;
        return Ok(Vec::new());
    }

    let html_path = staging.join("page.html");
    let (base, html) = match fetch_icon(page, &html_path) {
        Ok(base) => (base, fs::read_to_string(html_path).unwrap_or_default()),
        Err(_) => (page.to_owned(), String::new()),
    };
    for candidate in icon_candidates(&base, &html) {
        if fetch_icon(&candidate, &downloaded).is_ok()
            && convert_icon(&downloaded, &converted, staging).is_ok()
        {
            fs::copy(&converted, resources.join(ICON_FILE))?;
            return Ok(Vec::new());
        }
    }
    Ok(vec![format!(
        "No usable icon found for {page}; keeping the template's default icon"
    )])
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

fn convert_icon(source: &Path, destination: &Path, staging: &Path) -> Result<(), Error> {
    let iconset = staging.join("app.iconset");
    fs::create_dir_all(&iconset)?;
    let png = staging.join("source.png");
    run(Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(source)
        .arg("--out")
        .arg(&png))?;
    for size in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let pixels = (size * scale).to_string();
            let suffix = if scale == 2 { "@2x" } else { "" };
            run(Command::new("sips")
                .args(["-z", &pixels, &pixels])
                .arg(&png)
                .arg("--out")
                .arg(iconset.join(format!("icon_{size}x{size}{suffix}.png"))))?;
        }
    }
    run(Command::new("iconutil")
        .args(["-c", "icns"])
        .arg(&iconset)
        .arg("-o")
        .arg(destination))?;
    Ok(())
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
    let target = TemplateTarget::current()?;
    let root = match &options.cache_root {
        Some(root) => root.clone(),
        None => cache_root()?,
    };
    let directory = root
        .join("templates")
        .join(env!("CARGO_PKG_VERSION"))
        .join(target.as_str());
    let bundle = directory.join("Rake.app");
    if !options.refresh && complete_bundle(&bundle) {
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
    let actual = run(Command::new("shasum").args(["-a", "256"]).arg(&archive))?;
    if actual.split_whitespace().next() != Some(expected) {
        return Err(Error::ChecksumMismatch);
    }
    let entries = run(Command::new("tar").arg("-tzf").arg(&archive))?;
    for entry in entries.lines() {
        let mut components = Path::new(entry).components();
        if components.next() != Some(Component::Normal("Rake.app".as_ref()))
            || components.any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(Error::InvalidTemplate(
                "archive must contain only Rake.app".into(),
            ));
        }
    }
    let extracted = staging.0.join("extracted");
    fs::create_dir(&extracted)?;
    run(Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&extracted))?;
    let fresh = extracted.join("Rake.app");
    if !complete_bundle(&fresh) {
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

fn complete_bundle(path: &Path) -> bool {
    path.join("Contents/MacOS/rake-runtime").is_file() && path.join("Contents/Info.plist").is_file()
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
        Some("/usr/bin/ditto") => "ditto",
        Some("/usr/bin/plutil") => "plutil",
        Some("/usr/bin/codesign") => "codesign",
        Some("sips") => "sips",
        Some("iconutil") => "iconutil",
        _ => "tar",
    };
    let output = command.output().map_err(|error| Error::Command {
        program,
        detail: error.to_string(),
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

    #[test]
    fn discovers_icons_in_priority_order_and_resolves_relative_urls() {
        let html = r#"
            <!-- <link rel="apple-touch-icon" href="ignored.png"> -->
            <link href='../small.ico?a=1&amp;b=2' rel='shortcut ICON'>
            <LINK HREF = "//cdn.example.com/touch.png" REL = "APPLE-TOUCH-ICON">
            <link rel=icon href=/root.png>
            <link rel="icon" href="/root.png">
            <link rel="icon" href="data:image/png,abc">
            <link rel="stylesheet" href="style.css">
            <link rel="icon">
            <link rel="apple-touch-icon" href="touch>large.png">
        "#;
        assert_eq!(
            icon_candidates("https://example.com/path/page", html),
            [
                "https://cdn.example.com/touch.png",
                "https://example.com/path/touch%3Elarge.png",
                "https://example.com/small.ico?a=1&b=2",
                "https://example.com/root.png",
                "https://example.com/favicon.ico",
            ]
        );
        assert_eq!(
            icon_candidates("https://example.com/path/page", "no icons"),
            ["https://example.com/favicon.ico"]
        );
        assert!(icon_candidates("not a URL", "").is_empty());
    }

    #[test]
    fn unusable_site_icons_preserve_default_and_return_warning() {
        let (temp, template, mut config) = assembly_fixture();
        let site = temp.0.join("site");
        fs::create_dir(&site).unwrap();
        fs::write(
            site.join("index.html"),
            r#"<link rel="apple-touch-icon" href="/bad.png">"#,
        )
        .unwrap();
        fs::write(site.join("bad.png"), "not an image").unwrap();
        let server = Server::start(site);
        config.url = format!("{}/index.html", server.url);
        let result = assemble(&config, &template, &temp.0.join("out"), false, None).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("keeping the template's default icon"));
        assert_eq!(
            fs::read(result.path.join("Contents/Resources").join(ICON_FILE)).unwrap(),
            b"icon"
        );
    }

    #[test]
    fn signing_failure_surfaces_codesign_output_and_does_not_publish() {
        let (temp, template, config) = assembly_fixture();
        let existing_output = temp.0.join("existing");
        let existing = assemble(&config, &template, &existing_output, false, None)
            .unwrap()
            .path;
        let original = fs::read(existing.join("Contents/_CodeSignature/CodeResources")).unwrap();
        let attribute = Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.ResourceFork", "unsignable"])
            .arg(template.join("Contents/MacOS/rake-runtime"))
            .status()
            .unwrap();
        assert!(attribute.success());
        let output = temp.0.join("out");
        let error = assemble(&config, &template, &output, false, None).unwrap_err();
        let Error::Command { program, detail } = &error else {
            panic!("expected codesign failure, got {error}");
        };
        assert_eq!(*program, "codesign");
        assert!(
            detail.contains("resource fork, Finder information, or similar detritus not allowed"),
            "{detail}"
        );
        assert!(error.to_string().contains(detail));
        assert_eq!(error.exit_code(), 4);
        assert_eq!(fs::read_dir(output).unwrap().count(), 0);
        let error = assemble(&config, &template, &existing_output, true, None).unwrap_err();
        assert!(error.to_string().contains("detritus not allowed"));
        assert_eq!(
            fs::read(existing.join("Contents/_CodeSignature/CodeResources")).unwrap(),
            original
        );
        assert_eq!(fs::read_dir(existing_output).unwrap().count(), 1);
    }

    #[test]
    #[ignore = "requires cargo xtask template and macOS signing tools"]
    fn real_signed_app() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let template = env::var_os("RAKE_TEMPLATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/template/Rake.app"));
        assert!(complete_bundle(&template), "Run cargo xtask template first");
        let mut config = RakeConfig::new("https://example.com");
        config.name = Some("Rake Signing Proof".into());
        let icon = Icon::Path(template.join("Contents/Resources").join(ICON_FILE));
        let result = assemble(
            &config,
            &template,
            &root.join("target/rake018-verification"),
            true,
            Some(&icon),
        )
        .unwrap();
        let verification = Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(&result.path)
            .output()
            .unwrap();
        let detail = String::from_utf8_lossy(&verification.stderr);
        assert!(verification.status.success(), "{detail}");
        println!("{detail}App retained at {}", result.path.display());
    }

    #[test]
    fn explicit_icon_failures_do_not_publish_an_app() {
        let (temp, template, config) = assembly_fixture();
        let bad = temp.0.join("bad.png");
        fs::write(&bad, "not an image").unwrap();
        let output = temp.0.join("out");
        for icon in [
            Icon::Path(temp.0.join("missing.png")),
            Icon::Path(bad),
            Icon::Url("http://127.0.0.1:1/missing.png".into()),
            Icon::Url("file:///etc/passwd".into()),
        ] {
            assert!(assemble(&config, &template, &output, false, Some(&icon)).is_err());
            assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        }
    }

    #[test]
    #[ignore = "requires cargo xtask template and macOS icon tools"]
    fn real_icon_apps() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let template = env::var_os("RAKE_TEMPLATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/template/Rake.app"));
        assert!(complete_bundle(&template), "Run cargo xtask template first");
        let output = root.join("target/rake016-verification");
        fs::create_dir_all(&output).unwrap();
        let temp = Staging::new(&output).unwrap();
        let png = temp.0.join("touch.png");
        run(Command::new("sips")
            .args(["-s", "format", "png", "-z", "256", "256"])
            .arg(template.join("Contents/Resources").join(ICON_FILE))
            .arg("--out")
            .arg(&png))
        .unwrap();
        let png_bytes = fs::read(&png).unwrap();
        let mut ico = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
        ico.extend((png_bytes.len() as u32).to_le_bytes());
        ico.extend(22_u32.to_le_bytes());
        ico.extend(&png_bytes);

        for case in ["touch", "favicon", "none", "broken", "path", "url"] {
            let site = temp.0.join(case);
            fs::create_dir(&site).unwrap();
            let html = if matches!(case, "touch" | "broken") {
                r#"<link rel="icon" href="/favicon.ico"><link rel="apple-touch-icon" href="/touch.png">"#
            } else {
                "<html><title>Icon proof</title></html>"
            };
            fs::write(site.join("index.html"), html).unwrap();
            if case == "touch" {
                fs::copy(&png, site.join("touch.png")).unwrap();
                fs::write(site.join("favicon.ico"), "must prefer touch icon").unwrap();
            }
            if case == "broken" {
                fs::write(site.join("touch.png"), "broken PNG").unwrap();
            }
            if matches!(case, "favicon" | "broken") {
                fs::write(site.join("favicon.ico"), &ico).unwrap();
            }
            fs::copy(&png, site.join("explicit.png")).unwrap();
            let server = Server::start(site);
            let mut config = RakeConfig::new(format!("{}/index.html", server.url));
            config.name = Some(format!("Icon-{case}"));
            let icon = match case {
                "path" => Some(Icon::Path(png.clone())),
                "url" => Some(Icon::Url(format!("{}/explicit.png", server.url))),
                _ => None,
            };
            let result = assemble(&config, &template, &output, true, icon.as_ref()).unwrap();
            let icns = result.path.join("Contents/Resources").join(ICON_FILE);
            if case == "none" {
                assert_eq!(result.warnings.len(), 1);
                assert_eq!(
                    fs::read(icns).unwrap(),
                    fs::read(template.join("Contents/Resources").join(ICON_FILE)).unwrap()
                );
                println!(
                    "{case}: built app, default icon unchanged, warning = {}",
                    result.warnings[0]
                );
                continue;
            }
            assert!(result.warnings.is_empty(), "{:?}", result.warnings);
            let decoded = temp.0.join(format!("{case}.iconset"));
            run(Command::new("iconutil")
                .args(["-c", "iconset"])
                .arg(&icns)
                .arg("-o")
                .arg(&decoded))
            .unwrap();
            for size in [16, 32, 128, 256, 512] {
                for scale in [1, 2] {
                    let suffix = if scale == 2 { "@2x" } else { "" };
                    let image = decoded.join(format!("icon_{size}x{size}{suffix}.png"));
                    let properties = run(Command::new("sips")
                        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
                        .arg(image))
                    .unwrap();
                    let pixels = size * scale;
                    assert!(properties.contains(&format!("pixelWidth: {pixels}")));
                    assert!(properties.contains(&format!("pixelHeight: {pixels}")));
                }
            }
            println!("{case}: built app, decoded ICNS, verified all 10 standard representations");
        }
        println!("Apps retained at {}", output.display());
    }

    fn assembly_fixture() -> (Staging, PathBuf, RakeConfig) {
        let temp = Staging::new(&env::temp_dir()).unwrap();
        let template = temp.0.join("Rake.app");
        fs::create_dir_all(template.join("Contents/MacOS")).unwrap();
        fs::create_dir_all(template.join("Contents/Resources")).unwrap();
        fs::write(template.join("Contents/MacOS/rake-runtime"), "runtime").unwrap();
        fs::write(template.join("Contents/Resources/icon.icns"), "icon").unwrap();
        fs::write(template.join("Contents/Info.plist"), r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>rake-runtime</string></dict></plist>"#).unwrap();
        let mut config = RakeConfig::new("http://127.0.0.1:1/page");
        config.name = Some("My App".into());
        (temp, template, config)
    }

    fn assert_bundle_metadata(bundle: &Path, name: &str, identifier: &str) {
        let plist = bundle.join("Contents/Info.plist");
        let lint = run(Command::new("/usr/bin/plutil").arg("-lint").arg(&plist)).unwrap();
        assert!(lint.contains(": OK"), "{lint}");
        for (key, expected) in [
            ("CFBundleName", name),
            ("CFBundleDisplayName", name),
            ("CFBundleIdentifier", identifier),
            ("CFBundleShortVersionString", env!("CARGO_PKG_VERSION")),
            ("CFBundleVersion", env!("CARGO_PKG_VERSION")),
            ("CFBundleExecutable", name),
            ("CFBundleIconFile", "icon.icns"),
        ] {
            let actual = run(Command::new("/usr/bin/plutil")
                .args(["-extract", key, "raw", "-o", "-"])
                .arg(&plist))
            .unwrap();
            assert_eq!(actual.trim_end_matches('\n'), expected, "{key}");
        }
        assert!(bundle.join("Contents/MacOS").join(name).is_file());
        assert!(bundle.join("Contents/Resources/icon.icns").is_file());
    }

    #[test]
    fn patches_all_bundle_metadata_from_config_and_defaults() {
        for (name, identifier, expected_name) in [
            (
                Some("My & <App> \"Test\""),
                Some("org.example.Custom-App"),
                "My & <App> \"Test\"",
            ),
            (None, None, "127.0.0.1"),
        ] {
            let (temp, template, mut config) = assembly_fixture();
            config.name = name.map(String::from);
            config.identifier = identifier.map(String::from);
            let expected_identifier = config.identifier();
            let output = temp.0.join("out");
            let original = fs::read(template.join("Contents/Info.plist")).unwrap();
            let result = assemble(&config, &template, &output, false, None).unwrap();
            assert_bundle_metadata(&result.path, expected_name, &expected_identifier);
            assert_eq!(
                fs::read(template.join("Contents/Info.plist")).unwrap(),
                original
            );
        }
    }

    #[test]
    fn plist_failure_preserves_existing_app_and_removes_staging() {
        let (temp, template, config) = assembly_fixture();
        let output = temp.0.join("out");
        let result = assemble(&config, &template, &output, false, None).unwrap();
        let original = fs::read(result.path.join("Contents/Info.plist")).unwrap();
        fs::write(template.join("Contents/Info.plist"), "invalid plist").unwrap();
        let error = assemble(&config, &template, &output, true, None).unwrap_err();
        assert!(matches!(error, Error::Assembly(_)));
        assert!(error.to_string().contains("plutil"));
        assert_eq!(
            fs::read(result.path.join("Contents/Info.plist")).unwrap(),
            original
        );
        assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    }

    #[test]
    #[ignore = "requires cargo xtask template and macOS plist tools"]
    fn real_plist_app() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let template = env::var_os("RAKE_TEMPLATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("target/template/Rake.app"));
        assert!(complete_bundle(&template), "Run cargo xtask template first");
        let mut config = RakeConfig::new("https://example.com");
        config.name = Some("Rake Plist Proof".into());
        config.identifier = Some("org.example.rake-plist-proof".into());
        let icon = Icon::Path(template.join("Contents/Resources").join(ICON_FILE));
        let result = assemble(
            &config,
            &template,
            &root.join("target/rake017-verification"),
            true,
            Some(&icon),
        )
        .unwrap();
        assert_bundle_metadata(
            &result.path,
            "Rake Plist Proof",
            "org.example.rake-plist-proof",
        );
        println!(
            "Verified all seven plist keys and plutil -lint on {}",
            result.path.display()
        );
    }

    #[test]
    fn assembles_layout_executable_config_and_inject_files() {
        use std::os::unix::fs::PermissionsExt;
        let (temp, template, mut config) = assembly_fixture();
        let executable = template.join("Contents/MacOS/rake-runtime");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        for (name, body) in [
            ("style.css", "body {color:red}"),
            ("probe.js", "window.proof = 1"),
        ] {
            let file = temp.0.join(name);
            fs::write(&file, body).unwrap();
            config.inject.push(file);
        }
        let result = assemble(&config, &template, &temp.0.join("out"), false, None).unwrap();
        assert_eq!(result.warnings.len(), 1);
        let bundle = result.path;
        assert!(bundle.ends_with("out/My App.app"));
        let executable = bundle.join("Contents/MacOS/My App");
        assert_eq!(fs::read_to_string(&executable).unwrap(), "runtime");
        assert_eq!(
            fs::metadata(executable).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(!bundle.join("Contents/MacOS/rake-runtime").exists());
        assert_eq!(
            run(Command::new("/usr/bin/plutil")
                .args(["-extract", "CFBundleExecutable", "raw", "-o", "-"])
                .arg(bundle.join("Contents/Info.plist")))
            .unwrap()
            .trim(),
            "My App"
        );
        let resources = bundle.join("Contents/Resources");
        let written: RakeConfig =
            serde_json::from_slice(&fs::read(resources.join("config.json")).unwrap()).unwrap();
        let mut expected = config.clone();
        expected.inject = vec!["style.css".into(), "probe.js".into()];
        assert_eq!(written, expected);
        for source in config.inject {
            assert_eq!(
                fs::read(resources.join("inject").join(source.file_name().unwrap())).unwrap(),
                fs::read(source).unwrap()
            );
        }
        assert_eq!(fs::read(resources.join("icon.icns")).unwrap(), b"icon");
        assert!(template.join("Contents/MacOS/rake-runtime").exists());
        assert!(!template.join("Contents/Resources/config.json").exists());
    }

    #[test]
    fn refuses_overwrite_and_force_replaces_only_after_success() {
        let (temp, template, mut config) = assembly_fixture();
        let output = temp.0.join("out");
        let bundle = assemble(&config, &template, &output, false, None)
            .unwrap()
            .path;
        let marker = bundle.join("marker");
        fs::write(&marker, "keep me").unwrap();
        let error = assemble(&config, &template, &output, false, None).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("--force"));
        assert_eq!(fs::read_to_string(&marker).unwrap(), "keep me");
        config.inject = vec![temp.0.join("missing.js")];
        assert_eq!(
            assemble(&config, &template, &output, true, None)
                .unwrap_err()
                .exit_code(),
            3
        );
        assert_eq!(fs::read_to_string(&marker).unwrap(), "keep me");
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
        config.inject.clear();
        assert_eq!(
            assemble(&config, &template, &output, true, None)
                .unwrap()
                .path,
            bundle
        );
        assert!(!marker.exists());
        assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    }

    #[test]
    fn rejects_duplicate_inject_names_and_unsafe_app_names() {
        let (temp, template, mut config) = assembly_fixture();
        let output = temp.0.join("out");
        for directory in ["one", "two"] {
            fs::create_dir(temp.0.join(directory)).unwrap();
            let file = temp.0.join(directory).join("same.js");
            fs::write(&file, directory).unwrap();
            config.inject.push(file);
        }
        let error = assemble(&config, &template, &output, false, None).unwrap_err();
        assert!(error.to_string().contains("duplicate file name: same.js"));
        assert_eq!(error.exit_code(), 2);
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
        config.inject.clear();
        for name in [
            "",
            " ",
            ".",
            "..",
            "../escape",
            "/absolute",
            "bad:name",
            "bad\\name",
            "bad\0name",
        ] {
            config.name = Some(name.into());
            assert_eq!(
                assemble(&config, &template, &output, false, None)
                    .unwrap_err()
                    .exit_code(),
                2,
                "{name:?}"
            );
        }
    }

    #[test]
    fn refuses_to_replace_or_write_inside_template() {
        let (temp, template, mut config) = assembly_fixture();
        config.name = Some("Rake".into());
        for output in [&temp.0, &template] {
            assert_eq!(
                assemble(&config, &template, output, true, None)
                    .unwrap_err()
                    .exit_code(),
                2
            );
            assert!(complete_bundle(&template));
            assert!(!template.join("Contents/Resources/config.json").exists());
        }
    }

    struct Server {
        url: String,
        requests: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl Server {
        fn start(root: PathBuf) -> Self {
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
        let contents = release.join("Rake.app/Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::write(contents.join("MacOS/rake-runtime"), "fake runtime").unwrap();
        fs::write(contents.join("Info.plist"), "fake plist").unwrap();
        let asset = format!(
            "rake-template-{}-macos-arm64.tar.gz",
            env!("CARGO_PKG_VERSION")
        );
        run(Command::new("tar")
            .current_dir(&release)
            .env("COPYFILE_DISABLE", "1")
            .args(["-czf", &asset, "Rake.app"]))
        .unwrap();
        let sums = run(Command::new("shasum")
            .current_dir(&release)
            .args(["-a", "256", &asset]))
        .unwrap();
        fs::write(release.join("SHA256SUMS"), sums).unwrap();
        (temp, release, asset)
    }

    #[test]
    fn downloads_extracts_reuses_offline_and_refreshes() {
        let (temp, _, _) = fixture();
        let server = Server::start(temp.0.clone());
        let mut options = TemplateOptions {
            cache_root: Some(temp.0.join("cache")),
            base_url: server.url.clone(),
            ..Default::default()
        };
        let bundle = ensure_template(&options).unwrap();
        assert!(bundle.ends_with(format!(
            "templates/{}/macos-arm64/Rake.app",
            env!("CARGO_PKG_VERSION")
        )));
        assert_eq!(
            fs::read_to_string(bundle.join("Contents/MacOS/rake-runtime")).unwrap(),
            "fake runtime"
        );
        assert_eq!(server.requests.load(Ordering::Relaxed), 2);
        assert_eq!(ensure_template(&options).unwrap(), bundle);
        assert_eq!(server.requests.load(Ordering::Relaxed), 2);
        fs::write(bundle.join("Contents/Info.plist"), "changed").unwrap();
        options.refresh = true;
        assert_eq!(ensure_template(&options).unwrap(), bundle);
        assert_eq!(server.requests.load(Ordering::Relaxed), 4);
        assert_eq!(
            fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap(),
            "fake plist"
        );
        drop(server);
        options.refresh = false;
        assert_eq!(ensure_template(&options).unwrap(), bundle);
        assert_eq!(fs::read_dir(bundle.parent().unwrap()).unwrap().count(), 1);
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
            .join("macos-arm64");
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
            .join("macos-arm64");
        fs::create_dir_all(target.join("Rake.app/Contents")).unwrap();
        fs::write(target.join("Rake.app/Contents/Info.plist"), "partial").unwrap();
        fs::create_dir(target.join(".fetch-interrupted")).unwrap();
        fs::write(target.join(".fetch-interrupted/partial.tar.gz"), "partial").unwrap();
        let options = TemplateOptions {
            cache_root: Some(temp.0.join("cache")),
            base_url: server.url.clone(),
            ..Default::default()
        };
        let bundle = ensure_template(&options).unwrap();
        assert!(complete_bundle(&bundle));
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
        for (os, arch) in [
            ("macos", "x86_64"),
            ("linux", "aarch64"),
            ("windows", "x86_64"),
        ] {
            assert_eq!(
                TemplateTarget::resolve(os, arch).unwrap_err().exit_code(),
                4
            );
        }
    }
}
