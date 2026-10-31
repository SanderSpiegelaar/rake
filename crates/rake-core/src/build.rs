use std::path::PathBuf;

use rake_config::RakeConfig;

use crate::{Error, Target, TemplateOptions};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStep {
    Template,
    Assembling,
    Signing,
    Packaging,
    Installing,
    Done,
}

#[derive(Debug)]
pub struct BuildOptions {
    pub template: TemplateOptions,
    pub output_dir: PathBuf,
    pub target: Target,
    pub install: bool,
    pub force: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            template: TemplateOptions::default(),
            output_dir: PathBuf::from("."),
            target: Target::App,
            install: false,
            force: false,
        }
    }
}

#[derive(Debug)]
pub struct BuildResult {
    pub app_path: PathBuf,
    pub output_path: PathBuf,
}

pub fn build(
    config: &RakeConfig,
    options: &BuildOptions,
    warnings: &mut Vec<String>,
    mut progress: impl FnMut(BuildStep),
) -> Result<BuildResult, Error> {
    warnings.clear();
    config
        .validate()
        .map_err(|error| Error::InvalidInput(error.to_string()))?;
    progress(BuildStep::Template);
    let template = crate::ensure_template(&options.template)
        .map_err(|error| Error::MissingTemplate(error.to_string()))?;
    progress(BuildStep::Assembling);
    let app = crate::assemble_with_progress(
        config,
        &template,
        &options.output_dir,
        options.force,
        &mut progress,
    )?;
    *warnings = app.warnings;
    let output_path = match options.target {
        Target::App => app.path.clone(),
        Target::Dmg => {
            progress(BuildStep::Packaging);
            crate::package_dmg(&app.path, &options.output_dir, options.force)?
        }
    };
    if options.install {
        progress(BuildStep::Installing);
        crate::install(&app.path, std::path::Path::new("/Applications"))?;
    }
    progress(BuildStep::Done);
    Ok(BuildResult {
        app_path: app.path,
        output_path,
    })
}
