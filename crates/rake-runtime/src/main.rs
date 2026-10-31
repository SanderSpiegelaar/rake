use std::{env, ffi::OsString, fs, path::Path};

use rake_config::RakeConfig;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::WebViewBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = env::current_exe()
        .map_err(|error| {
            format!("Cannot locate the executable to find ../Resources/config.json: {error}")
        })
        .and_then(|exe| load_config(&exe, env::var_os("RAKE_CONFIG")))
        .unwrap_or_else(|error| {
            show_error(&error);
            std::process::exit(1);
        });
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Rake")
        .with_inner_size(LogicalSize::new(config.width, config.height))
        .build(&event_loop)?;
    let _webview = WebViewBuilder::new().with_url(&config.url).build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(target_os = "macos")]
fn show_error(message: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::NSString;

    let mtm = MainThreadMarker::new().expect("main runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    app.finishLaunching();
    app.activate();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("Rake"));
    alert.setInformativeText(&NSString::from_str(message));
    alert.runModal();
}

#[cfg(not(target_os = "macos"))]
fn show_error(message: &str) {
    eprintln!("{message}");
}

fn load_config(executable: &Path, fallback: Option<OsString>) -> Result<RakeConfig, String> {
    let bundled = executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| {
            format!(
                "Cannot find the executable directory for {}",
                executable.display()
            )
        })?
        .join("Resources/config.json");
    let path = if bundled.exists() {
        bundled
    } else if let Some(path) = fallback {
        path.into()
    } else {
        return Err(format!(
            "Config not found at {} and RAKE_CONFIG is not set",
            bundled.display()
        ));
    };
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let config: RakeConfig = serde_json::from_str(&contents)
        .map_err(|error| format!("Invalid config {}: {error}", path.display()))?;
    config
        .validate()
        .map_err(|error| format!("Invalid config {}: {error}", path.display()))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_bundle_before_fallback_and_reports_config_errors() {
        let root = env::temp_dir().join(format!("rake-config-test-{}", std::process::id()));
        let macos = root.join("Contents/MacOS");
        let resources = root.join("Contents/Resources");
        fs::create_dir_all(&macos).unwrap();
        fs::create_dir_all(&resources).unwrap();
        let exe = macos.join("rake-runtime");
        let bundled = resources.join("config.json");
        let fallback = root.join("dev.json");

        let error = load_config(&exe, None).unwrap_err();
        assert!(error.contains("Resources/config.json"));
        assert!(error.contains("RAKE_CONFIG is not set"));

        fs::write(&fallback, r#"{"url":"https://www.rust-lang.org"}"#).unwrap();
        assert_eq!(
            load_config(&exe, Some(fallback.clone().into_os_string()))
                .unwrap()
                .url,
            "https://www.rust-lang.org"
        );
        fs::write(&bundled, r#"{"url":"https://example.com"}"#).unwrap();
        assert_eq!(
            load_config(&exe, Some(fallback.clone().into_os_string()))
                .unwrap()
                .url,
            "https://example.com"
        );

        for (contents, reason) in [
            (r#"{"url":"ftp://x"}"#, "url: must be an http or https URL"),
            ("{", "line 1 column 1"),
            (r#"{"url":"https://example.com","widht":5}"#, "widht"),
        ] {
            fs::write(&bundled, contents).unwrap();
            let error = load_config(&exe, Some(fallback.clone().into_os_string())).unwrap_err();
            assert!(error.contains("Resources/config.json"), "{error}");
            assert!(error.contains(reason), "{error}");
        }

        fs::remove_file(&bundled).unwrap();
        fs::remove_file(&fallback).unwrap();
        let error = load_config(&exe, Some(fallback.clone().into_os_string())).unwrap_err();
        assert!(error.contains(&fallback.display().to_string()), "{error}");
        assert!(error.contains("Cannot read"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }
}
