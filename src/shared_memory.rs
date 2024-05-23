use std::mem::size_of;
use std::ptr;
use libc::pthread_rwlock_t;

#[repr(C)]
pub struct SharedMemory {
    pub(crate) lock: pthread_rwlock_t,
    pub(crate) version: usize,
    pub(crate) message_size: usize,
    pub(crate) data: [u8],
}

impl SharedMemory {
    pub(crate) fn size_of_other_fields() -> usize {
        size_of::<pthread_rwlock_t>() + size_of::<usize>() * 2
    }
    
    pub(crate) unsafe fn read_lock(&self) -> Result<PThreadRwLockGuard, String> {
        let ptr = (&self.lock as *const pthread_rwlock_t) as *mut pthread_rwlock_t;
        let result = libc::pthread_rwlock_rdlock(ptr);

        if result != 0 {
            Err(format!("Failed to create write lock: {}", result))
        } else {
            Ok(PThreadRwLockGuard::create(ptr))
        }
    }

    pub(crate) unsafe fn write_lock(&mut self) -> Result<PThreadRwLockGuard, String> {
        let ptr = &mut self.lock as *mut pthread_rwlock_t;
        let result = libc::pthread_rwlock_wrlock(ptr);

        if result != 0 {
            Err(format!("Failed to create write lock: {}", result))
        } else {
            Ok(PThreadRwLockGuard::create(ptr))
        }
    }


    pub fn write_message_safe(&mut self, data_to_send: &[u8]) -> Result<(), String> {
        if data_to_send.len() > self.data.len() {
            return Err(format!("Message is too large to be sent! Max size: {}. Current message size: {}", data_to_send.len(), self.data.len()));
        }

        let _guard = unsafe { self.write_lock()? };

        self.version += 1;
        self.message_size = data_to_send.len();
        unsafe { ptr::copy_nonoverlapping(data_to_send.as_ptr(), self.data.as_mut_ptr(), data_to_send.len()); }
        Ok(())
    }
}

pub(crate) struct PThreadRwLockGuard {
    lock: *mut pthread_rwlock_t,
}

impl PThreadRwLockGuard {
    fn create(lock: *mut pthread_rwlock_t) -> Self {
        Self {
            lock,
        }
    }
}

impl Drop for PThreadRwLockGuard {
    fn drop(&mut self) {
        unsafe { libc::pthread_rwlock_unlock(self.lock); }
    }
}

pub fn read_message(
    shared_memory: &SharedMemory,
    last_read_version: &mut usize,
    buffer: &mut [u8],
) -> Option<usize> {
    let _guard = unsafe { shared_memory.read_lock().ok()? };

    let message_version = shared_memory.version;
    // if message_version == *last_read_version {
    //     return None; // There is no new data
    // }
    *last_read_version = message_version;

    let size = shared_memory.message_size;
    buffer[..size].copy_from_slice(&shared_memory.data[..size]);

    Some(size)
}