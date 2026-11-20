use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

pub fn find_database(executable: &Path) -> Result<PathBuf> {
    let directory = executable
        .parent()
        .context("could not determine osu! installation directory")?;

    let database = directory.join("osu!.db");

    if !database.is_file() {
        bail!("osu!.db not found at {}", database.display());
    }

    Ok(database)
}
