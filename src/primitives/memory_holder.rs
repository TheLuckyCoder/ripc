use std::ffi::{c_void, CString};
use std::os::fd::OwnedFd;
use std::ptr::slice_from_raw_parts_mut;

use rustix::fs::Mode;
use rustix::mm::{MapFlags, ProtFlags};
use rustix::shm::ShmOFlags;

pub struct SharedMemoryHolder {
    name: CString,
    _fd: OwnedFd,
    shared_memory_ptr: *mut c_void,
    size: usize,
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

        // Map shared memory
        let mmap_result = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                size,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &shm,
                0,
            )
        };

        match mmap_result {
            Ok(ptr) => Ok(Self {
                name,
                _fd: shm,
                shared_memory_ptr: ptr,
                size,
                created: true,
            }),
            Err(e) => {
                let _ = rustix::shm::shm_unlink(name);
                Err(e.into())
            }
        }
    }

    pub fn open(name: CString) -> std::io::Result<Self> {
        // Open shared memory
        let shm = rustix::shm::shm_open(&name, ShmOFlags::RDWR, Mode::all())?;

        // Read size
        let stats = rustix::fs::fstat(&shm)?;
        let size = stats.st_size as usize;

        // Map shared memory
        let ptr = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                size,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                &shm,
                0,
            )
        }?;

        Ok(Self {
            name,
            _fd: shm,
            shared_memory_ptr: ptr,
            size,
            created: false,
        })
    }

    pub fn slice_ptr(&self) -> *mut [u8] {
        slice_from_raw_parts_mut(self.shared_memory_ptr.cast(), self.size)
    }
}

unsafe impl Send for SharedMemoryHolder {}

impl Drop for SharedMemoryHolder {
    fn drop(&mut self) {
        if let Err(e) = unsafe { rustix::mm::munmap(self.shared_memory_ptr.cast(), self.size) } {
            eprintln!("Failed to unmap shared memory: {}", e);
        }

        if self.created {
            if let Err(e) = rustix::shm::shm_unlink(&self.name) {
                eprintln!("Failed to unlink shared memory: {}", e);
            }
        }
    }
}
