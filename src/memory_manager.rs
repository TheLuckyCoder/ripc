use std::ffi::{CStr, CString};
use std::mem::MaybeUninit;
use std::os::fd::OwnedFd;
use std::ptr::slice_from_raw_parts_mut;

use rustix::fs::Mode;
use rustix::mm::{MapFlags, ProtFlags};
use rustix::shm::ShmOFlags;
use crate::shared_memory::SharedMemory;

pub struct SharedMemoryManager {
    name: Option<CString>,
    _fd: OwnedFd,
    shared_memory_ptr: *mut SharedMemory,
    size: usize,
}

impl SharedMemoryManager {
    pub fn create(name: CString, size: usize) -> std::io::Result<Self> {
        // Open shared memory
        let shm = rustix::shm::shm_open(name.as_c_str(), ShmOFlags::CREATE | ShmOFlags::RDWR, Mode::all())?;

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
            Ok(ptr) => {
                let memory_slice: *mut [u8] = slice_from_raw_parts_mut(ptr.cast(), size);
                let shared_memory_ptr = memory_slice as *mut SharedMemory;
                let mut attr: MaybeUninit<libc::pthread_rwlockattr_t> = MaybeUninit::uninit();

                unsafe {
                    libc::pthread_rwlockattr_init(attr.as_mut_ptr());
                    libc::pthread_rwlockattr_setpshared(attr.as_mut_ptr(), libc::PTHREAD_PROCESS_SHARED);
                    libc::pthread_rwlockattr_setkind_np(attr.as_mut_ptr(), 2);//libc::PTHREAD_RWLOCK_PREFER_WRITER_NONRECURSIVE_NP)
                    libc::pthread_rwlock_init(shared_memory_ptr.cast(), attr.as_mut_ptr());
                };

                Ok(Self {
                    name: Some(name),
                    _fd: shm,
                    shared_memory_ptr,
                    size,
                })
            }
            Err(e) => {
                let _ = rustix::shm::shm_unlink(name);
                Err(e.into())
            }
        }
    }
    pub fn open(name: &CStr) -> std::io::Result<Self> {
        // Open shared memory
        let shm = rustix::shm::shm_open(name, ShmOFlags::RDWR, Mode::all())?;

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


        let memory_slice: *mut [u8] = slice_from_raw_parts_mut(ptr.cast(), size);
        let shared_memory_ptr = memory_slice as *mut SharedMemory;

        Ok(Self {
            name: None,
            _fd: shm,
            shared_memory_ptr,
            size,
        })
    }

    pub fn get_ptr(&self) -> *mut SharedMemory {
        self.shared_memory_ptr
    }

    pub fn memory_size(&self) -> usize { self.size }
}

unsafe impl Send for SharedMemoryManager {}

impl Drop for SharedMemoryManager {
    fn drop(&mut self) {
        if let Err(e) = unsafe { rustix::mm::munmap(self.shared_memory_ptr.cast(), self.size) } {
            eprintln!("Failed to unmap shared memory: {}", e);
        }

        if let Some(ref name) = self.name {
            if let Err(e) = rustix::shm::shm_unlink(name.as_c_str()) {
                eprintln!("Failed to unlink shared memory: {}", e);
            }
        }
    }
}
