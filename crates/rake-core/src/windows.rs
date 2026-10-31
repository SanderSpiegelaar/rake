use super::*;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
};

pub(crate) fn powershell(script: &str) -> Command {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!("$ErrorActionPreference = 'Stop'; {script}"),
    ]);
    command
}

pub(crate) fn assemble_with_progress(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
    _progress: &mut impl FnMut(BuildStep),
) -> Result<AssemblyResult, Error> {
    assemble(config, template, output_dir, force)
}

pub(crate) fn install_default(_app: &Path) -> Result<(), Error> {
    Err(Error::InvalidInput("--install is macOS-only".into()))
}

pub fn assemble(
    config: &RakeConfig,
    template: &Path,
    output_dir: &Path,
    force: bool,
) -> Result<AssemblyResult, Error> {
    config
        .validate()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    let name = config.app_name();
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        })
    {
        return Err(Error::InvalidInput(
            "name: must be a valid Windows file name".into(),
        ));
    }
    if !Platform::Windows.complete_bundle(template) {
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
    fs::create_dir(&app)?;
    let executable = app.join(format!("{name}.exe"));
    fs::copy(template.join("rake-runtime.exe"), &executable)?;
    write_config(config, &app)?;
    let (icon, warnings) = icon(config, &template, &staging.0)?;
    replace_icon(&executable, &icon)?;
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

fn icon(
    config: &RakeConfig,
    template: &Path,
    work: &Path,
) -> Result<(Vec<u8>, Vec<String>), Error> {
    let downloaded = work.join("icon");
    let candidates = if let Some(source) = &config.icon {
        vec![source.clone()]
    } else {
        let html = work.join("page.html");
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
        if let Ok(bytes) = fs::read(source)
            && let Ok(icon) = super::windows_icon::ico(&bytes)
        {
            return Ok((icon, Vec::new()));
        }
    }
    if config.icon.is_some() {
        return Err(Error::InvalidInput(
            "icon: expected a readable ICO or PNG with dimensions from 1 to 256 pixels".into(),
        ));
    }
    Ok((
        super::windows_icon::ico(&fs::read(template.join("icon.png"))?)?,
        vec!["No usable site icon found; using the default icon".into()],
    ))
}

fn replace_icon(executable: &Path, ico: &[u8]) -> Result<(), Error> {
    let (group, images) = super::windows_icon::resources(ico)?;
    let path: Vec<u16> = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: The path is NUL-terminated. Resource buffers stay alive through each call.
    // The handle is committed once on success and discarded on every update failure.
    unsafe {
        let handle = BeginUpdateResourceW(path.as_ptr(), 0);
        if handle.is_null() {
            return Err(io::Error::last_os_error().into());
        }
        for (index, image) in images.iter().enumerate() {
            if UpdateResourceW(
                handle,
                3usize as *const u16,
                (index + 1) as *const u16,
                0,
                image.as_ptr().cast(),
                image.len() as u32,
            ) == 0
            {
                let error = io::Error::last_os_error();
                EndUpdateResourceW(handle, 1);
                return Err(error.into());
            }
        }
        if UpdateResourceW(
            handle,
            14usize as *const u16,
            std::ptr::without_provenance(1),
            0,
            group.as_ptr().cast(),
            group.len() as u32,
        ) == 0
        {
            let error = io::Error::last_os_error();
            EndUpdateResourceW(handle, 1);
            return Err(error.into());
        }
        if EndUpdateResourceW(handle, 0) == 0 {
            return Err(io::Error::last_os_error().into());
        }
    }
    Ok(())
}

pub(crate) fn package(
    config: &RakeConfig,
    app: &Path,
    output_dir: &Path,
    target: Target,
    force: bool,
    _architecture: TemplateTarget,
) -> Result<PathBuf, Error> {
    if target == Target::App {
        return Ok(app.to_path_buf());
    }
    if target != Target::Zip {
        return Err(Error::InvalidInput("expected Windows ZIP output".into()));
    }
    let destination = output_dir.join(format!("{}.zip", config.app_name()));
    check_destination(&destination, force)?;
    let staging = Staging::new(output_dir)?;
    let archive = staging.0.join("package.zip");
    run(powershell("Add-Type -AssemblyName System.IO.Compression.FileSystem; [System.IO.Compression.ZipFile]::CreateFromDirectory($env:RAKE_ZIP_SOURCE, $env:RAKE_ZIP_DESTINATION, [System.IO.Compression.CompressionLevel]::Optimal, $true)")
        .env("RAKE_ZIP_SOURCE", app).env("RAKE_ZIP_DESTINATION", &archive))?;
    let exists = check_destination(&destination, force)?;
    let old = staging.0.join("old.zip");
    if exists {
        fs::rename(&destination, &old)?;
    }
    if let Err(error) = fs::rename(&archive, &destination) {
        if exists {
            fs::rename(&old, &destination)?;
        }
        return Err(error.into());
    }
    Ok(destination)
}
