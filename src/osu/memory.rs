pub mod regions;
pub mod scanner;

use anyhow::{Context, Result, bail};
use std::{ffi::c_void, io};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        Diagnostics::Debug::ReadProcessMemory,
        Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
    },
};

pub struct MemoryReader {
    handle: HANDLE,
    pid: u32,
}

impl MemoryReader {
    pub fn attach(pid: u32) -> Result<Self> {
        // open the process with read-only permissions
        let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error())
                .with_context(|| format!("failed to attach to process {pid}"));
        }

        Ok(Self { handle, pid })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn read_exact(&self, address: u32, buffer: &mut [u8]) -> Result<()> {
        let mut bytes_read = 0usize;

        // copy bytes from the target process into the local buffer
        let success = unsafe {
            ReadProcessMemory(
                self.handle,
                address as usize as *const c_void,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut bytes_read,
            )
        };

        if success == 0 {
            return Err(io::Error::last_os_error())
                .with_context(|| format!("failed to read memory at {address:#010X}"));
        }

        // reject incomplete reads instead of silently using corrupted data
        if bytes_read != buffer.len() {
            bail!(
                "partial memory read at {address:#010X}: {bytes_read}/{} bytes",
                buffer.len()
            );
        }

        Ok(())
    }

    pub fn read_u32(&self, address: u32) -> Result<u32> {
        let mut buffer = [0u8; 4];
        self.read_exact(address, &mut buffer)?;
        Ok(u32::from_le_bytes(buffer))
    }

    pub(super) fn handle(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for MemoryReader {
    fn drop(&mut self) {
        // release the Windows process handle when the reader is dropped
        if !self.handle.is_null() {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}
