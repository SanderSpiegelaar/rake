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
        Some("template") => template(),
        _ => {
            eprintln!("Usage: cargo xtask <schema|template>");
            std::process::exit(2);
        }
    }
}

fn template() -> Result<(), Box<dyn Error>> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err("template requires an Apple Silicon Mac".into());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = root.join("target/template");
    let bundle = output.join("Rake.app");
    let contents = bundle.join("Contents");
    let archive = format!(
        "rake-template-{}-macos-arm64.tar.gz",
        env!("CARGO_PKG_VERSION")
    );
    run(Command::new("cargo").current_dir(root).args([
        "build",
        "--release",
        "--locked",
        "--package",
        "rake-runtime",
        "--target",
        "aarch64-apple-darwin",
        "--target-dir",
        "target",
    ]))?;
    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir_all(contents.join("Resources"))?;
    fs::copy(
        root.join("target/aarch64-apple-darwin/release/rake-runtime"),
        contents.join("MacOS/rake-runtime"),
    )?;
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
    let checksum = Command::new("shasum")
        .current_dir(&output)
        .args(["-a", "256", &archive])
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
        assert_eq!(
            include_str!("../../schema/rake.schema.json"),
            schema_json(),
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
