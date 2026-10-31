use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    time::Duration,
};

use gpui::{Context, Entity, FocusHandle, Window, actions, div, prelude::*, px, rgb};
use rake_config::RakeConfig;
use rake_core::{BuildOptions, BuildResult, BuildStep};

use crate::{Builder, form::ConfigDraft, input::TextInput};

actions!(build, [Build]);

pub struct BuildControls {
    output: Entity<TextInput>,
    focus: FocusHandle,
    state: BuildState,
}

#[derive(Debug, Default, PartialEq)]
enum BuildState {
    #[default]
    Idle,
    Running(BuildStep),
    Done(PathBuf),
    Failed(String),
}

impl BuildState {
    fn running(&self) -> bool {
        matches!(self, Self::Running(_))
    }

    fn label(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Running(step) => match step {
                BuildStep::Template => "Preparing template",
                BuildStep::Assembling => "Assembling",
                BuildStep::Signing => "Signing",
                BuildStep::Packaging => "Packaging",
                BuildStep::Installing => "Installing",
                BuildStep::Done => "Revealing app",
            }
            .into(),
            Self::Done(_) => "Done".into(),
            Self::Failed(error) => error.clone(),
        }
    }

    fn finish(&mut self, result: Result<PathBuf, String>) {
        *self = match result {
            Ok(path) => Self::Done(path),
            Err(error) => Self::Failed(error),
        };
    }
}

struct BuildRequest {
    config: RakeConfig,
    options: BuildOptions,
}

impl BuildRequest {
    fn from_draft(draft: &ConfigDraft, output: &str) -> Result<Self, String> {
        if output.trim().is_empty() {
            return Err("Output folder is required".into());
        }
        Ok(Self {
            config: draft.config()?,
            options: BuildOptions {
                output_dir: PathBuf::from(output),
                install: draft.install,
                ..Default::default()
            },
        })
    }

    fn run(self, progress: impl FnMut(BuildStep)) -> Result<BuildResult, String> {
        let mut warnings = Vec::new();
        let result = rake_core::build(&self.config, &self.options, &mut warnings, progress);
        for warning in warnings {
            eprintln!("warning: {warning}");
        }
        result.map_err(|error| error.to_string())
    }
}

fn reveal_command(app: &Path) -> Command {
    let mut command = Command::new("/usr/bin/open");
    command.arg("-R").arg(app);
    command
}

enum BuildUpdate {
    Progress(BuildStep),
    Finished(Result<PathBuf, String>),
}

impl BuildControls {
    pub fn new(cx: &mut Context<Builder>) -> Self {
        let output = std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Desktop"))
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            output: cx.new(|cx| TextInput::new(output.to_string_lossy().into_owned(), false, cx)),
            focus: cx.focus_handle().tab_stop(true),
            state: BuildState::Idle,
        }
    }
}

impl Builder {
    fn start_build(&mut self, _: &Build, _: &mut Window, cx: &mut Context<Self>) {
        if self.build.state.running() {
            return;
        }
        let request = match BuildRequest::from_draft(
            &self.draft,
            self.build.output.read(cx).content.as_ref(),
        ) {
            Ok(request) => request,
            Err(error) => {
                self.build.state = BuildState::Failed(error);
                cx.notify();
                return;
            }
        };
        self.build.state = BuildState::Running(BuildStep::Template);
        cx.notify();
        let (sender, receiver) = mpsc::channel();
        cx.background_executor()
            .spawn(async move {
                let result = request
                    .run(|step| {
                        let _ = sender.send(BuildUpdate::Progress(step));
                    })
                    .and_then(|result| {
                        let output = reveal_command(&result.app_path)
                            .output()
                            .map_err(|error| format!("Reveal app: {error}"))?;
                        if !output.status.success() {
                            return Err(format!(
                                "Reveal app: {}",
                                String::from_utf8_lossy(&output.stderr).trim()
                            ));
                        }
                        Ok(result.app_path)
                    });
                let _ = sender.send(BuildUpdate::Finished(result));
            })
            .detach();
        cx.spawn(async move |builder, cx| {
            loop {
                let finished = builder.update(cx, |builder, cx| {
                    let mut finished = false;
                    loop {
                        match receiver.try_recv() {
                            Ok(BuildUpdate::Progress(step)) => {
                                builder.build.state = BuildState::Running(step);
                            }
                            Ok(BuildUpdate::Finished(result)) => {
                                builder.build.state.finish(result);
                                finished = true;
                                break;
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => {
                                builder.build.state = BuildState::Failed("Build stopped".into());
                                finished = true;
                                break;
                            }
                        }
                    }
                    cx.notify();
                    finished
                });
                if finished.unwrap_or(true) {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(30))
                    .await;
            }
        })
        .detach();
    }

    pub fn build_controls(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.build.state.running();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child("Output folder")
            .child(self.build.output.clone())
            .child(
                div()
                    .id("build")
                    .key_context("Build")
                    .track_focus(&self.build.focus)
                    .on_action(cx.listener(Self::start_build))
                    .on_click(cx.listener(|builder, _, window, cx| {
                        builder.start_build(&Build, window, cx);
                    }))
                    .px_4()
                    .py_2()
                    .border_1()
                    .border_color(if self.build.focus.is_focused(window) {
                        rgb(0x202020)
                    } else {
                        rgb(0x0066cc)
                    })
                    .bg(if running {
                        rgb(0x888888)
                    } else {
                        rgb(0x0066cc)
                    })
                    .text_color(rgb(0xffffff))
                    .when(!running, |button| button.cursor_pointer())
                    .child("Build"),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .when(
                        matches!(self.build.state, BuildState::Failed(_)),
                        |status| status.text_color(rgb(0xb42318)),
                    )
                    .child(self.build.state.label()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Field;

    #[test]
    fn validates_and_snapshots_the_form_before_building() {
        let mut draft = ConfigDraft::default();
        assert!(BuildRequest::from_draft(&draft, "/output").is_err());
        draft.text[Field::Url as usize] = "https://example.com".into();
        assert!(BuildRequest::from_draft(&draft, " ").is_err());
        draft.install = true;
        let request = BuildRequest::from_draft(&draft, "/output").unwrap();
        draft.text[Field::Url as usize] = "https://changed.example".into();
        assert_eq!(request.config.url, "https://example.com");
        assert_eq!(request.options.output_dir, Path::new("/output"));
        assert!(request.options.install);
        assert!(!request.options.force);
    }

    #[test]
    fn completion_and_errors_allow_another_build() {
        let mut state = BuildState::Running(BuildStep::Signing);
        assert!(state.running());
        state.finish(Err("codesign failed".into()));
        assert!(!state.running());
        assert_eq!(state.label(), "codesign failed");
        state = BuildState::Running(BuildStep::Assembling);
        state.finish(Ok(PathBuf::from("/output/Example.app")));
        assert!(!state.running());
        assert_eq!(state.label(), "Done");
    }

    #[test]
    fn finder_reveal_keeps_the_app_path_as_one_argument() {
        let app = Path::new("/my apps/Example & Test.app");
        let command = reveal_command(app);
        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [std::ffi::OsStr::new("-R"), app.as_os_str()]
        );
    }

    #[test]
    #[ignore = "requires RAKE_CLI_BIN, RAKE_TEMPLATE and RAKE_BUILD_PROOF; retains signed apps without revealing Finder"]
    fn real_builder_matches_cli() {
        use std::fs;

        let required = |name| PathBuf::from(std::env::var_os(name).expect(name));
        let template = required("RAKE_TEMPLATE");
        let binary = required("RAKE_CLI_BIN");
        let proof = required("RAKE_BUILD_PROOF");
        let home = proof.join("home");
        let cache = home.join("Library/Caches/rake");
        let cached = cache
            .join("templates")
            .join(env!("CARGO_PKG_VERSION"))
            .join(rake_core::TemplateTarget::current().unwrap().as_str())
            .join("Rake.app");
        fs::create_dir_all(cached.parent().unwrap()).unwrap();
        assert!(
            Command::new("/usr/bin/ditto")
                .arg(&template)
                .arg(&cached)
                .status()
                .unwrap()
                .success()
        );
        let css = proof.join("style.css");
        let js = proof.join("script.js");
        fs::write(&css, "body { color: red }").unwrap();
        fs::write(&js, "window.rakeProof = true;").unwrap();
        let icon = template.join("Contents/Resources/icon.icns");
        let mut draft = ConfigDraft::default();
        for (field, value) in [
            (Field::Url, "https://example.com".to_owned()),
            (Field::Name, "Rake Build Proof".into()),
            (Field::Identifier, "org.example.rake-build-proof".into()),
            (Field::Icon, icon.to_string_lossy().into_owned()),
            (Field::Width, "1440".into()),
            (Field::Height, "900".into()),
            (Field::MinWidth, "640".into()),
            (Field::MinHeight, "480".into()),
            (Field::Title, "Build proof".into()),
            (Field::UserAgent, "Rake/Proof".into()),
            (
                Field::Inject,
                format!("{}\n{}", css.display(), js.display()),
            ),
            (Field::SafeDomains, "login.example.com\nexample.org".into()),
        ] {
            draft.text[field as usize] = value;
        }
        draft.hide_title_bar = true;
        draft.dark_mode = true;
        draft.debug = true;
        let builder_output = proof.join("builder");
        let mut request =
            BuildRequest::from_draft(&draft, builder_output.to_str().unwrap()).unwrap();
        request.options.template.cache_root = Some(cache);
        request.options.template.base_url = "http://127.0.0.1:1".into();
        request.options.force = true;
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || request.run(|step| sender.send(step).unwrap()));
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(10)).unwrap(),
            BuildStep::Template
        );
        let app = worker.join().unwrap().unwrap().app_path;
        assert_eq!(
            receiver.into_iter().collect::<Vec<_>>(),
            [BuildStep::Assembling, BuildStep::Signing, BuildStep::Done]
        );

        let cli_output = proof.join("cli");
        fs::create_dir_all(&cli_output).unwrap();
        let mut cli = Command::new(binary);
        cli.current_dir(&cli_output)
            .env("HOME", &home)
            .arg(&draft.text[Field::Url as usize]);
        for field in Field::ALL.into_iter().skip(1) {
            let flag = format!("--{}", field.key().replace('_', "-"));
            if field.is_list() {
                for value in draft.text[field as usize].lines() {
                    cli.arg(if matches!(field, Field::SafeDomains) {
                        "--safe-domain"
                    } else {
                        &flag
                    })
                    .arg(value);
                }
            } else {
                cli.arg(flag).arg(&draft.text[field as usize]);
            }
        }
        let output = cli
            .args([
                "--hide-title-bar",
                "--dark-mode",
                "--debug",
                "--force",
                "--json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cli_app = cli_output.join("Rake Build Proof.app");
        for bundle in [&app, &cli_app] {
            assert!(
                Command::new("/usr/bin/codesign")
                    .args(["--verify", "--deep", "--strict"])
                    .arg(bundle)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        for file in ["config.json", "inject/style.css", "inject/script.js"] {
            assert_eq!(
                fs::read(app.join("Contents/Resources").join(file)).unwrap(),
                fs::read(cli_app.join("Contents/Resources").join(file)).unwrap()
            );
        }
        println!(
            "Signed builder and CLI apps have identical config and injections at {}",
            proof.display()
        );
    }
}
