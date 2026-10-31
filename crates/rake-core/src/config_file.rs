use std::{fs, path::Path};

use rake_config::RakeConfig;

use crate::Error;

// Validation follows CLI overrides, which may repair invalid file values.
pub fn load_config(path: &Path) -> Result<RakeConfig, Error> {
    let read = || {
        let path = std::path::absolute(path)?;
        let bytes = fs::read(&path)?;
        let mut config: RakeConfig = serde_json::from_slice(&bytes)?;
        resolve_paths(&mut config, path.parent().unwrap());
        Ok::<_, Box<dyn std::error::Error>>(config)
    };
    read().map_err(|error| Error::InvalidInput(format!("config {}: {error}", path.display())))
}

pub fn save_config(config: &RakeConfig, path: &Path) -> Result<(), Error> {
    config
        .validate()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    let mut config = config.clone();
    resolve_paths(&mut config, &std::env::current_dir()?);
    let bytes = serde_json::to_vec_pretty(&config)
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    fs::write(path, bytes)?;
    Ok(())
}

fn resolve_paths(config: &mut RakeConfig, directory: &Path) {
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
}
