use std::path::PathBuf;

use rake_config::RakeConfig;

use crate::{Error, Target, TemplateOptions, TemplateTarget};

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
    let host = TemplateTarget::current()?;
    let platform = host.platform();
    let target = options.template.target.unwrap_or(host);
    platform.validate(options.target, options.install)?;
    if target.platform() != platform {
        return Err(Error::InvalidInput(
            "template must match the host OS".into(),
        ));
    }
    progress(BuildStep::Template);
    let template = crate::ensure_template_for(&options.template, target)
        .map_err(|error| Error::MissingTemplate(error.to_string()))?;
    progress(BuildStep::Assembling);
    let app = crate::host::assemble_with_progress(
        config,
        &template,
        &options.output_dir,
        options.force,
        &mut progress,
    )?;
    *warnings = app.warnings;
    if options.target != Target::App {
        progress(BuildStep::Packaging);
    }
    let output_path = crate::host::package(
        config,
        &app.path,
        &options.output_dir,
        options.target,
        options.force,
        target,
    )?;
    if options.install {
        progress(BuildStep::Installing);
        crate::host::install_default(&app.path)?;
    }
    progress(BuildStep::Done);
    Ok(BuildResult {
        app_path: app.path,
        output_path,
    })
}
