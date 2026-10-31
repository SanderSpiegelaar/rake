use std::{
    fs,
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, ValueEnum};
use rake_config::RakeConfig;
use rake_core::{Error, Target, TemplateOptions};

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
    #[arg(long)]
    install: bool,
    #[arg(long)]
    force: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputTarget {
    App,
    Dmg,
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
            if self.icon.is_none()
                && let Some(icon) = &mut config.icon
                && !icon.split_once("://").is_some_and(|(scheme, _)| {
                    scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
                })
                && Path::new(icon).is_relative()
            {
                *icon = path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(&*icon)
                    .to_string_lossy()
                    .into_owned();
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

fn run(args: Args) -> Result<(), Error> {
    let config = args.merge()?;
    let template = rake_core::ensure_template(&TemplateOptions::default())?;
    let output = std::env::current_dir()?;
    let app = rake_core::assemble(&config, &template, &output, args.force)?;
    for warning in &app.warnings {
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
    Ok(())
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("error: {error}");
        process::exit(error.exit_code());
    }
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
            "style.css",
            "--inject",
            "script.js",
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
                "style.css",
                "--inject",
                "script.js",
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
    fn file_icon_paths_are_relative_to_config_but_flag_paths_are_not() {
        with_config(
            r#"{"url":"https://example.com","icon":"icon.png"}"#,
            |path| {
                let config = Args::try_parse_from(["rake", "--config", path.to_str().unwrap()])
                    .unwrap()
                    .merge()
                    .unwrap();
                assert_eq!(
                    config.icon,
                    Some(
                        path.parent()
                            .unwrap()
                            .join("icon.png")
                            .to_string_lossy()
                            .into_owned()
                    )
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
    fn clap_definition_is_consistent() {
        use clap::CommandFactory;
        Args::command().debug_assert();
    }
}
