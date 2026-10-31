use std::path::Path;

use crate::form::ConfigDraft;

impl ConfigDraft {
    pub fn import(path: &Path) -> Result<Self, String> {
        let config = rake_core::load_config(path).map_err(|error| error.to_string())?;
        config.validate().map_err(|error| error.to_string())?;
        Ok(Self::from(config))
    }

    pub fn export(&self, path: &Path) -> Result<(), String> {
        rake_core::save_config(&self.config()?, path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Field;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "rake-config-io-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            Self(fs::canonicalize(root).unwrap())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn import_export_round_trip_resolves_paths_beside_import_not_export() {
        let fixture = Fixture::new();
        let import = fixture.0.join("source.json");
        fs::write(
            &import,
            r#"{
            "schema_version": 7,
            "url": "https://example.com",
            "name": "Example",
            "identifier": "org.example.app",
            "icon": "my icon.png",
            "width": 1440, "height": 900, "min_width": 0, "min_height": 480,
            "title": "Example App", "user_agent": "Rake/1",
            "hide_title_bar": true, "dark_mode": true, "debug": true,
            "inject": ["my style.css", "/absolute/script.js"],
            "safe_domains": ["login.example.com", "auth.example.com"]
        }"#,
        )
        .unwrap();
        let draft = ConfigDraft::import(&import).unwrap();
        let config = draft.config().unwrap();
        assert_eq!(config, rake_core::load_config(&import).unwrap());
        assert_eq!(
            config.icon.as_deref(),
            fixture.0.join("my icon.png").to_str()
        );
        assert_eq!(
            config.inject,
            [
                fixture.0.join("my style.css"),
                PathBuf::from("/absolute/script.js")
            ]
        );
        let exports = fixture.0.join("elsewhere");
        fs::create_dir(&exports).unwrap();
        let export = exports.join("saved.json");
        draft.export(&export).unwrap();
        assert_eq!(rake_core::load_config(&export).unwrap(), config);
        assert_eq!(
            ConfigDraft::import(&export).unwrap().config().unwrap(),
            config
        );
        assert!(!draft.install);
    }

    #[test]
    fn export_resolves_form_paths_from_working_directory_and_keeps_remote_icons() {
        let fixture = Fixture::new();
        let export = fixture.0.join("saved.json");
        let mut draft = ConfigDraft::default();
        draft.text[Field::Url as usize] = "https://example.com".into();
        draft.text[Field::Inject as usize] = "my style.css\nscript.js".into();
        draft.install = true;
        for icon in ["my icon.png", "HTTPS://example.com/icon.png"] {
            draft.text[Field::Icon as usize] = icon.into();
            draft.export(&export).unwrap();
            let config = rake_core::load_config(&export).unwrap();
            let cwd = std::env::current_dir().unwrap();
            let expected = if icon.starts_with("HTTPS://") {
                icon.to_owned()
            } else {
                cwd.join(icon).to_string_lossy().into_owned()
            };
            assert_eq!(config.icon.as_deref(), Some(expected.as_str()));
            assert_eq!(
                config.inject,
                [cwd.join("my style.css"), cwd.join("script.js")]
            );
            assert!(!fs::read_to_string(&export).unwrap().contains("install"));
        }
    }

    #[test]
    fn invalid_imports_and_exports_report_errors_without_overwriting() {
        let fixture = Fixture::new();
        let path = fixture.0.join("config.json");
        assert!(ConfigDraft::import(&path).unwrap_err().contains("config"));
        for contents in [
            "{",
            r#"{"url":"https://example.com","unknown":true}"#,
            r#"{"url":"invalid"}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert!(ConfigDraft::import(&path).is_err());
        }
        fs::write(&path, "keep this").unwrap();
        assert!(ConfigDraft::default().export(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "keep this");
        let mut draft = ConfigDraft::default();
        draft.text[Field::Url as usize] = "https://example.com".into();
        assert!(
            draft
                .export(&fixture.0.join("missing/config.json"))
                .is_err()
        );
    }
}
