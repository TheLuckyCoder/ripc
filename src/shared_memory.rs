use crate::primitives::condvar::SharedCondvar;
use crate::primitives::mutex::SharedMutex;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[repr(C)]
pub struct SharedMemory {
    pub closed: AtomicBool,
    pub condvar: SharedCondvar,
    pub version: AtomicUsize,
    pub data: SharedMutex<SharedMemoryData>,
}

impl SharedMemory {
    pub(crate) const fn size_of_fields() -> usize {
        size_of::<usize>() * 4
    }

    pub(crate) fn write_message(&mut self, data_to_send: &[u8]) -> std::io::Result<usize> {
        let mut content = self.data.lock()?;

        if data_to_send.len() > content.data.len() {
            return Err(std::io::Error::other(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                content.data.len(),
                data_to_send.len()
            )));
        }

        let new_version = self.version.fetch_add(1, Ordering::Relaxed);
        content.message_size = data_to_send.len();
        unsafe {
            ptr::copy_nonoverlapping(
                data_to_send.as_ptr(),
                content.data.as_mut_ptr(),
                data_to_send.len(),
            );
        }
        self.condvar.notify_all();
        
        Ok(new_version)
    }
}

#[repr(C)]
pub struct SharedMemoryData {
    pub(crate) message_size: usize,
    pub(crate) data: [u8],
}
