use rake_config::RakeConfig;

#[derive(Clone, Copy, Debug)]
pub enum Field {
    Url,
    Name,
    Identifier,
    Icon,
    Width,
    Height,
    MinWidth,
    MinHeight,
    Title,
    UserAgent,
    Inject,
    SafeDomains,
}

impl Field {
    pub const ALL: [Self; 12] = [
        Self::Url,
        Self::Name,
        Self::Identifier,
        Self::Icon,
        Self::Width,
        Self::Height,
        Self::MinWidth,
        Self::MinHeight,
        Self::Title,
        Self::UserAgent,
        Self::Inject,
        Self::SafeDomains,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Url => "URL",
            Self::Name => "Name",
            Self::Identifier => "Identifier",
            Self::Icon => "Icon",
            Self::Width => "Width",
            Self::Height => "Height",
            Self::MinWidth => "Minimum width",
            Self::MinHeight => "Minimum height",
            Self::Title => "Title",
            Self::UserAgent => "User agent",
            Self::Inject => "Inject files (one per line)",
            Self::SafeDomains => "Safe domains (one per line)",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Url => "url",
            Self::Name => "name",
            Self::Identifier => "identifier",
            Self::Icon => "icon",
            Self::Width => "width",
            Self::Height => "height",
            Self::MinWidth => "min_width",
            Self::MinHeight => "min_height",
            Self::Title => "title",
            Self::UserAgent => "user_agent",
            Self::Inject => "inject",
            Self::SafeDomains => "safe_domains",
        }
    }

    pub fn is_list(self) -> bool {
        matches!(self, Self::Inject | Self::SafeDomains)
    }
}

pub struct ConfigDraft {
    pub text: [String; Field::ALL.len()],
    pub hide_title_bar: bool,
    pub dark_mode: bool,
    pub debug: bool,
    pub install: bool,
}

impl Default for ConfigDraft {
    fn default() -> Self {
        let config = RakeConfig::new("");
        Self {
            text: [
                config.url,
                config.name.unwrap_or_default(),
                config.identifier.unwrap_or_default(),
                config.icon.unwrap_or_default(),
                config.width.to_string(),
                config.height.to_string(),
                config.min_width.map(|n| n.to_string()).unwrap_or_default(),
                config.min_height.map(|n| n.to_string()).unwrap_or_default(),
                config.title.unwrap_or_default(),
                config.user_agent.unwrap_or_default(),
                config
                    .inject
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n"),
                config.safe_domains.join("\n"),
            ],
            hide_title_bar: config.hide_title_bar,
            dark_mode: config.dark_mode,
            debug: config.debug,
            install: false,
        }
    }
}

impl ConfigDraft {
    pub fn config(&self) -> Result<RakeConfig, String> {
        let text = |field: Field| self.text[field as usize].as_str();
        let optional = |field| {
            let value = text(field);
            (!value.is_empty()).then(|| value.to_owned())
        };
        let number = |field: Field| {
            text(field).parse::<u32>().map_err(|_| {
                format!(
                    "{}: must be a nonnegative integer, got `{}`",
                    field.key(),
                    text(field)
                )
            })
        };
        let optional_number = |field| {
            if text(field).is_empty() {
                Ok(None)
            } else {
                number(field).map(Some)
            }
        };
        let mut config = RakeConfig::new(text(Field::Url));
        config.name = optional(Field::Name);
        config.identifier = optional(Field::Identifier);
        config.icon = optional(Field::Icon);
        config.width = number(Field::Width)?;
        config.height = number(Field::Height)?;
        config.min_width = optional_number(Field::MinWidth)?;
        config.min_height = optional_number(Field::MinHeight)?;
        config.title = optional(Field::Title);
        config.user_agent = optional(Field::UserAgent);
        config.hide_title_bar = self.hide_title_bar;
        config.dark_mode = self.dark_mode;
        config.debug = self.debug;
        config.inject = text(Field::Inject)
            .lines()
            .filter(|line| !line.is_empty())
            .map(Into::into)
            .collect();
        config.safe_domains = text(Field::SafeDomains)
            .lines()
            .filter(|line| !line.is_empty())
            .map(Into::into)
            .collect();
        config.validate().map_err(|error| error.to_string())?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> ConfigDraft {
        let mut draft = ConfigDraft::default();
        draft.text[Field::Url as usize] = "https://example.com".into();
        draft
    }

    #[test]
    fn defaults_come_from_config() {
        assert_eq!(
            draft().config().unwrap(),
            RakeConfig::new("https://example.com")
        );
        assert!(!draft().install);
        assert_eq!(
            ConfigDraft::default().config().unwrap_err(),
            RakeConfig::new("").validate().unwrap_err().to_string()
        );
    }

    #[test]
    fn every_app_option_is_parsed() {
        let draft = ConfigDraft {
            text: [
                "https://example.com",
                "Example",
                "org.example.app",
                "/my icons/icon.png",
                "1440",
                "900",
                "640",
                "480",
                "Example App",
                "Rake/1",
                "/my scripts/style.css\nscript.js\n",
                "login.example.com\nlocalhost\n",
            ]
            .map(String::from),
            hide_title_bar: true,
            dark_mode: true,
            debug: true,
            install: true,
        };
        let mut expected = RakeConfig::new("https://example.com");
        expected.name = Some("Example".into());
        expected.identifier = Some("org.example.app".into());
        expected.icon = Some("/my icons/icon.png".into());
        expected.width = 1440;
        expected.height = 900;
        expected.min_width = Some(640);
        expected.min_height = Some(480);
        expected.title = Some("Example App".into());
        expected.user_agent = Some("Rake/1".into());
        expected.hide_title_bar = true;
        expected.dark_mode = true;
        expected.debug = true;
        expected.inject = vec!["/my scripts/style.css".into(), "script.js".into()];
        expected.safe_domains = vec!["login.example.com".into(), "localhost".into()];
        assert_eq!(draft.config().unwrap(), expected);
        assert!(draft.install);
    }

    #[test]
    fn blank_optional_fields_clear_and_lists_preserve_order_and_spaces() {
        let mut draft = draft();
        draft.text[Field::MinWidth as usize] = "0".into();
        draft.text[Field::Inject as usize] = "first file.css\n\nsecond.js".into();
        assert_eq!(draft.config().unwrap().min_width, Some(0));
        assert_eq!(
            draft.config().unwrap().inject,
            vec![
                std::path::PathBuf::from("first file.css"),
                "second.js".into()
            ]
        );
        draft.text[Field::MinWidth as usize].clear();
        draft.text[Field::Inject as usize].clear();
        assert_eq!(
            draft.config().unwrap(),
            RakeConfig::new("https://example.com")
        );
        draft.text[Field::SafeDomains as usize] = " example.com".into();
        assert_eq!(
            draft.config().unwrap_err(),
            "safe_domains: must contain bare hostnames, got ` example.com`"
        );
    }

    #[test]
    fn malformed_numbers_never_reuse_the_last_valid_value() {
        for field in [
            Field::Width,
            Field::Height,
            Field::MinWidth,
            Field::MinHeight,
        ] {
            for value in ["abc", "-1", "1.5", "4294967296"] {
                let mut draft = draft();
                draft.text[field as usize] = value.into();
                assert_eq!(
                    draft.config().unwrap_err(),
                    format!(
                        "{}: must be a nonnegative integer, got `{value}`",
                        field.key()
                    )
                );
            }
        }
    }

    #[test]
    fn validation_messages_are_config_errors_verbatim() {
        for (field, value) in [
            (Field::Url, "invalid"),
            (Field::Width, "0"),
            (Field::Height, "0"),
            (Field::MinWidth, "1201"),
            (Field::MinHeight, "781"),
            (Field::Identifier, "invalid"),
            (Field::SafeDomains, "https://example.com"),
        ] {
            let mut draft = draft();
            let mut config = draft.config().unwrap();
            draft.text[field as usize] = value.into();
            match field {
                Field::Url => config.url = value.into(),
                Field::Width => config.width = 0,
                Field::Height => config.height = 0,
                Field::MinWidth => config.min_width = Some(1201),
                Field::MinHeight => config.min_height = Some(781),
                Field::Identifier => config.identifier = Some(value.into()),
                Field::SafeDomains => config.safe_domains = vec![value.into()],
                _ => unreachable!(),
            }
            assert_eq!(
                draft.config().unwrap_err(),
                config.validate().unwrap_err().to_string()
            );
        }
    }

    #[test]
    #[ignore = "requires a built CLI binary at RAKE_CLI_BIN"]
    fn validation_messages_match_the_real_cli() {
        let binary = std::env::var_os("RAKE_CLI_BIN").expect("Set RAKE_CLI_BIN to the rake binary");
        for (field, value, flag) in [
            (Field::Url, "invalid", None),
            (Field::Width, "0", Some("--width")),
            (Field::Height, "0", Some("--height")),
            (Field::MinWidth, "1201", Some("--min-width")),
            (Field::MinHeight, "781", Some("--min-height")),
            (Field::Identifier, "invalid", Some("--identifier")),
            (
                Field::SafeDomains,
                "https://example.com",
                Some("--safe-domain"),
            ),
        ] {
            let mut draft = draft();
            draft.text[field as usize] = value.into();
            let mut command = std::process::Command::new(&binary);
            if let Some(flag) = flag {
                command.args(["https://example.com", flag, value]);
            } else {
                command.arg(value);
            }
            let output = command.output().unwrap();
            assert_eq!(output.status.code(), Some(2));
            assert_eq!(
                String::from_utf8(output.stderr).unwrap(),
                format!("error: {}\n", draft.config().unwrap_err())
            );
            assert!(output.stdout.is_empty());
        }
    }
}
