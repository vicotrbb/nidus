use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::generate_name::to_pascal_case;

pub(crate) fn artifact(kind: &str, name: &str, module_name: &str, root: &Path) -> String {
    let type_name = to_pascal_case(module_name);
    match kind {
        "module" => module_artifact(root, module_name),
        "controller" => format!(
            r#"use nidus::prelude::*;

#[controller("/{name}")]
#[allow(dead_code)]
pub struct {type_name}Controller;

#[routes]
#[allow(dead_code)]
impl {type_name}Controller {{
    #[get("/")]
    pub async fn index(&self) {{}}
}}
"#
        ),
        "service" => format!(
            r#"use nidus::prelude::*;

#[injectable]
#[allow(dead_code)]
pub struct {type_name}Service;
"#
        ),
        "repository" => format!(
            r#"use nidus::prelude::*;

#[injectable]
#[allow(dead_code)]
pub struct {type_name}Repository;
"#
        ),
        _ => unreachable!("artifact kind should be validated before rendering"),
    }
}

pub(crate) fn sync_generated_feature_module(root: &Path, module_name: &str) -> Result<()> {
    let path = root
        .join("src")
        .join("modules")
        .join(format!("{module_name}.rs"));
    if !path.exists() {
        return Ok(());
    }

    let type_name = to_pascal_case(module_name);
    let contents =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if !is_generated_module_artifact(&contents, &type_name) {
        return Ok(());
    }

    write(&path, &module_artifact(root, module_name))
}

fn is_generated_module_artifact(contents: &str, type_name: &str) -> bool {
    contents.contains("use nidus::prelude::*;")
        && contents.contains("#[module")
        && contents.contains("#[allow(dead_code)]")
        && contents.contains(&format!("pub struct {type_name}Module;"))
        && !contents.contains('{')
}

fn module_artifact(root: &Path, module_name: &str) -> String {
    let type_name = to_pascal_case(module_name);
    let metadata = feature_module_metadata(root, module_name);
    let module_attr = metadata.render_module_attr();
    format!(
        r#"use nidus::prelude::*;

{module_attr}
#[allow(dead_code)]
pub struct {type_name}Module;
"#
    )
}

#[derive(Default)]
struct FeatureModuleMetadata {
    providers: Vec<String>,
    controllers: Vec<String>,
    exports: Vec<String>,
}

impl FeatureModuleMetadata {
    fn render_module_attr(&self) -> String {
        let mut sections = Vec::new();
        if !self.providers.is_empty() {
            sections.push(format!("providers({})", self.providers.join(", ")));
        }
        if !self.controllers.is_empty() {
            sections.push(format!("controllers({})", self.controllers.join(", ")));
        }
        if !self.exports.is_empty() {
            sections.push(format!("exports({})", self.exports.join(", ")));
        }

        if sections.is_empty() {
            "#[module]".to_owned()
        } else {
            format!("#[module(\n    {}\n)]", sections.join(",\n    "))
        }
    }
}

fn feature_module_metadata(root: &Path, module_name: &str) -> FeatureModuleMetadata {
    let mut metadata = FeatureModuleMetadata::default();

    if artifact_exists(root, "repositories", module_name) {
        metadata
            .providers
            .push(feature_type_path("repositories", module_name, "Repository"));
    }

    if artifact_exists(root, "services", module_name) {
        let service = feature_type_path("services", module_name, "Service");
        metadata.providers.push(service.clone());
        metadata.exports.push(service);
    }

    if artifact_exists(root, "controllers", module_name) {
        metadata
            .controllers
            .push(feature_type_path("controllers", module_name, "Controller"));
    }

    metadata
}

fn artifact_exists(root: &Path, directory: &str, module_name: &str) -> bool {
    root.join("src")
        .join(directory)
        .join(format!("{module_name}.rs"))
        .exists()
}

fn feature_type_path(directory: &str, module_name: &str, suffix: &str) -> String {
    format!(
        "crate::{directory}::{module_name}::{}{suffix}",
        to_pascal_case(module_name)
    )
}

fn write(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::FeatureModuleMetadata;

    #[test]
    fn module_metadata_renders_empty_module_attribute() {
        assert_eq!(
            FeatureModuleMetadata::default().render_module_attr(),
            "#[module]"
        );
    }

    #[test]
    fn module_metadata_renders_populated_sections() {
        let metadata = FeatureModuleMetadata {
            providers: vec!["UsersRepository".to_owned(), "UsersService".to_owned()],
            controllers: vec!["UsersController".to_owned()],
            exports: vec!["UsersService".to_owned()],
        };

        assert_eq!(
            metadata.render_module_attr(),
            "#[module(\n    providers(UsersRepository, UsersService),\n    controllers(UsersController),\n    exports(UsersService)\n)]"
        );
    }
}
