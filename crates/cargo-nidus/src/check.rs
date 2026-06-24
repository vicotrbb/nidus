use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};

pub(crate) fn check_project(root: &Path) -> Result<()> {
    let required = ["Cargo.toml", "src/main.rs"];
    let missing = required
        .iter()
        .filter(|path| !root.join(path).exists())
        .copied()
        .collect::<Vec<_>>();

    if !missing.is_empty() {
        bail!(
            "Nidus project check failed for {}. Missing required files: {}",
            root.display(),
            missing.join(", ")
        );
    }

    validate_module_indexes(root)?;

    println!("Nidus project check passed for {}", root.display());
    Ok(())
}

fn validate_module_indexes(root: &Path) -> Result<()> {
    for directory in ["modules", "controllers", "services", "repositories"] {
        let directory = root.join("src").join(directory);
        if !directory.exists() {
            continue;
        }

        let source_modules = source_module_names(&directory)?;
        let mod_rs = directory.join("mod.rs");
        if !mod_rs.exists() {
            if let Some(module) = source_modules.first() {
                bail!(
                    "missing module index file {}: add pub mod {}; for {}",
                    mod_rs.display(),
                    module,
                    directory.join(format!("{module}.rs")).display()
                );
            }
            continue;
        }
        let contents =
            fs::read_to_string(&mod_rs).with_context(|| format!("reading {}", mod_rs.display()))?;
        let indexed_modules = contents
            .lines()
            .filter_map(extract_module_index_entry)
            .collect::<BTreeSet<_>>();

        for module in &indexed_modules {
            let expected = mod_rs
                .parent()
                .expect("mod.rs has a parent directory")
                .join(format!("{module}.rs"));
            if !expected.exists() {
                bail!(
                    "stale module index entry in {}: missing {}",
                    mod_rs.display(),
                    expected.display()
                );
            }
        }

        for module in source_modules {
            if !indexed_modules.contains(&module) {
                bail!(
                    "missing module index entry in {}: add pub mod {};",
                    mod_rs.display(),
                    module
                );
            }
        }
    }
    Ok(())
}

fn source_module_names(directory: &Path) -> Result<Vec<String>> {
    let mut modules = Vec::new();
    for entry in
        fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
    {
        let path = entry?.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
            && let Some(module) = path.file_stem().and_then(|stem| stem.to_str())
        {
            modules.push(module.to_owned());
        }
    }
    modules.sort();
    Ok(modules)
}

fn extract_module_index_entry(line: &str) -> Option<String> {
    let line = line.trim();
    let module = line.strip_prefix("pub mod ")?;
    let module = module.strip_suffix(';')?;
    let module = module.trim();
    if module.is_empty() {
        None
    } else {
        Some(module.to_owned())
    }
}
