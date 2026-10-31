use super::*;

pub const ICON_FILE: &str = "icon.icns";

pub(crate) fn package(
    _config: &RakeConfig,
    app: &Path,
    output_dir: &Path,
    target: Target,
    force: bool,
    _architecture: TemplateTarget,
) -> Result<PathBuf, Error> {
    match target {
        Target::App => Ok(app.to_path_buf()),
        Target::Dmg => package_dmg(app, output_dir, force),
        _ => Err(Error::InvalidInput(
            "expected a macOS package target".into(),
        )),
    }
}

pub(crate) fn install_default(app: &Path) -> Result<(), Error> {
    install(app, Path::new("/Applications"))?;
    Ok(())
}

/// Copy a template into <output_dir>/<Name>.app without modifying the template.
/// Inject paths are resolved from the caller's working directory.
pub fn assemble(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
) -> Result<AssemblyResult, Error> {
    assemble_with_progress(config, template, output_dir, force, &mut |_| {})
}

pub(crate) fn assemble_with_progress(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
    progress: &mut impl FnMut(BuildStep),
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
    run(Command::new("/usr/bin/ditto").arg(template).arg(&bundle))?;
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
            .arg(contents.join("Info.plist")))?;
    }
    let resources = contents.join("Resources");
    write_config(config, &resources)?;
    let icon = config.icon.as_ref().map(|source| {
        if Url::parse(source).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
            Icon::Url(source.clone())
        } else {
            Icon::Path(PathBuf::from(source))
        }
    });
    let warnings = install_icon(&config.url, icon.as_ref(), &resources)?;
    progress(BuildStep::Signing);
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

pub fn package_dmg(app: &Path, output_dir: &Path, force: bool) -> Result<PathBuf, Error> {
    let name = bundle_name(app)?;
    let destination = output_dir.join(format!("{name}.dmg"));
    let check_destination = || -> Result<(), Error> {
        match fs::symlink_metadata(&destination) {
            Ok(_) if !force => Err(Error::InvalidInput(format!(
                "{} already exists; use --force to replace it",
                destination.display()
            ))),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    };
    check_destination()?;
    let app = fs::canonicalize(app)?;
    prepare_output_directory(output_dir, &app)?;
    let staging = Staging::new(output_dir)?;
    let payload = staging.0.join("payload");
    fs::create_dir(&payload)?;
    let copy = payload.join(format!("{name}.app"));
    run(Command::new("/usr/bin/ditto").arg(&app).arg(&copy))?;
    verify_signature(&copy)?;
    std::os::unix::fs::symlink("/Applications", payload.join("Applications"))?;
    let image = staging.0.join(format!("{name}.dmg"));
    run(Command::new("/usr/bin/hdiutil")
        .args(["create", "-volname", name, "-srcfolder"])
        .arg(&payload)
        .args(["-ov", "-format", "UDZO"])
        .arg(&image))?;
    check_destination()?;
    fs::rename(image, &destination)?;
    Ok(destination)
}

pub fn install(app: &Path, applications: &Path) -> Result<PathBuf, Error> {
    let name = bundle_name(app)?;
    let identifier = bundle_identifier(app)?;
    if !app.join("Contents/Resources/config.json").is_file() {
        return Err(Error::InvalidInput("source is not a Rake-built app".into()));
    }
    let destination = applications.join(format!("{name}.app"));
    let check_destination = || -> Result<bool, Error> {
        match fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir()
                    || !destination.join("Contents/Resources/config.json").is_file()
                    || bundle_identifier(&destination).ok().as_ref() != Some(&identifier)
                {
                    return Err(Error::InvalidInput(format!(
                        "refusing to replace {}; it is not a Rake-built app with identifier {identifier}",
                        destination.display()
                    )));
                }
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    };
    check_destination()?;
    let app = fs::canonicalize(app)?;
    if fs::canonicalize(&destination).is_ok_and(|path| path == app) {
        return Err(Error::InvalidInput(
            "installation must not replace the source app".into(),
        ));
    }
    prepare_output_directory(applications, &app)?;
    let staging = Staging::new(applications)?;
    let copy = staging.0.join(format!("{name}.app"));
    run(Command::new("/usr/bin/ditto").arg(&app).arg(&copy))?;
    verify_signature(&copy)?;
    let old = staging.0.join("old.app");
    let exists = check_destination()?;
    if exists {
        fs::rename(&destination, &old)?;
    }
    if let Err(error) = fs::rename(&copy, &destination) {
        if exists {
            fs::rename(&old, &destination)?;
        }
        return Err(error.into());
    }
    Ok(destination)
}

fn prepare_output_directory(directory: &Path, app: &Path) -> Result<(), Error> {
    let absolute = env::current_dir()?.join(directory);
    for ancestor in absolute.ancestors() {
        match fs::canonicalize(ancestor) {
            Ok(parent) => {
                if parent.starts_with(app) {
                    return Err(Error::InvalidInput(
                        "output must not be inside the source app".into(),
                    ));
                }
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    fs::create_dir_all(directory)?;
    Ok(())
}

fn bundle_name(app: &Path) -> Result<&str, Error> {
    app.file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| {
            app.extension().is_some_and(|extension| extension == "app")
                && !name.trim().is_empty()
                && !name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
        })
        .ok_or_else(|| Error::InvalidInput("expected a named .app bundle".into()))
}

fn bundle_identifier(app: &Path) -> Result<String, Error> {
    let identifier = run(Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist")))?;
    let identifier = identifier.trim_end_matches('\n');
    if identifier.is_empty() {
        return Err(Error::InvalidInput(
            "app has an empty CFBundleIdentifier".into(),
        ));
    }
    Ok(identifier.into())
}

fn verify_signature(app: &Path) -> Result<(), Error> {
    run(Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app))?;
    Ok(())
}

fn install_icon(page: &str, icon: Option<&Icon>, resources: &Path) -> Result<Vec<String>, Error> {
    match prepare_icon(page, icon, &resources.join(ICON_FILE)) {
        Ok(_) => Ok(Vec::new()),
        Err(Error::InvalidInput(message)) if icon.is_none() => Ok(vec![message]),
        Err(error) => Err(error),
    }
}

/// Write a converted ICNS icon and return its PNG preview. Discovery failure leaves
/// the destination unchanged. The destination's parent directory must exist.
pub fn prepare_icon(page: &str, icon: Option<&Icon>, destination: &Path) -> Result<Vec<u8>, Error> {
    let work = Staging::new(destination.parent().unwrap_or(Path::new(".")))?;
    let staging = &work.0;
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
        let preview = fs::read(staging.join("source.png"))?;
        fs::copy(converted, destination)?;
        return Ok(preview);
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
            let preview = fs::read(staging.join("source.png"))?;
            fs::copy(&converted, destination)?;
            return Ok(preview);
        }
    }
    Err(Error::InvalidInput(format!(
        "No usable icon found for {page}; keeping the template's default icon"
    )))
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

fn complete_bundle(path: &Path) -> bool {
    Platform::Macos.complete_bundle(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Server;

    fn install_fixture() -> (Staging, PathBuf, PathBuf) {
        let (temp, template, config) = assembly_fixture();
        let app = assemble(&config, &template, &temp.0.join("out"), false)
            .unwrap()
            .path;
        let applications = temp.0.join("Applications");
        (temp, app, applications)
    }

    #[test]
    fn build_reports_real_steps_and_stops_on_error() {
        let (temp, template, config) = assembly_fixture();
        let cache = temp.0.join("cache");
        let cached = cache
            .join("templates")
            .join(env!("CARGO_PKG_VERSION"))
            .join(TemplateTarget::current().unwrap().as_str())
            .join("Rake.app");
        fs::create_dir_all(cached.parent().unwrap()).unwrap();
        fs::rename(template, cached).unwrap();
        let mut options = BuildOptions {
            template: TemplateOptions {
                cache_root: Some(cache),
                base_url: "http://127.0.0.1:1".into(),
                ..Default::default()
            },
            output_dir: temp.0.join("out"),
            ..Default::default()
        };
        let mut steps = Vec::new();
        let mut warnings = Vec::new();
        let result = build(&config, &options, &mut warnings, |step| steps.push(step)).unwrap();
        assert_eq!(
            steps,
            [
                BuildStep::Template,
                BuildStep::Assembling,
                BuildStep::Signing,
                BuildStep::Done
            ]
        );
        assert_eq!(result.app_path, result.output_path);
        verify_signature(&result.app_path).unwrap();
        assert_eq!(warnings.len(), 1);

        steps.clear();
        assert!(build(&config, &options, &mut warnings, |step| steps.push(step)).is_err());
        assert_eq!(steps, [BuildStep::Template, BuildStep::Assembling]);
        assert!(warnings.is_empty());

        options.target = Target::Dmg;
        options.output_dir = temp.0.join("dmg");
        fs::create_dir(&options.output_dir).unwrap();
        fs::write(options.output_dir.join("My App.dmg"), "existing").unwrap();
        steps.clear();
        assert!(build(&config, &options, &mut warnings, |step| steps.push(step)).is_err());
        assert_eq!(
            steps,
            [
                BuildStep::Template,
                BuildStep::Assembling,
                BuildStep::Signing,
                BuildStep::Packaging
            ]
        );
        assert_eq!(warnings.len(), 1);

        steps.clear();
        assert!(
            build(
                &RakeConfig::new("invalid"),
                &options,
                &mut warnings,
                |step| steps.push(step)
            )
            .is_err()
        );
        assert!(steps.is_empty());
    }

    #[test]
    fn installs_fresh_app_and_preserves_signature() {
        let (_temp, app, applications) = install_fixture();
        let destination = install(&app, &applications).unwrap();
        assert_eq!(destination, applications.join("My App.app"));
        assert_eq!(Target::default(), Target::App);
        assert_eq!(
            fs::read(destination.join("Contents/_CodeSignature/CodeResources")).unwrap(),
            fs::read(app.join("Contents/_CodeSignature/CodeResources")).unwrap()
        );
        verify_signature(&destination).unwrap();
        assert_eq!(fs::read_dir(applications).unwrap().count(), 1);
    }

    #[test]
    fn replaces_same_identifier_rake_app() {
        let (_temp, app, applications) = install_fixture();
        let destination = install(&app, &applications).unwrap();
        fs::write(app.join("Contents/Resources/update"), "new version").unwrap();
        sign(&app).unwrap();
        assert_eq!(install(&app, &applications).unwrap(), destination);
        assert_eq!(
            fs::read(destination.join("Contents/Resources/update")).unwrap(),
            b"new version"
        );
        verify_signature(&destination).unwrap();
        assert_eq!(fs::read_dir(applications).unwrap().count(), 1);
    }

    #[test]
    fn refuses_different_identifier_without_changing_existing_app() {
        let (_temp, app, applications) = install_fixture();
        let destination = install(&app, &applications).unwrap();
        let plist = destination.join("Contents/Info.plist");
        run(Command::new("/usr/bin/plutil")
            .args([
                "-replace",
                "CFBundleIdentifier",
                "-string",
                "org.example.other",
            ])
            .arg(&plist))
        .unwrap();
        let original = fs::read(&plist).unwrap();
        let error = install(&app, &applications).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("refusing to replace"));
        assert_eq!(fs::read(plist).unwrap(), original);
        assert_eq!(fs::read_dir(applications).unwrap().count(), 1);
    }

    #[test]
    fn refuses_non_rake_app_even_with_same_identifier() {
        let (_temp, app, applications) = install_fixture();
        let destination = install(&app, &applications).unwrap();
        fs::remove_file(destination.join("Contents/Resources/config.json")).unwrap();
        let original = fs::read(destination.join("Contents/Info.plist")).unwrap();
        let error = install(&app, &applications).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("not a Rake-built app"));
        assert_eq!(
            fs::read(destination.join("Contents/Info.plist")).unwrap(),
            original
        );
        assert!(!destination.join("Contents/Resources/config.json").exists());
        assert_eq!(fs::read_dir(applications).unwrap().count(), 1);
    }

    #[test]
    fn refuses_install_destination_symlink() {
        let (_temp, app, applications) = install_fixture();
        fs::create_dir(&applications).unwrap();
        let destination = applications.join("My App.app");
        std::os::unix::fs::symlink(&app, &destination).unwrap();
        assert_eq!(install(&app, &applications).unwrap_err().exit_code(), 2);
        assert_eq!(fs::read_link(destination).unwrap(), app);
    }

    #[test]
    fn failed_signature_verification_preserves_installed_app() {
        let (_temp, app, applications) = install_fixture();
        let destination = install(&app, &applications).unwrap();
        let original = fs::read(destination.join("Contents/Resources/config.json")).unwrap();
        fs::write(app.join("Contents/Resources/config.json"), "tampered").unwrap();
        let error = install(&app, &applications).unwrap_err();
        assert!(matches!(
            error,
            Error::Command {
                program: "codesign",
                ..
            }
        ));
        assert_eq!(
            fs::read(destination.join("Contents/Resources/config.json")).unwrap(),
            original
        );
        verify_signature(&destination).unwrap();
        assert_eq!(fs::read_dir(applications).unwrap().count(), 1);
    }

    #[test]
    fn rejects_outputs_inside_source_without_modifying_it() {
        let (_temp, app, _) = install_fixture();
        let nested = app.join("new/output");
        assert_eq!(install(&app, &nested).unwrap_err().exit_code(), 2);
        assert_eq!(
            package_dmg(&app, &nested, false).unwrap_err().exit_code(),
            2
        );
        assert!(!app.join("new").exists());
        assert_eq!(
            install(&app, app.parent().unwrap())
                .unwrap_err()
                .exit_code(),
            2
        );
        verify_signature(&app).unwrap();
    }

    #[test]
    fn refuses_dmg_overwrite_and_preserves_existing_image_on_failure() {
        let (_temp, app, output) = install_fixture();
        fs::create_dir(&output).unwrap();
        let image = output.join("My App.dmg");
        fs::write(&image, "existing image").unwrap();
        assert_eq!(
            package_dmg(&app, &output, false).unwrap_err().exit_code(),
            2
        );
        fs::write(app.join("Contents/Resources/config.json"), "tampered").unwrap();
        assert!(package_dmg(&app, &output, true).is_err());
        assert_eq!(fs::read(image).unwrap(), b"existing image");
        assert_eq!(fs::read_dir(output).unwrap().count(), 1);
    }

    #[test]
    #[ignore = "requires cargo xtask template and macOS disk image tools"]
    fn real_dmg_app() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let template = root.join("target/template/Rake.app");
        assert!(complete_bundle(&template), "Run cargo xtask template first");
        let output = root.join("target/rake019-verification");
        let mut config = RakeConfig::new("https://example.com");
        config.name = Some("Rake Install Proof".into());
        config.identifier = Some("org.example.rake-install-proof".into());
        config.icon = Some(
            template
                .join("Contents/Resources")
                .join(ICON_FILE)
                .to_string_lossy()
                .into_owned(),
        );
        let app = assemble(&config, &template, &output, true).unwrap().path;
        let image = package_dmg(&app, &output, true).unwrap();
        let mount = Staging::new(&output).unwrap();
        run(Command::new("/usr/bin/hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
            .arg(&mount.0)
            .arg(&image))
        .unwrap();
        let mounted = mount.0.join("Rake Install Proof.app");
        let verification = verify_signature(&mounted);
        let has_config = mounted.join("Contents/Resources/config.json").is_file();
        let applications_link = fs::read_link(mount.0.join("Applications"));
        run(Command::new("/usr/bin/hdiutil").arg("detach").arg(&mount.0)).unwrap();
        verification.unwrap();
        assert!(has_config);
        assert_eq!(applications_link.unwrap(), Path::new("/Applications"));
        println!(
            "Mounted DMG read-only, verified app signature and Applications link, detached. Artifacts retained at {}",
            output.display()
        );
    }

    #[test]
    #[ignore = "writes only /Applications/Rake Install Proof.app; run real_dmg_app first, then launch and remove manually"]
    fn real_install_app() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let app = root.join("target/rake019-verification/Rake Install Proof.app");
        let applications = env::var_os("RAKE_APPLICATIONS")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/Applications"));
        let destination = applications.join("Rake Install Proof.app");
        assert!(
            fs::symlink_metadata(&destination)
                .is_err_and(|error| error.kind() == io::ErrorKind::NotFound),
            "proof destination must not exist"
        );
        assert_eq!(install(&app, &applications).unwrap(), destination);
        verify_signature(&destination).unwrap();
        println!(
            "Installed and verified {}. Launch, quit, and remove this proof app manually.",
            destination.display()
        );
    }

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
        let result = assemble(&config, &template, &temp.0.join("out"), false).unwrap();
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
        let existing = assemble(&config, &template, &existing_output, false)
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
        let error = assemble(&config, &template, &output, false).unwrap_err();
        let Error::Command { program, detail } = &error else {
            panic!("expected codesign failure, got {error}");
        };
        assert_eq!(*program, "codesign");
        assert!(
            detail.contains("resource fork, Finder information, or similar detritus not allowed"),
            "{detail}"
        );
        assert!(error.to_string().contains(detail));
        assert_eq!(error.exit_code(), 3);
        assert_eq!(fs::read_dir(output).unwrap().count(), 0);
        let error = assemble(&config, &template, &existing_output, true).unwrap_err();
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
        config.icon = Some(
            template
                .join("Contents/Resources")
                .join(ICON_FILE)
                .to_string_lossy()
                .into_owned(),
        );
        let result = assemble(
            &config,
            &template,
            &root.join("target/rake018-verification"),
            true,
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
        let (temp, template, mut config) = assembly_fixture();
        let bad = temp.0.join("bad.png");
        fs::write(&bad, "not an image").unwrap();
        let output = temp.0.join("out");
        for icon in [
            temp.0.join("missing.png").to_string_lossy().into_owned(),
            bad.to_string_lossy().into_owned(),
            "http://127.0.0.1:1/missing.png".into(),
            "file:///etc/passwd".into(),
        ] {
            config.icon = Some(icon);
            assert!(assemble(&config, &template, &output, false).is_err());
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
            config.icon = match case {
                "path" => Some(png.to_string_lossy().into_owned()),
                "url" => Some(format!("{}/explicit.png", server.url)),
                _ => None,
            };
            let result = assemble(&config, &template, &output, true).unwrap();
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
            let result = assemble(&config, &template, &output, false).unwrap();
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
        let result = assemble(&config, &template, &output, false).unwrap();
        let original = fs::read(result.path.join("Contents/Info.plist")).unwrap();
        fs::write(template.join("Contents/Info.plist"), "invalid plist").unwrap();
        let error = assemble(&config, &template, &output, true).unwrap_err();
        assert!(matches!(
            error,
            Error::Command {
                program: "plutil",
                ..
            }
        ));
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
        config.icon = Some(
            template
                .join("Contents/Resources")
                .join(ICON_FILE)
                .to_string_lossy()
                .into_owned(),
        );
        let result = assemble(
            &config,
            &template,
            &root.join("target/rake017-verification"),
            true,
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
        let result = assemble(&config, &template, &temp.0.join("out"), false).unwrap();
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
        let bundle = assemble(&config, &template, &output, false).unwrap().path;
        let marker = bundle.join("marker");
        fs::write(&marker, "keep me").unwrap();
        let error = assemble(&config, &template, &output, false).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("--force"));
        assert_eq!(fs::read_to_string(&marker).unwrap(), "keep me");
        config.inject = vec![temp.0.join("missing.js")];
        assert_eq!(
            assemble(&config, &template, &output, true)
                .unwrap_err()
                .exit_code(),
            3
        );
        assert_eq!(fs::read_to_string(&marker).unwrap(), "keep me");
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
        config.inject.clear();
        assert_eq!(
            assemble(&config, &template, &output, true).unwrap().path,
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
        let error = assemble(&config, &template, &output, false).unwrap_err();
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
                assemble(&config, &template, &output, false)
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
                assemble(&config, &template, output, true)
                    .unwrap_err()
                    .exit_code(),
                2
            );
            assert!(complete_bundle(&template));
            assert!(!template.join("Contents/Resources/config.json").exists());
        }
    }
}
