use std::{
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use sysinfo::System;

pub fn wait_for_process() -> Result<PathBuf> {
    let mut system = System::new_all().context("failed to initialize process detection")?;

    loop {
        system.refresh_all();

        if let Some(executable) = system.processes().values().find_map(|process| {
            let executable = process.exe()?;

            executable
                .file_name()?
                .to_string_lossy()
                .eq_ignore_ascii_case("osu!.exe")
                .then(|| executable.to_path_buf())
        }) {
            return Ok(executable);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

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
