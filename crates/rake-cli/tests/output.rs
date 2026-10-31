#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

struct Fixture {
    root: PathBuf,
    home: PathBuf,
    output: PathBuf,
    template: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "rake-output-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let home = root.join("home");
        let output = root.join("output");
        let template = home.join(format!(
            "Library/Caches/rake/templates/{}/macos-arm64/Rake.app",
            env!("CARGO_PKG_VERSION")
        ));
        fs::create_dir_all(template.join("Contents/MacOS")).unwrap();
        fs::create_dir_all(template.join("Contents/Resources")).unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        let curl = root.join("bin/curl");
        fs::write(
            &curl,
            "#!/bin/sh\necho 'network disabled by test' >&2\nexit 22\n",
        )
        .unwrap();
        fs::set_permissions(curl, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(template.join("Contents/MacOS/rake-runtime"), "runtime").unwrap();
        fs::write(
            template.join("Contents/Resources/icon.icns"),
            "default icon",
        )
        .unwrap();
        fs::write(template.join("Contents/Info.plist"), r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleExecutable</key><string>rake-runtime</string></dict></plist>"#).unwrap();
        Self {
            root,
            home,
            output,
            template,
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rake"));
        command
            .current_dir(&self.output)
            .env("HOME", &self.home)
            .env("PATH", self.root.join("bin"));
        command
    }

    fn build(&self) -> Command {
        let mut command = self.command();
        command.args(["https://example.com", "--name", "Example", "--json"]);
        command
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn assert_result(output: Output, code: i32) -> Value {
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(code), "{stderr}");
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result.is_object());
    assert_eq!(
        result["status"],
        if code == 0 { "success" } else { "error" }
    );
    assert!(result["duration"].as_f64().unwrap() >= 0.0);
    assert!(result["warnings"].is_array());
    if code == 0 {
        assert!(result["error"].is_null());
    } else {
        assert!(result["app_path"].is_null());
        assert!(!result["error"].as_str().unwrap().is_empty());
        assert!(!stderr.is_empty());
    }
    result
}

#[test]
fn success_returns_app_path_duration_and_warnings_with_exit_zero() {
    let fixture = Fixture::new();
    let output = fixture.build().output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let result = assert_result(output, 0);
    let app = fixture.output.join("Example.app");
    assert_eq!(result["app_path"], app.to_str().unwrap());
    assert_eq!(result["warnings"].as_array().unwrap().len(), 1);
    assert!(stderr.contains(result["warnings"][0].as_str().unwrap()));
    assert!(stderr.contains(app.to_str().unwrap()));
    assert!(app.join("Contents/Resources/config.json").is_file());
    assert!(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(app)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn unexpected_deleted_working_directory_exits_one() {
    let fixture = Fixture::new();
    let output = Command::new("/bin/sh")
        .current_dir(&fixture.output)
        .env("HOME", &fixture.home)
        .env("PATH", fixture.root.join("bin"))
        .args([
            "-c",
            "/bin/rmdir \"$PWD\"; exec \"$1\" https://example.com --json",
            "rake-test",
            env!("CARGO_BIN_EXE_rake"),
        ])
        .output()
        .unwrap();
    let result = assert_result(output, 1);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("current directory")
    );
}

#[test]
fn bad_input_and_parser_errors_exit_two() {
    let fixture = Fixture::new();
    for args in [
        vec!["--json"],
        vec!["--json", "not-a-url"],
        vec!["https://example.com", "--width", "no", "--json"],
        vec!["--unknown", "--json"],
        vec!["--config", "missing.json", "--json"],
    ] {
        assert_result(fixture.command().args(args).output().unwrap(), 2);
    }
}

#[test]
fn assembly_and_signing_failures_exit_three() {
    let fixture = Fixture::new();
    let plist = fixture.template.join("Contents/Info.plist");
    let original = fs::read(&plist).unwrap();
    fs::write(&plist, "broken plist").unwrap();
    let result = assert_result(fixture.build().output().unwrap(), 3);
    assert!(result["error"].as_str().unwrap().contains("plutil"));
    assert!(!fixture.output.join("Example.app").exists());

    fs::write(plist, original).unwrap();
    assert!(
        Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.ResourceFork", "unsignable"])
            .arg(fixture.template.join("Contents/MacOS/rake-runtime"))
            .status()
            .unwrap()
            .success()
    );
    let result = assert_result(fixture.build().output().unwrap(), 3);
    assert!(result["error"].as_str().unwrap().contains("codesign"));
    assert!(!fixture.output.join("Example.app").exists());
}

#[test]
fn missing_environment_and_unavailable_template_exit_four() {
    let fixture = Fixture::new();
    assert_result(fixture.build().env_remove("HOME").output().unwrap(), 4);
    fs::remove_dir_all(&fixture.template).unwrap();
    let result = assert_result(fixture.build().output().unwrap(), 4);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("network disabled by test")
    );
    assert_result(
        fixture
            .build()
            .env("PATH", fixture.root.join("missing-bin"))
            .output()
            .unwrap(),
        4,
    );
}

#[test]
fn packaging_failure_retains_warnings_in_json() {
    let fixture = Fixture::new();
    fs::write(fixture.output.join("Example.dmg"), "existing image").unwrap();
    let result = assert_result(
        fixture.build().args(["--target", "dmg"]).output().unwrap(),
        2,
    );
    assert_eq!(result["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read(fixture.output.join("Example.dmg")).unwrap(),
        b"existing image"
    );
}

#[test]
fn human_output_stays_on_stderr_and_help_remains_available() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["https://example.com", "--name", "Example"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Example.app"));
    let output = fixture.command().args(["not-a-url"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    for flag in ["--help", "--version"] {
        let output = fixture.command().arg(flag).output().unwrap();
        assert!(output.status.success());
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
        assert_result(
            fixture.command().args(["--json", flag]).output().unwrap(),
            0,
        );
    }
}
