use std::{error::Error, fs, path::Path, process::Command};

use rake_config::RakeConfig;

fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("schema") => {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../schema");
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("rake.schema.json"), schema_json())?;
            Ok(())
        }
        Some("template") => template(std::env::args().nth(2).as_deref()),
        _ => {
            eprintln!("Usage: cargo xtask schema | template [arm64|x86_64|universal]");
            std::process::exit(2);
        }
    }
}

fn template(arch: Option<&str>) -> Result<(), Box<dyn Error>> {
    if !cfg!(target_os = "macos") {
        return Err("template requires macOS".into());
    }
    let arch = arch.unwrap_or(if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    });
    let targets: &[&str] = match arch {
        "arm64" => &["aarch64-apple-darwin"],
        "x86_64" => &["x86_64-apple-darwin"],
        "universal" => &["aarch64-apple-darwin", "x86_64-apple-darwin"],
        _ => return Err(format!("unsupported architecture: {arch}").into()),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = root.join("target/template");
    let bundle = output.join("Rake.app");
    let contents = bundle.join("Contents");
    let archive = format!(
        "rake-template-{}-macos-{arch}.tar.gz",
        env!("CARGO_PKG_VERSION")
    );
    for target in targets {
        run(Command::new("cargo").current_dir(root).args([
            "build",
            "--release",
            "--locked",
            "--package",
            "rake-runtime",
            "--target",
            target,
            "--target-dir",
            "target",
        ]))?;
    }
    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(contents.join("Resources"))?;
    let executable = contents.join("MacOS/rake-runtime");
    let binaries: Vec<_> = targets
        .iter()
        .map(|target| root.join(format!("target/{target}/release/rake-runtime")))
        .collect();
    if arch == "universal" {
        run(Command::new("lipo")
            .arg("-create")
            .args(&binaries)
            .arg("-output")
            .arg(&executable))?;
    } else {
        fs::copy(&binaries[0], &executable)?;
    }
    fs::write(contents.join("Info.plist"), template_plist())?;
    // A legacy monochrome ICNS entry keeps the replaceable placeholder dependency-free.
    let mut icon = b"icns\0\0\x01\x10ICN#\0\0\x01\x08".to_vec();
    icon.extend([0; 128]);
    icon.extend([255; 128]);
    fs::write(contents.join("Resources/icon.icns"), icon)?;
    run(Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&bundle))?;
    run(Command::new("tar")
        .current_dir(&output)
        .env("COPYFILE_DISABLE", "1")
        .args(["-czf", &archive, "Rake.app"]))?;
    let prefix = format!("rake-template-{}-macos-", env!("CARGO_PKG_VERSION"));
    let mut archives = fs::read_dir(&output)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    archives.retain(|name| {
        name.to_str()
            .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".tar.gz"))
    });
    archives.sort();
    let checksum = Command::new("shasum")
        .current_dir(&output)
        .args(["-a", "256"])
        .args(archives)
        .output()?;
    if !checksum.status.success() {
        return Err("shasum failed".into());
    }
    fs::write(output.join("SHA256SUMS"), &checksum.stdout)?;
    println!("{}", output.join(archive).display());
    print!("{}", String::from_utf8(checksum.stdout)?);
    Ok(())
}

fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    if !command.status()?.success() {
        return Err(format!("{command:?} failed").into());
    }
    Ok(())
}

fn template_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>rake-runtime</string>
    <key>CFBundleName</key><string>Rake</string>
    <key>CFBundleDisplayName</key><string>Rake</string>
    <key>CFBundleIdentifier</key><string>app.rake.template</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>{version}</string>
    <key>CFBundleVersion</key><string>{version}</string>
    <key>CFBundleIconFile</key><string>icon.icns</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSAppTransportSecurity</key><dict><key>NSAllowsArbitraryLoadsInWebContent</key><true/></dict>
</dict>
</plist>
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

fn schema_json() -> String {
    let schema = schemars::schema_for!(RakeConfig);
    format!("{}\n", serde_json::to_string_pretty(&schema).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn template_metadata_matches_runtime_and_workspace_version() {
        let plist = template_plist();
        for value in [
            "rake-runtime",
            "icon.icns",
            "APPL",
            env!("CARGO_PKG_VERSION"),
        ] {
            assert!(plist.contains(&format!("<string>{value}</string>")));
        }
        assert!(plist.contains("<key>NSAllowsArbitraryLoadsInWebContent</key><true/>"));
    }

    #[test]
    fn committed_schema_is_current() {
        // GPUI enables serde_json/preserve_order, but JSON object order is not part of the schema.
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(include_str!(
                "../../schema/rake.schema.json"
            ))
            .unwrap(),
            serde_json::from_str::<serde_json::Value>(&schema_json()).unwrap(),
            "Run cargo xtask schema to update the committed schema"
        );
    }

    #[test]
    fn schema_reflects_serde_contract() {
        let schema = serde_json::to_value(schemars::schema_for!(RakeConfig)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"]["url"]["type"], "string");
        for (field, default) in [
            ("schema_version", json!(1)),
            ("width", json!(1200)),
            ("height", json!(780)),
            ("hide_title_bar", json!(false)),
            ("dark_mode", json!(false)),
            ("debug", json!(false)),
        ] {
            assert_eq!(schema["properties"][field]["default"], default, "{field}");
        }
    }
}
