use anyhow::{Context, Result, bail};
use std::{ffi::c_void, io, mem::size_of};
use windows_sys::Win32::{
    Foundation::{ERROR_INVALID_PARAMETER, GetLastError},
    System::Memory::{
        MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
        PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE,
        PAGE_WRITECOPY, VirtualQueryEx,
    },
};

use super::MemoryReader;

#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub base_address: usize,
    pub size: usize,
    pub state: u32,
    pub protection: u32,
}

impl MemoryRegion {
    pub fn is_readable(&self) -> bool {
        if self.state != MEM_COMMIT || self.protection & (PAGE_GUARD | PAGE_NOACCESS) != 0 {
            return false;
        }

        // Ignore protection modifiers and check the base protection.
        matches!(
            self.protection & 0xFF,
            PAGE_READONLY
                | PAGE_READWRITE
                | PAGE_WRITECOPY
                | PAGE_EXECUTE_READ
                | PAGE_EXECUTE_READWRITE
                | PAGE_EXECUTE_WRITECOPY
        )
    }
}

pub fn enumerate(reader: &MemoryReader) -> Result<Vec<MemoryRegion>> {
    let mut regions = Vec::new();
    let mut address = 0usize;

    loop {
        let mut info = MEMORY_BASIC_INFORMATION::default();

        // Query the memory region containing the current address.
        let result = unsafe {
            VirtualQueryEx(
                reader.handle(),
                address as *const c_void,
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };

        if result == 0 {
            let error = unsafe { GetLastError() };

            // Reaching the end of the address space is expected.
            if error == ERROR_INVALID_PARAMETER {
                break;
            }

            return Err(io::Error::from_raw_os_error(error as i32))
                .context(format!("failed to query memory at {address:#X}"));
        }

        if info.RegionSize == 0 {
            bail!("invalid memory region size at {address:#X}");
        }

        regions.push(MemoryRegion {
            base_address: info.BaseAddress as usize,
            size: info.RegionSize,
            state: info.State,
            protection: info.Protect,
        });

        // Move to the address immediately after this region.
        address = (info.BaseAddress as usize)
            .checked_add(info.RegionSize)
            .context("memory address overflow")?;
    }

    Ok(regions)
}
