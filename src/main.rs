mod logging;
mod osu;

use anyhow::{Context, Result};
use osu::memory::scanner;
use std::{
    thread,
    time::{Duration, Instant},
};
use tracing::info;

fn read_selected_beatmap(memory: &osu::MemoryReader, pointer_address: u32) -> Option<(u32, u32)> {
    let pointer = memory.read_u32(pointer_address).ok()?;

    if pointer == 0 {
        return None;
    }

    let beatmap_address = memory.read_u32(pointer).ok()?;

    if beatmap_address == 0 {
        return None;
    }

    let fields_address = beatmap_address.checked_add(0xC8)?;

    let mut fields = [0u8; 8];
    memory.read_exact(fields_address, &mut fields).ok()?;

    if memory.read_u32(pointer_address).ok()? != pointer
        || memory.read_u32(pointer).ok()? != beatmap_address
    {
        return None;
    }

    let mut verified_fields = [0u8; 8];
    memory
        .read_exact(fields_address, &mut verified_fields)
        .ok()?;

    if fields != verified_fields {
        return None;
    }

    let beatmap_id = u32::from_le_bytes(fields[0..4].try_into().ok()?);
    let beatmapset_id = u32::from_le_bytes(fields[4..8].try_into().ok()?);

    if beatmap_id == 0 || beatmapset_id == 0 {
        return None;
    }

    Some((beatmap_id, beatmapset_id))
}

fn main() -> Result<()> {
    logging::init();

    info!("waiting for osu! stable...");

    let process = osu::wait_for_process()?;

    info!(
        pid = process.pid,
        executable = %process.executable.display(),
        "found osu! stable"
    );

    let database = osu::find_database(&process.executable)?;
    info!(path = %database.display(), "found osu! database");

    let memory = osu::MemoryReader::attach(process.pid)?;
    info!(pid = memory.pid(), "attached to osu! memory");

    let start = Instant::now();
    let regions = osu::memory::regions::enumerate(&memory)?;
    let readable = regions.iter().filter(|region| region.is_readable()).count();

    info!(
        elapsed = ?start.elapsed(),
        total = regions.len(),
        readable,
        "enumerated process memory"
    );

    let pattern = scanner::Pattern::parse("F8 01 74 04 83 65")?;

    info!("scanning osu! memory...");

    let start = Instant::now();
    let result = scanner::scan_first(&memory, &regions, &pattern)?
        .context("osu! beatmap base signature not found")?;

    let pointer_address = result
        .address
        .checked_sub(0x0C)
        .context("invalid beatmap pointer address")?;

    info!(
        elapsed = ?start.elapsed(),
        address = format_args!("{:#010X}", result.address),
        pointer_address = format_args!("{pointer_address:#010X}"),
        "found beatmap signature"
    );

    let mut previous = None;

    loop {
        if let Some(current) = read_selected_beatmap(&memory, pointer_address) {
            if previous != Some(current) {
                let (beatmap_id, beatmapset_id) = current;

                info!(beatmap_id, beatmapset_id, "selected beatmap");
                previous = Some(current);
            }
        }

        thread::sleep(Duration::from_millis(250));
    }
}
