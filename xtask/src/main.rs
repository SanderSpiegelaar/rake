use std::{error::Error, fs, path::Path};

use rake_config::RakeConfig;

fn main() -> Result<(), Box<dyn Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("schema") => {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../schema");
            fs::create_dir_all(&directory)?;
            fs::write(directory.join("rake.schema.json"), schema_json())?;
            Ok(())
        }
        _ => {
            eprintln!("Usage: cargo xtask schema");
            std::process::exit(2);
        }
    }
}

fn schema_json() -> String {
    let schema = schemars::schema_for!(RakeConfig);
    format!("{}\n", serde_json::to_string_pretty(&schema).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn committed_schema_is_current() {
        assert_eq!(
            include_str!("../../schema/rake.schema.json"),
            schema_json(),
            "Run cargo xtask schema to update the committed schema"
        );
    }

    #[test]
    fn schema_reflects_serde_contract() {
        let schema = serde_json::to_value(schemars::schema_for!(RakeConfig)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["required"], json!(["url"]));
        assert_eq!(schema["properties"]["url"]["type"], "string");
        for (field, default) in [
            ("schema_version", json!(1)),
            ("width", json!(1200)),
            ("height", json!(780)),
            ("hide_title_bar", json!(false)),
            ("dark_mode", json!(false)),
            ("debug", json!(false)),
        ] {
            assert_eq!(schema["properties"][field]["default"], default, "{field}");
        }
    }
}
