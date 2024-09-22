use crate::utils::pthread_rw_lock::PThreadRwLock;
use libc::pthread_rwlock_t;
use std::mem::size_of;
use std::ptr;

#[repr(C)]
pub struct SharedMemoryMessage {
    pub(crate) lock: PThreadRwLock<SharedMemoryMessageContent>,
}

#[repr(C)]
pub struct SharedMemoryMessageContent {
    pub(crate) version: usize,
    pub(crate) message_size: usize,
    pub(crate) closed: bool,
    pub(crate) data: [u8],
}

pub enum ReadingResult {
    MessageSize(usize),
    SameVersion,
    Closed,
    FailedCreatingLock(std::io::Error),
}

impl SharedMemoryMessage {
    pub(crate) const fn size_of_fields() -> usize {
        size_of::<pthread_rwlock_t>() + size_of::<usize>() * 2 + size_of::<bool>()
    }

    pub fn write_message(&mut self, data_to_send: &[u8]) -> std::io::Result<()> {
        let mut content = self.lock.write_lock()?;

        if data_to_send.len() > content.data.len() {
            return Err(std::io::Error::other(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                content.data.len(),
                data_to_send.len()
            )));
        }

        content.version += 1;
        content.message_size = data_to_send.len();
        unsafe {
            ptr::copy_nonoverlapping(
                data_to_send.as_ptr(),
                content.data.as_mut_ptr(),
                data_to_send.len(),
            );
        }
        Ok(())
    }

    pub fn read_message(&self, last_read_version: &mut usize, buffer: &mut [u8]) -> ReadingResult {
        let content = match self.lock.read_lock() {
            Ok(content) => content,
            Err(e) => return ReadingResult::FailedCreatingLock(e),
        };

        if content.closed {
            return ReadingResult::Closed;
        }

        let message_version = content.version;
        if message_version == *last_read_version {
            return ReadingResult::SameVersion; // There is no new data
        }
        *last_read_version = message_version;

        let size = content.message_size;
        buffer[..size].copy_from_slice(&content.data[..size]);

        ReadingResult::MessageSize(size)
    }
}
