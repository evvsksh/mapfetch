mod logging;
mod osu;

use anyhow::Result;
use tracing::info;

fn main() -> Result<()> {
    logging::init();

    info!("waiting for osu! stable...");

    let process = osu::wait_for_process()?;

    info!(pid = process.pid,executable = %process.executable.display(),"found osu! stable");

    let database = osu::find_database(&process.executable)?;
    info!(path = %database.display(), "found osu! database");

    let memory = osu::MemoryReader::attach(process.pid)?;
    info!(pid = memory.pid(), "attached to osu! memory");

    let regions = osu::memory::regions::enumerate(&memory)?;
    let readable = regions.iter().filter(|region| region.is_readable()).count();

    info!(total = regions.len(), readable, "enumerated process memory");

    Ok(())
}
