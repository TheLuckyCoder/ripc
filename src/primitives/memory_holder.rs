use std::ffi::CString;
use std::os::fd::OwnedFd;
use std::ptr::slice_from_raw_parts_mut;

use rustix::fs::Mode;
use rustix::mm::{MapFlags, ProtFlags};
use rustix::shm::ShmOFlags;

pub struct SharedMemoryHolder {
    name: CString,
    _fd: OwnedFd,
    memory: *mut [u8],
    created: bool,
}

impl SharedMemoryHolder {
    pub fn create(name: CString, size: usize) -> std::io::Result<Self> {
        // Open shared memory
        let shm = rustix::shm::shm_open(
            name.as_c_str(),
            ShmOFlags::CREATE | ShmOFlags::RDWR | ShmOFlags::TRUNC,
            Mode::all(),
        )?;

        // Resize shared memory
        if let Err(e) = rustix::fs::ftruncate(&shm, size as u64) {
            let _ = rustix::shm::shm_unlink(name.as_c_str());
            return Err(e.into());
        }

        match Self::map_memory(&shm) {
            Ok(slice_ptr) => {
                unsafe {
                    (*slice_ptr).fill(0);
                }

                Ok(Self {
                    name,
                    _fd: shm,
                    memory: slice_ptr,
                    created: true,
                })
            }
            Err(e) => {
                let _ = rustix::shm::shm_unlink(name);
                Err(e.into())
            }
        }
    }

    pub fn open(name: CString) -> std::io::Result<Self> {
        // Open shared memory
        let shm = rustix::shm::shm_open(&name, ShmOFlags::RDWR, Mode::all())?;

        Ok(Self {
            name,
            memory: Self::map_memory(&shm)?,
            _fd: shm,
            created: false,
        })
    }

    pub fn slice_ptr(&self) -> *mut [u8] {
        self.memory
    }

    fn map_memory(shm: &OwnedFd) -> rustix::io::Result<*mut [u8]> {
        // Read actual size
        let stats = rustix::fs::fstat(shm)?;
        let size = stats.st_size as usize;

        // Map shared memory
        let ptr = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                size,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED_VALIDATE | MapFlags::POPULATE,
                shm,
                0,
            )?
        };

        Ok(slice_from_raw_parts_mut(ptr.cast(), size))
    }
}

unsafe impl Send for SharedMemoryHolder {}
unsafe impl Sync for SharedMemoryHolder {}

impl Drop for SharedMemoryHolder {
    fn drop(&mut self) {
        if let Err(e) = unsafe { rustix::mm::munmap(self.memory.cast(), self.memory.len()) } {
            eprintln!("Failed to unmap shared memory: {}", e);
        }

        if self.created {
            if let Err(e) = rustix::shm::shm_unlink(&self.name) {
                eprintln!("Failed to unlink shared memory: {}", e);
            }
        }
    }
}
