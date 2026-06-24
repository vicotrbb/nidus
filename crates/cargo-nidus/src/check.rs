use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};

pub(crate) fn check_project(root: &Path) -> Result<()> {
    if !root.join("Cargo.toml").exists() {
        bail!(
            "Nidus project check failed for {}. Missing required files: {}",
            root.display(),
            "Cargo.toml"
        );
    }

    if !root.join("src/main.rs").exists() && !root.join("src/lib.rs").exists() {
        bail!(
            "Nidus project check failed for {}. Missing required crate root: src/main.rs or src/lib.rs",
            root.display()
        );
    }

    validate_module_indexes(root)?;
    validate_crate_root_modules(root)?;

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

fn validate_crate_root_modules(root: &Path) -> Result<()> {
    let crate_roots = crate_root_contents(root)?;
    for module in ["modules", "controllers", "services", "repositories"] {
        let directory = root.join("src").join(module);
        if !directory.exists() || source_module_names(&directory)?.is_empty() {
            continue;
        }

        if !crate_roots
            .iter()
            .any(|contents| contains_module_declaration(contents, module))
        {
            bail!(
                "missing crate root module declaration for {}: add mod {}; to src/main.rs or pub mod {}; to src/lib.rs",
                directory.display(),
                module,
                module
            );
        }
    }
    Ok(())
}

fn crate_root_contents(root: &Path) -> Result<Vec<String>> {
    let mut roots = Vec::new();
    for path in [root.join("src/main.rs"), root.join("src/lib.rs")] {
        if path.exists() {
            roots.push(
                fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?,
            );
        }
    }
    Ok(roots)
}

fn contains_module_declaration(contents: &str, module: &str) -> bool {
    let private = format!("mod {module};");
    let public = format!("pub mod {module};");
    let crate_public = format!("pub(crate) mod {module};");
    contents
        .lines()
        .map(str::trim)
        .any(|line| line == private || line == public || line == crate_public)
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
