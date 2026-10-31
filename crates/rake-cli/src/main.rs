use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::Instant,
};

use clap::{Parser, ValueEnum};
use rake_config::RakeConfig;
use rake_core::{Error, Target, TemplateOptions, TemplateTarget};

#[derive(Debug, Parser)]
#[command(name = "rake", version = concat!(env!("CARGO_PKG_VERSION"), " (template ", env!("CARGO_PKG_VERSION"), ")"))]
struct Args {
    #[arg(required_unless_present = "config")]
    url: Option<String>,
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    identifier: Option<String>,
    #[arg(long)]
    icon: Option<String>,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
    #[arg(long)]
    min_width: Option<u32>,
    #[arg(long)]
    min_height: Option<u32>,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    user_agent: Option<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    hide_title_bar: Option<bool>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    dark_mode: Option<bool>,
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    debug: Option<bool>,
    #[arg(long, value_name = "FILE")]
    inject: Option<Vec<PathBuf>>,
    #[arg(long = "safe-domain", value_name = "DOMAIN")]
    safe_domains: Option<Vec<String>>,
    #[arg(long, value_enum, default_value = "app")]
    target: OutputTarget,
    #[arg(long, value_enum)]
    arch: Option<Architecture>,
    #[arg(long)]
    install: bool,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputTarget {
    App,
    Dmg,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Architecture {
    Arm64,
    #[value(name = "x86_64")]
    X86_64,
    Universal,
}

impl From<Architecture> for TemplateTarget {
    fn from(arch: Architecture) -> Self {
        match arch {
            Architecture::Arm64 => Self::MacosArm64,
            Architecture::X86_64 => Self::MacosX86_64,
            Architecture::Universal => Self::MacosUniversal,
        }
    }
}

impl From<OutputTarget> for Target {
    fn from(target: OutputTarget) -> Self {
        match target {
            OutputTarget::App => Self::App,
            OutputTarget::Dmg => Self::Dmg,
        }
    }
}

impl Args {
    fn merge(&self) -> Result<RakeConfig, Error> {
        let mut config = if let Some(path) = &self.config {
            let bytes = fs::read(path).map_err(|error| {
                Error::InvalidInput(format!("config {}: {error}", path.display()))
            })?;
            let mut config: RakeConfig = serde_json::from_slice(&bytes).map_err(|error| {
                Error::InvalidInput(format!("config {}: {error}", path.display()))
            })?;
            let directory = path.parent().unwrap_or(Path::new("."));
            if let Some(icon) = &mut config.icon
                && !icon.split_once("://").is_some_and(|(scheme, _)| {
                    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
                })
            {
                *icon = directory.join(&*icon).to_string_lossy().into_owned();
            }
            for file in &mut config.inject {
                *file = directory.join(&*file);
            }
            config
        } else {
            RakeConfig::new(self.url.clone().unwrap_or_default())
        };
        config.url = self.url.clone().unwrap_or(config.url);
        config.name = self.name.clone().or(config.name);
        config.identifier = self.identifier.clone().or(config.identifier);
        config.icon = self.icon.clone().or(config.icon);
        config.width = self.width.unwrap_or(config.width);
        config.height = self.height.unwrap_or(config.height);
        config.min_width = self.min_width.or(config.min_width);
        config.min_height = self.min_height.or(config.min_height);
        config.title = self.title.clone().or(config.title);
        config.user_agent = self.user_agent.clone().or(config.user_agent);
        config.hide_title_bar = self.hide_title_bar.unwrap_or(config.hide_title_bar);
        config.dark_mode = self.dark_mode.unwrap_or(config.dark_mode);
        config.debug = self.debug.unwrap_or(config.debug);
        config.inject = self.inject.clone().unwrap_or(config.inject);
        config.safe_domains = self.safe_domains.clone().unwrap_or(config.safe_domains);
        config
            .validate()
            .map_err(|error| Error::InvalidInput(error.to_string()))?;
        Ok(config)
    }
}

fn run(args: &Args, warnings: &mut Vec<String>) -> Result<PathBuf, Error> {
    let config = args.merge()?;
    let output = std::env::current_dir()
        .map_err(|error| Error::Unexpected(format!("current directory: {error}")))?;
    let template = rake_core::ensure_template(&TemplateOptions {
        target: args.arch.map(TemplateTarget::from),
        ..Default::default()
    })
    .map_err(|error| Error::MissingTemplate(error.to_string()))?;
    let app = rake_core::assemble(&config, &template, &output, args.force)?;
    *warnings = app.warnings;
    for warning in warnings.iter() {
        eprintln!("warning: {warning}");
    }
    let result = match Target::from(args.target) {
        Target::App => app.path.clone(),
        Target::Dmg => rake_core::package_dmg(&app.path, &output, args.force)?,
    };
    if args.install {
        rake_core::install(&app.path, Path::new("/Applications"))?;
    }
    eprintln!("{}", result.display());
    Ok(app.path)
}

fn main() {
    let started = Instant::now();
    let arguments: Vec<_> = std::env::args_os().collect();
    let json = arguments
        .iter()
        .skip(1)
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json");
    let mut warnings = Vec::new();
    let (app_path, error, code) = match Args::try_parse_from(arguments) {
        Ok(args) => match run(&args, &mut warnings) {
            Ok(path) => (Some(path), None, 0),
            Err(error) => {
                eprintln!("error: {error}");
                (None, Some(error.to_string()), error.exit_code())
            }
        },
        Err(error) => {
            let code = error.exit_code();
            eprint!("{error}");
            (None, (code != 0).then(|| error.to_string()), code)
        }
    };
    if json {
        let result = serde_json::json!({
            "status": if code == 0 { "success" } else { "error" },
            "app_path": app_path.as_deref().map(Path::to_string_lossy),
            "duration": started.elapsed().as_secs_f64(),
            "warnings": warnings,
            "error": error,
        });
        if let Err(error) = writeln!(io::stdout().lock(), "{result}") {
            eprintln!("error: JSON output: {error}");
            process::exit(1);
        }
    }
    process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn with_config(contents: &str, test: impl FnOnce(&Path)) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "rake-cli-{}-{}.json",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, contents).unwrap();
        test(&path);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn flags_file_and_mixed_inputs_produce_equal_configs() {
        let flags = Args::try_parse_from([
            "rake",
            "https://example.com",
            "--name",
            "Example",
            "--identifier",
            "org.example.app",
            "--icon",
            "https://example.com/icon.png",
            "--width",
            "1440",
            "--height",
            "900",
            "--min-width",
            "640",
            "--min-height",
            "480",
            "--title",
            "Example App",
            "--user-agent",
            "Rake/1",
            "--hide-title-bar",
            "--dark-mode",
            "--debug",
            "--inject",
            "/tmp/style.css",
            "--inject",
            "/tmp/script.js",
            "--safe-domain",
            "example.com",
            "--safe-domain",
            "login.example.com",
        ])
        .unwrap()
        .merge()
        .unwrap();
        with_config(&serde_json::to_string(&flags).unwrap(), |path| {
            let file = Args::try_parse_from(["rake", "--config", path.to_str().unwrap()])
                .unwrap()
                .merge()
                .unwrap();
            assert_eq!(flags, file);
        });
        let mut base = flags.clone();
        base.url = "invalid until overridden".into();
        base.width = 1;
        base.title = Some("Old title".into());
        base.dark_mode = false;
        base.inject = vec!["old.css".into()];
        base.safe_domains = vec!["old.example.com".into()];
        with_config(&serde_json::to_string(&base).unwrap(), |path| {
            let mixed = Args::try_parse_from([
                "rake",
                "https://example.com",
                "--config",
                path.to_str().unwrap(),
                "--width",
                "1440",
                "--title",
                "Example App",
                "--dark-mode",
                "--inject",
                "/tmp/style.css",
                "--inject",
                "/tmp/script.js",
                "--safe-domain",
                "example.com",
                "--safe-domain",
                "login.example.com",
            ])
            .unwrap()
            .merge()
            .unwrap();
            assert_eq!(flags, mixed);
        });
    }

    #[test]
    fn omitted_booleans_preserve_file_values_and_false_overrides_them() {
        with_config(
            r#"{"url":"https://example.com","dark_mode":true,"debug":true,"hide_title_bar":true}"#,
            |path| {
                let config = Args::try_parse_from([
                    "rake",
                    "--config",
                    path.to_str().unwrap(),
                    "--dark-mode=false",
                    "--debug=false",
                ])
                .unwrap()
                .merge()
                .unwrap();
                assert!(!config.dark_mode);
                assert!(!config.debug);
                assert!(config.hide_title_bar);
            },
        );
    }

    #[test]
    fn file_paths_are_relative_to_config_but_flag_paths_are_not() {
        with_config(
            r#"{"url":"https://example.com","icon":"icon.png","inject":["style.css","/abs.js"]}"#,
            |path| {
                let config = Args::try_parse_from(["rake", "--config", path.to_str().unwrap()])
                    .unwrap()
                    .merge()
                    .unwrap();
                let directory = path.parent().unwrap();
                assert_eq!(
                    config.icon,
                    Some(directory.join("icon.png").to_string_lossy().into_owned())
                );
                assert_eq!(
                    config.inject,
                    [directory.join("style.css"), PathBuf::from("/abs.js")]
                );
                let config = Args::try_parse_from([
                    "rake",
                    "--config",
                    path.to_str().unwrap(),
                    "--icon",
                    "override.png",
                ])
                .unwrap()
                .merge()
                .unwrap();
                assert_eq!(config.icon.as_deref(), Some("override.png"));
            },
        );
    }

    #[test]
    fn merged_inputs_are_validated() {
        for args in [
            vec!["rake", "not-a-url"],
            vec!["rake", "https://example.com", "--width", "0"],
            vec!["rake", "https://example.com", "--height", "0"],
            vec!["rake", "https://example.com", "--min-width", "1201"],
            vec![
                "rake",
                "https://example.com",
                "--safe-domain",
                "https://example.com",
            ],
            vec!["rake", "https://example.com", "--identifier", "invalid"],
        ] {
            assert!(matches!(
                Args::try_parse_from(args).unwrap().merge(),
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(Args::try_parse_from(["rake"]).is_err());
        with_config("{", |path| {
            assert!(matches!(
                Args::try_parse_from(["rake", "--config", path.to_str().unwrap()])
                    .unwrap()
                    .merge(),
                Err(Error::InvalidInput(_))
            ));
        });
    }

    #[test]
    fn architecture_is_independent_of_output_format() {
        let defaults = Args::try_parse_from(["rake", "https://example.com"]).unwrap();
        assert!(defaults.arch.is_none());
        for (arch, expected) in [
            ("arm64", TemplateTarget::MacosArm64),
            ("x86_64", TemplateTarget::MacosX86_64),
            ("universal", TemplateTarget::MacosUniversal),
        ] {
            for target in ["app", "dmg"] {
                let args = Args::try_parse_from([
                    "rake",
                    "https://example.com",
                    "--arch",
                    arch,
                    "--target",
                    target,
                ])
                .unwrap();
                assert_eq!(args.arch.map(TemplateTarget::from), Some(expected));
                assert!(matches!(
                    (args.target, target),
                    (OutputTarget::App, "app") | (OutputTarget::Dmg, "dmg")
                ));
            }
        }
        assert!(
            Args::try_parse_from(["rake", "https://example.com", "--arch", "invalid"]).is_err()
        );
    }

    #[test]
    fn clap_definition_is_consistent() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
