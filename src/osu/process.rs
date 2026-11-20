use anyhow::{Context, Result};
use std::{path::PathBuf, thread, time::Duration};
use sysinfo::System;

#[derive(Debug, Clone)]
pub struct OsuProcess {
    pub pid: u32,
    pub executable: PathBuf,
}

pub fn wait_for_process() -> Result<OsuProcess> {
    let mut system = System::new_all().context("failed to initialize process detection")?;

    loop {
        system.refresh_all();

        if let Some(process) = system.processes().values().find_map(|process| {
            let executable = process.exe()?;

            let name = executable.file_name()?.to_string_lossy();

            name.eq_ignore_ascii_case("osu!.exe").then(|| OsuProcess {
                pid: process.pid().as_u32(),
                executable: executable.to_path_buf(),
            })
        }) {
            return Ok(process);
        }

        thread::sleep(Duration::from_secs(1));
    }
}
