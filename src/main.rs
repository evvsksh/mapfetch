mod logging;
mod osu;

use anyhow::Result;
use tracing::info;

fn main() -> Result<()> {
    logging::init();

    info!("waiting for osu!");

    let executable = osu::wait_for_process()?;
    let database = osu::find_database(&executable)?;

    info!("osu! executable: {}", executable.display());
    info!("osu! database: {}", database.display());

    Ok(())
}
