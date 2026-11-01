#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::{env, ffi::OsString, fs, path::Path, rc::Rc};

use rake_config::RakeConfig;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Theme, WindowBuilder},
};
use wry::{NewWindowResponse, PageLoadEvent, WebViewBuilder};

mod navigation;

use navigation::NavigationPolicy;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (config, scripts) = env::current_exe()
        .map_err(|error| {
            format!("Cannot locate the executable to find ../Resources/config.json: {error}")
        })
        .and_then(|exe| load_config(&exe, env::var_os("RAKE_CONFIG")))
        .unwrap_or_else(|error| {
            show_error(&error);
            std::process::exit(1);
        });
    if config.debug {
        eprintln!("Rake: config loaded for {}", config.url);
    }
    let event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let navigation = Rc::new(NavigationPolicy::new(&config));
    let mut window = WindowBuilder::new()
        .with_title(config.title.clone().unwrap_or_else(|| config.app_name()))
        .with_inner_size(LogicalSize::new(config.width, config.height))
        .with_theme(config.dark_mode.then_some(Theme::Dark));
    if config.min_width.is_some() || config.min_height.is_some() {
        window = window.with_min_inner_size(LogicalSize::new(
            config.min_width.unwrap_or(0),
            config.min_height.unwrap_or(0),
        ));
    }
    #[cfg(target_os = "macos")]
    if config.hide_title_bar {
        use tao::platform::macos::WindowBuilderExtMacOS;

        window = window
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true);
    }
    #[cfg(windows)]
    {
        use tao::{
            platform::windows::{IconExtWindows, WindowBuilderExtWindows},
            window::Icon,
        };
        if let Ok(icon) = Icon::from_resource(1, None) {
            window = window
                .with_window_icon(Some(icon.clone()))
                .with_taskbar_icon(Some(icon));
        }
    }
    let window = Rc::new(window.build(&event_loop)?);
    let mut webview = WebViewBuilder::new()
        .with_url(&config.url)
        .with_devtools(config.debug);
    for script in scripts {
        webview = webview.with_initialization_script(script);
    }
    let popup_navigation = Rc::clone(&navigation);
    let proxy = event_loop.create_proxy();
    webview = webview
        .with_navigation_handler(move |url| navigation.navigate(&url))
        .with_new_window_req_handler(move |url, _| {
            if popup_navigation.navigate(&url) {
                let _ = proxy.send_event(url);
            }
            NewWindowResponse::Deny
        });
    if config.debug {
        webview = webview.with_on_page_load_handler(|event, url| {
            let state = match event {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "finished",
            };
            eprintln!("Rake: page load {state}: {url}");
        });
    }
    if let Some(user_agent) = &config.user_agent {
        webview = webview.with_user_agent(user_agent);
    }
    #[cfg(target_os = "macos")]
    if config.hide_title_bar {
        let window = Rc::clone(&window);
        // The webview covers the titlebar, so its top 28 CSS pixels restore native dragging.
        webview = webview
            .with_initialization_script(
                r#"
                for (const [event, message] of [["mousedown", "rake:drag"], ["dblclick", "rake:zoom"]]) {
                    document.addEventListener(event, (event) => {
                        if (event.button === 0 && event.clientY >= 0 && event.clientY < 28 &&
                            event.target instanceof Element &&
                            !event.target.closest('a, button, input, select, textarea, label, [role="button"], [contenteditable]')) {
                            window.ipc.postMessage(message);
                        }
                    });
                }
                "#,
            )
            .with_ipc_handler(move |request| match request.body().as_str() {
                "rake:drag" => {
                    let _ = window.drag_window();
                }
                "rake:zoom" => window.set_maximized(!window.is_maximized()),
                _ => {}
            });
    }
    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        webview.build_gtk(window.default_vbox().expect("tao creates a GTK box"))?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = webview.build(window.as_ref())?;
    if config.debug {
        webview.open_devtools();
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::UserEvent(url) = &event
            && let Err(error) = webview.load_url(url)
        {
            eprintln!("Cannot navigate to {url}: {error}");
        }
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

fn load_config(
    executable: &Path,
    fallback: Option<OsString>,
) -> Result<(RakeConfig, Vec<String>), String> {
    #[cfg(target_os = "macos")]
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
    #[cfg(not(target_os = "macos"))]
    let bundled = executable
        .parent()
        .ok_or("Cannot find executable directory")?
        .join("config.json");
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
    let scripts = load_inject_scripts(&config, path.parent().unwrap_or(Path::new(".")))?;
    Ok((config, scripts))
}

fn load_inject_scripts(config: &RakeConfig, resources: &Path) -> Result<Vec<String>, String> {
    config
        .inject
        .iter()
        .map(|file| {
            if file.as_os_str().is_empty()
                || file.components().any(|component| {
                    !matches!(
                        component,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
            {
                return Err(format!("Inject file must be relative to inject/: {}", file.display()));
            }
            let path = resources.join("inject").join(file);
            let contents = fs::read_to_string(&path)
                .map_err(|error| format!("Cannot read inject file {}: {error}", path.display()))?;
            match file.extension().and_then(|extension| extension.to_str()) {
                Some("js") => Ok(contents),
                Some("css") => {
                    let css = serde_json::to_string(&contents).map_err(|error| {
                        format!("Cannot encode inject file {}: {error}", path.display())
                    })?;
                    Ok(format!(
                        r#"(() => {{
                            const apply = () => {{
                                const style = document.createElement("style");
                                style.textContent = {css};
                                document.head.appendChild(style);
                            }};
                            if (document.readyState === "loading") {{
                                document.addEventListener("DOMContentLoaded", apply, {{ once: true }});
                            }} else {{
                                apply();
                            }}
                        }})();"#
                    ))
                }
                _ => Err(format!("Inject file must be CSS or JS: {}", path.display())),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_inject_files_in_order_and_names_errors() {
        let root = env::temp_dir().join(format!("rake-inject-test-{}", std::process::id()));
        fs::create_dir_all(root.join("inject/nested")).unwrap();
        let css = "body::before { content: 'quotes \\\" and `ticks` ${value}'; }\n";
        let js = "window.injected = true;";
        fs::write(root.join("inject/nested/style.css"), css).unwrap();
        fs::write(root.join("inject/script.js"), js).unwrap();
        let mut config = RakeConfig::new("https://example.com");
        assert!(load_inject_scripts(&config, &root).unwrap().is_empty());
        config.inject = vec!["nested/style.css".into(), "script.js".into()];
        let scripts = load_inject_scripts(&config, &root).unwrap();
        assert_eq!(scripts.len(), 2);
        assert!(scripts[0].contains(&serde_json::to_string(css).unwrap()));
        assert!(scripts[0].contains("DOMContentLoaded"));
        assert_eq!(scripts[1], js);

        for (file, reason) in [
            ("missing.js", "Cannot read inject file"),
            ("../script.js", "must be relative"),
            ("/script.js", "must be relative"),
        ] {
            config.inject = vec![file.into()];
            let error = load_inject_scripts(&config, &root).unwrap_err();
            assert!(error.contains(file), "{error}");
            assert!(error.contains(reason), "{error}");
        }
        fs::write(root.join("inject/file.txt"), "text").unwrap();
        config.inject = vec!["file.txt".into()];
        assert!(
            load_inject_scripts(&config, &root)
                .unwrap_err()
                .contains("file.txt")
        );
        fs::write(root.join("inject/invalid.js"), [0xff]).unwrap();
        config.inject = vec!["invalid.js".into()];
        assert!(
            load_inject_scripts(&config, &root)
                .unwrap_err()
                .contains("invalid.js")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(target_os = "macos")]
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
                .0
                .url,
            "https://www.rust-lang.org"
        );
        fs::write(&bundled, r#"{"url":"https://example.com"}"#).unwrap();
        assert_eq!(
            load_config(&exe, Some(fallback.clone().into_os_string()))
                .unwrap()
                .0
                .url,
            "https://example.com"
        );

        fs::create_dir_all(resources.join("inject")).unwrap();
        fs::write(resources.join("inject/bundle.js"), "window.bundled = true;").unwrap();
        fs::write(
            &bundled,
            r#"{"url":"https://example.com","inject":["bundle.js"]}"#,
        )
        .unwrap();
        assert_eq!(
            load_config(&exe, None).unwrap().1,
            ["window.bundled = true;"]
        );

        fs::create_dir_all(root.join("inject")).unwrap();
        fs::write(root.join("inject/dev.js"), "window.dev = true;").unwrap();
        fs::write(
            &fallback,
            r#"{"url":"https://example.com","inject":["dev.js"]}"#,
        )
        .unwrap();
        fs::remove_file(&bundled).unwrap();
        assert_eq!(
            load_config(&exe, Some(fallback.clone().into_os_string()))
                .unwrap()
                .1,
            ["window.dev = true;"]
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
