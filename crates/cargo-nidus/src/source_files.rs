use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub(crate) fn rust_source_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut sources = BTreeSet::new();
    collect_rust_source_files(root, &mut sources)?;
    Ok(sources.into_iter().collect())
}

fn collect_rust_source_files(directory: &Path, sources: &mut BTreeSet<PathBuf>) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }

    for entry in
        fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?
    {
        let entry = entry.with_context(|| format!("reading entry in {}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("reading file type for {}", path.display()))?;
        if file_type.is_dir() {
            collect_rust_source_files(&path, sources)?;
        } else if (file_type.is_file() || (file_type.is_symlink() && path.is_file()))
            && path.extension().and_then(|extension| extension.to_str()) == Some("rs")
        {
            sources.insert(path);
        }
    }
    Ok(())
}
