use super::*;

#[cfg(target_os = "linux")]
pub(crate) fn assemble_with_progress(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
    _progress: &mut impl FnMut(BuildStep),
) -> Result<AssemblyResult, Error> {
    assemble(config, template, output_dir, force)
}

#[cfg(target_os = "linux")]
pub(crate) fn install_default(_app: &Path) -> Result<(), Error> {
    Err(Error::InvalidInput(
        "install the .deb with apt instead".into(),
    ))
}

pub fn assemble(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
) -> Result<AssemblyResult, Error> {
    config
        .validate()
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    let name = config.app_name();
    if name.trim().is_empty()
        || matches!(name.as_str(), "." | "..")
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::InvalidInput(
            "name: must be a nonempty file name".into(),
        ));
    }
    if !Platform::Linux.complete_bundle(template) {
        return Err(Error::InvalidTemplate("missing runtime or PNG icon".into()));
    }
    fs::create_dir_all(output_dir)?;
    let output_dir = fs::canonicalize(output_dir)?;
    let template = fs::canonicalize(template)?;
    let destination = output_dir.join(&name);
    if output_dir.starts_with(&template)
        || fs::canonicalize(&destination).is_ok_and(|path| template.starts_with(path))
    {
        return Err(Error::InvalidInput(
            "output must not replace or be inside the template".into(),
        ));
    }
    check_destination(&destination, force)?;
    let staging = Staging::new(&output_dir)?;
    let app = staging.0.join("app");
    run(Command::new("cp").arg("-R").arg(&template).arg(&app))?;
    write_config(config, &app)?;
    let warnings = install_icon(config, &app)?;
    fs::write(
        app.join(format!("{}.desktop", config.identifier())),
        desktop(config, &destination),
    )?;
    let old = staging.0.join("old");
    let exists = check_destination(&destination, force)?;
    if exists {
        fs::rename(&destination, &old)?;
    }
    if let Err(error) = fs::rename(&app, &destination) {
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

fn check_destination(path: &Path, force: bool) -> Result<bool, Error> {
    match fs::symlink_metadata(path) {
        Ok(_) if !force => Err(Error::InvalidInput(format!(
            "{} already exists; use --force to replace it",
            path.display()
        ))),
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn desktop(config: &RakeConfig, directory: &Path) -> String {
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    };
    let executable = directory
        .join("rake-runtime")
        .to_string_lossy()
        .into_owned();
    let executable = executable
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nExec=\"{}\"\nIcon={}\nTerminal=false\nCategories=Network;\n",
        escape(&config.app_name()),
        executable,
        escape(&directory.join("icon.png").to_string_lossy())
    )
}

fn install_icon(config: &RakeConfig, app: &Path) -> Result<Vec<String>, Error> {
    let work = Staging::new(app)?;
    let downloaded = work.0.join("icon");
    let candidates = if let Some(source) = &config.icon {
        vec![source.clone()]
    } else {
        let html = work.0.join("page.html");
        let base = fetch_icon(&config.url, &html).unwrap_or_else(|_| config.url.clone());
        icon_candidates(&base, &fs::read_to_string(html).unwrap_or_default())
    };
    for candidate in candidates {
        let source =
            if Url::parse(&candidate).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
                if fetch_icon(&candidate, &downloaded).is_err() {
                    continue;
                }
                downloaded.clone()
            } else {
                PathBuf::from(candidate)
            };
        let bytes = fs::read(&source).unwrap_or_default();
        let bytes = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            bytes
        } else {
            let converted = work.0.join("converted.png");
            if run(Command::new("gdk-pixbuf-thumbnailer")
                .args(["--size", "256", "--"])
                .arg(&source)
                .arg(&converted))
            .is_err()
            {
                continue;
            }
            fs::read(converted)?
        };
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            fs::write(app.join("icon.png"), bytes)?;
            return Ok(Vec::new());
        }
    }
    Ok(vec!["No usable icon found; keeping the template's default icon. Non-PNG icons require gdk-pixbuf-thumbnailer".into()])
}

pub(crate) fn package(
    config: &RakeConfig,
    app: &Path,
    output_dir: &Path,
    target: Target,
    force: bool,
    architecture: TemplateTarget,
) -> Result<PathBuf, Error> {
    if target == Target::App {
        return Ok(app.to_path_buf());
    }
    let name = config.app_name();
    let extension = if target == Target::Deb {
        "deb"
    } else {
        "tar.gz"
    };
    let destination = output_dir.join(format!("{name}.{extension}"));
    check_destination(&destination, force)?;
    let staging = Staging::new(output_dir)?;
    let archive = staging.0.join(format!("package.{extension}"));
    if target == Target::Tarball {
        run(Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(app.parent().unwrap())
            .arg("--")
            .arg(&name))?;
    } else {
        let identifier = config.identifier().to_ascii_lowercase();
        let root = staging.0.join("root");
        let install_path = PathBuf::from("/opt").join(&identifier);
        fs::create_dir_all(root.join("opt"))?;
        run(Command::new("cp")
            .arg("-R")
            .arg(app)
            .arg(root.join("opt").join(&identifier)))?;
        let applications = root.join("usr/share/applications");
        fs::create_dir_all(&applications)?;
        fs::write(
            applications.join(format!("{identifier}.desktop")),
            desktop(config, &install_path),
        )?;
        fs::write(
            root.join("opt")
                .join(&identifier)
                .join(format!("{}.desktop", config.identifier())),
            desktop(config, &install_path),
        )?;
        let icons = root.join("usr/share/pixmaps");
        fs::create_dir_all(&icons)?;
        fs::copy(
            app.join("icon.png"),
            icons.join(format!("{identifier}.png")),
        )?;
        fs::create_dir(root.join("DEBIAN"))?;
        let arch = match architecture {
            TemplateTarget::LinuxArm64 => "arm64",
            TemplateTarget::LinuxX86_64 => "amd64",
            _ => return Err(Error::InvalidInput("expected a Linux template".into())),
        };
        fs::write(
            root.join("DEBIAN/control"),
            format!(
                "Package: {identifier}\nVersion: {}\nArchitecture: {arch}\nMaintainer: Rake <noreply@rake.invalid>\nDepends: libc6 (>= 2.39), libwebkit2gtk-4.1-0, libgtk-3-0, xdg-utils\nDescription: {}\n",
                env!("CARGO_PKG_VERSION"),
                name
            ),
        )?;
        run(Command::new("chmod").args(["-R", "u=rwX,go=rX"]).arg(&root))?;
        run(Command::new("dpkg-deb")
            .args(["--build", "--root-owner-group"])
            .arg(root)
            .arg(&archive))?;
    }
    check_destination(&destination, force)?;
    fs::rename(archive, &destination)?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Staging, PathBuf, RakeConfig) {
        let temp = Staging::new(&env::temp_dir()).unwrap();
        let template = temp.0.join("Rake");
        fs::create_dir(&template).unwrap();
        fs::write(template.join("rake-runtime"), "runtime").unwrap();
        fs::write(
            template.join("icon.png"),
            include_bytes!("../../../assets/icon.png"),
        )
        .unwrap();
        let mut config = RakeConfig::new("http://127.0.0.1:1");
        config.name = Some("My Linux App".into());
        config.icon = Some(template.join("icon.png").to_string_lossy().into_owned());
        (temp, template, config)
    }

    #[test]
    fn assembles_config_icon_injection_and_desktop_without_changing_template() {
        let (temp, template, mut config) = fixture();
        let script = temp.0.join("script.js");
        fs::write(&script, "window.injected = true").unwrap();
        config.inject = vec![script];
        let result = assemble(&config, &template, &temp.0.join("out"), false).unwrap();
        assert!(result.warnings.is_empty());
        assert_eq!(
            fs::read_to_string(result.path.join("rake-runtime")).unwrap(),
            "runtime"
        );
        assert_eq!(
            fs::read_to_string(result.path.join("inject/script.js")).unwrap(),
            "window.injected = true"
        );
        let written: RakeConfig =
            serde_json::from_slice(&fs::read(result.path.join("config.json")).unwrap()).unwrap();
        assert_eq!(written.inject, [PathBuf::from("script.js")]);
        let entry =
            fs::read_to_string(result.path.join(format!("{}.desktop", config.identifier())))
                .unwrap();
        assert!(entry.contains("Name=My Linux App\n"));
        assert!(entry.contains(&format!("Exec=\"{}/rake-runtime\"", result.path.display())));
        assert!(!template.join("config.json").exists());
        let tarball = package(
            &config,
            &result.path,
            &temp.0,
            Target::Tarball,
            false,
            TemplateTarget::LinuxArm64,
        )
        .unwrap();
        let listing = run(Command::new("tar").arg("-tzf").arg(tarball)).unwrap();
        assert!(listing.contains("My Linux App/config.json"));
    }

    #[test]
    fn failed_force_build_preserves_previous_output_and_rejects_unsafe_paths() {
        let (temp, template, mut config) = fixture();
        let output = temp.0.join("out");
        let app = assemble(&config, &template, &output, false).unwrap().path;
        assert_eq!(
            assemble(&config, &template, &output, false)
                .unwrap_err()
                .exit_code(),
            2
        );
        config.inject = vec![temp.0.join("missing.js")];
        assert!(assemble(&config, &template, &output, true).is_err());
        assert!(app.join("config.json").is_file());
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
        config.inject.clear();
        for name in ["../escape", "bad\nName", ".", "..", ""] {
            config.name = Some(name.into());
            assert!(assemble(&config, &template, &output, true).is_err());
        }
        config.name = Some("Rake".into());
        assert!(assemble(&config, &template, &temp.0, true).is_err());
        assert!(assemble(&config, &template, &template, true).is_err());
        assert!(template.join("rake-runtime").is_file());
    }

    #[test]
    fn unsupported_icon_keeps_default_and_warns() {
        let (temp, template, mut config) = fixture();
        config.icon = Some(temp.0.join("missing.svg").to_string_lossy().into_owned());
        let result = assemble(&config, &template, &temp.0.join("out"), false).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(
            fs::read(result.path.join("icon.png")).unwrap(),
            fs::read(template.join("icon.png")).unwrap()
        );
    }

    #[test]
    fn package_targets_are_validated_before_fetching() {
        assert!(Platform::Linux.validate(Target::Dmg, false).is_err());
        assert!(Platform::Linux.validate(Target::Deb, true).is_err());
        assert!(Platform::Macos.validate(Target::Deb, false).is_err());
        assert!(TemplateTarget::resolve("linux", "universal").is_err());
        assert_eq!(
            TemplateTarget::resolve("linux", "aarch64").unwrap(),
            TemplateTarget::LinuxArm64
        );
        assert_eq!(
            TemplateTarget::resolve("linux", "x86_64").unwrap(),
            TemplateTarget::LinuxX86_64
        );
    }

    #[test]
    fn option_like_names_are_tar_members_not_arguments() {
        let (temp, template, mut config) = fixture();
        config.name = Some("--checkpoint=1".into());
        let app = assemble(&config, &template, &temp.0.join("out"), false)
            .unwrap()
            .path;
        let archive = package(
            &config,
            &app,
            &temp.0,
            Target::Tarball,
            false,
            TemplateTarget::LinuxArm64,
        )
        .unwrap();
        let entries = run(Command::new("tar").arg("-tzf").arg(archive)).unwrap();
        assert!(entries.contains("--checkpoint=1/config.json"));
    }
}
