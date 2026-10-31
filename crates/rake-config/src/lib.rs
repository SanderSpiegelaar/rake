use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("url: must be an http or https URL, got `{url}`")]
    InvalidUrl { url: String },
    #[error("{field}: must be at least {minimum}, got {value}")]
    BelowMinimum {
        field: &'static str,
        value: u32,
        minimum: u32,
    },
    #[error("safe_domains: must contain bare hostnames, got `{domain}`")]
    InvalidSafeDomain { domain: String },
    #[error("identifier: must be a reverse-DNS identifier, got `{identifier}`")]
    InvalidIdentifier { identifier: String },
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_width() -> u32 {
    1200
}

fn default_height() -> u32 {
    780
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RakeConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub hide_title_bar: bool,
    #[serde(default)]
    pub dark_mode: bool,
    #[serde(default)]
    pub debug: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inject: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safe_domains: Vec<String>,
}

impl RakeConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !url::Url::parse(&self.url)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.has_host())
        {
            return Err(ConfigError::InvalidUrl {
                url: self.url.clone(),
            });
        }

        for (field, value, minimum) in [
            ("width", self.width, self.min_width),
            ("height", self.height, self.min_height),
        ] {
            if let Some(minimum) = minimum
                && value < minimum
            {
                return Err(ConfigError::BelowMinimum {
                    field,
                    value,
                    minimum,
                });
            }
        }

        for domain in &self.safe_domains {
            if domain
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, ':' | '/' | '\\' | '*'))
                || !matches!(url::Host::parse(domain), Ok(url::Host::Domain(_)))
            {
                return Err(ConfigError::InvalidSafeDomain {
                    domain: domain.clone(),
                });
            }
        }

        if let Some(identifier) = &self.identifier
            && (!identifier.contains('.')
                || identifier.split('.').any(|segment| {
                    segment.is_empty()
                        || !segment
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '-')
                }))
        {
            return Err(ConfigError::InvalidIdentifier {
                identifier: identifier.clone(),
            });
        }

        Ok(())
    }

    pub fn identifier(&self) -> String {
        if let Some(identifier) = &self.identifier {
            return identifier.clone();
        }

        let parsed_url = url::Url::parse(&self.url).ok();
        let name = self
            .name
            .as_deref()
            .or_else(|| parsed_url.as_ref().and_then(url::Url::host_str))
            .unwrap_or("app");
        let mut slug = String::new();
        for c in name.to_lowercase().chars() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                slug.push(c);
            } else if !slug.is_empty() && !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug = slug.trim_end_matches('-');
        let slug = if slug.is_empty() { "app" } else { slug };

        let mut hash = 0xcbf29ce484222325_u64;
        for byte in self.url.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let hash = format!("{hash:016x}");
        format!("com.rake.{slug}-{}", &hash[..8])
    }

    pub fn new(url: impl Into<String>) -> Self {
        Self {
            schema_version: default_schema_version(),
            url: url.into(),
            name: None,
            identifier: None,
            icon: None,
            width: default_width(),
            height: default_height(),
            min_width: None,
            min_height: None,
            title: None,
            user_agent: None,
            hide_title_bar: bool::default(),
            dark_mode: bool::default(),
            debug: bool::default(),
            inject: Vec::default(),
            safe_domains: Vec::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_urls() {
        for url in [
            "ftp://example.com",
            "not a URL",
            "https://",
            "mailto:hello@example.com",
        ] {
            assert_eq!(
                RakeConfig::new(url).validate(),
                Err(ConfigError::InvalidUrl { url: url.into() })
            );
        }
    }

    #[test]
    fn checks_dimensions_against_minimums() {
        let mut config = RakeConfig::new("https://example.com");
        config.min_width = Some(1201);
        config.min_height = Some(781);
        assert_eq!(
            config.validate(),
            Err(ConfigError::BelowMinimum {
                field: "width",
                value: 1200,
                minimum: 1201,
            })
        );
        config.min_width = Some(1200);
        assert_eq!(
            config.validate(),
            Err(ConfigError::BelowMinimum {
                field: "height",
                value: 780,
                minimum: 781,
            })
        );
        config.min_height = Some(780);
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn rejects_non_bare_safe_domains() {
        for domain in [
            "https://example.com",
            "example.com/path",
            "example.com:443",
            "*.example.com",
            "127.0.0.1",
            "[::1]",
            "::1",
            " example.com",
            "example .com",
            "",
            "example.com?query",
            "example.com#fragment",
            "user@example.com",
            "example.com\\path",
        ] {
            let mut config = RakeConfig::new("https://example.com");
            config.safe_domains = vec![domain.into()];
            assert_eq!(
                config.validate(),
                Err(ConfigError::InvalidSafeDomain {
                    domain: domain.into(),
                }),
                "{domain:?}"
            );
        }
    }

    #[test]
    fn accepts_valid_config() {
        let mut config = RakeConfig::new("https://example.com");
        config.safe_domains = ["example.com", "sub.example.co.uk", "localhost"]
            .map(String::from)
            .into();
        config.identifier = Some("com.Example-123.app".into());
        assert_eq!(config.validate(), Ok(()));
        assert_eq!(RakeConfig::new("http://localhost").validate(), Ok(()));
    }

    #[test]
    fn rejects_invalid_identifiers() {
        for identifier in [
            "app",
            "com..app",
            ".app",
            "com.",
            "com.my_app",
            "com.ápp",
            "",
        ] {
            let mut config = RakeConfig::new("https://example.com");
            config.identifier = Some(identifier.into());
            assert_eq!(
                config.validate(),
                Err(ConfigError::InvalidIdentifier {
                    identifier: identifier.into(),
                })
            );
        }
    }

    #[test]
    fn errors_name_their_fields() {
        for (error, field) in [
            (ConfigError::InvalidUrl { url: "bad".into() }, "url"),
            (
                ConfigError::BelowMinimum {
                    field: "width",
                    value: 1,
                    minimum: 2,
                },
                "width",
            ),
            (
                ConfigError::BelowMinimum {
                    field: "height",
                    value: 1,
                    minimum: 2,
                },
                "height",
            ),
            (
                ConfigError::InvalidSafeDomain {
                    domain: "bad/path".into(),
                },
                "safe_domains",
            ),
            (
                ConfigError::InvalidIdentifier {
                    identifier: "bad".into(),
                },
                "identifier",
            ),
        ] {
            assert!(error.to_string().starts_with(&format!("{field}:")));
        }
    }

    #[test]
    fn default_identifier_is_stable_and_url_specific() {
        let mut config = RakeConfig::new("https://example.com");
        config.name = Some("  My !!! App  ".into());
        assert_eq!(config.identifier(), "com.rake.my-app-837b2b57");
        assert_eq!(config.identifier(), config.identifier());
        let mut other = config.clone();
        other.url = "https://example.org".into();
        assert_ne!(config.identifier(), other.identifier());
        config.identifier = Some(config.identifier());
        assert_eq!(config.validate(), Ok(()));
    }

    #[test]
    fn identifier_uses_name_then_host_then_app() {
        for (name, expected_slug) in [
            (None, "example-com"),
            (Some(""), "app"),
            (Some("!!!"), "app"),
            (Some("Hello_123 WORLD"), "hello-123-world"),
        ] {
            let mut config = RakeConfig::new("https://example.com");
            config.name = name.map(String::from);
            assert_eq!(
                config.identifier(),
                format!("com.rake.{expected_slug}-837b2b57")
            );
            config.identifier = Some(config.identifier());
            assert_eq!(config.validate(), Ok(()));
        }
        assert!(
            RakeConfig::new("invalid")
                .identifier()
                .starts_with("com.rake.app-")
        );
    }

    #[test]
    fn configured_identifier_is_preserved() {
        let mut config = RakeConfig::new("https://example.com");
        config.identifier = Some("org.custom.App".into());
        assert_eq!(config.identifier(), "org.custom.App");
    }

    #[test]
    fn url_only_uses_defaults() {
        let config: RakeConfig = serde_json::from_str(r#"{"url":"https://example.com"}"#).unwrap();

        assert_eq!(config, RakeConfig::new("https://example.com"));
        assert_eq!(config.schema_version, 1);
        assert_eq!(config.width, 1200);
        assert_eq!(config.height, 780);

        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::json!({
                "schema_version": 1,
                "url": "https://example.com",
                "width": 1200,
                "height": 780,
                "hide_title_bar": false,
                "dark_mode": false,
                "debug": false
            })
        );
    }

    #[test]
    fn unknown_field_is_named_in_error() {
        let error = serde_json::from_str::<RakeConfig>(r#"{"url":"x","widht":5}"#).unwrap_err();

        assert!(error.to_string().contains("widht"));
    }

    #[test]
    fn fully_populated_config_round_trips() {
        let config = RakeConfig {
            schema_version: SCHEMA_VERSION,
            url: "https://example.com".into(),
            name: Some("Example".into()),
            identifier: Some("com.example.app".into()),
            icon: Some("https://example.com/icon.png".into()),
            width: 1440,
            height: 900,
            min_width: Some(640),
            min_height: Some(480),
            title: Some("Example App".into()),
            user_agent: Some("Rake/1.0".into()),
            hide_title_bar: true,
            dark_mode: true,
            debug: true,
            inject: vec![PathBuf::from("styles.css"), PathBuf::from("script.js")],
            safe_domains: vec!["example.com".into(), "login.example.com".into()],
        };

        let json = serde_json::to_string(&config).unwrap();
        let restored: RakeConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, config);
    }
}
