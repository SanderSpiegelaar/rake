use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

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
