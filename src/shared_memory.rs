use std::mem::size_of;
use std::ptr;

use crate::pthread_lock::PThreadRwLock;

#[repr(C)]
pub struct SharedMemory {
    pub(crate) lock: PThreadRwLock,
    pub(crate) version: usize,
    pub(crate) message_size: usize,
    pub(crate) data: [u8],
}

impl SharedMemory {
    pub(crate) fn size_of_other_fields() -> usize {
        size_of::<PThreadRwLock>() + size_of::<usize>() * 2
    }

    pub fn write_message(&mut self, data_to_send: &[u8]) -> Result<(), String> {
        if data_to_send.len() > self.data.len() {
            return Err(format!("Message is too large to be sent! Max size: {}. Current message size: {}", self.data.len(), data_to_send.len()));
        }

        let _guard = unsafe { self.lock.write_lock()? };

        self.version += 1;
        self.message_size = data_to_send.len();
        unsafe { ptr::copy_nonoverlapping(data_to_send.as_ptr(), self.data.as_mut_ptr(), data_to_send.len()); }
        Ok(())
    }
}


pub fn read_message(
    shared_memory: &SharedMemory,
    last_read_version: &mut usize,
    buffer: &mut [u8],
) -> Option<usize> {
    let _guard = unsafe { shared_memory.lock.read_lock().ok()? };

    let message_version = shared_memory.version;
    // if message_version == *last_read_version {
    //     return None; // There is no new data
    // }
    *last_read_version = message_version;

    let size = shared_memory.message_size;
    buffer[..size].copy_from_slice(&shared_memory.data[..size]);

    Some(size)
}