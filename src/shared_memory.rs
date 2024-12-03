use std::ptr;
use std::sync::atomic::AtomicBool;
use crate::primitives::mutex::SharedMutex;

pub enum ReadingMetadata {
    NewMessage(usize),
    SameVersion,
}

#[repr(C)]
pub struct SharedMemoryMessage {
    pub(crate) closed: AtomicBool,
    pub(crate) data: SharedMutex<SharedMemoryMessageContent>,
}

impl SharedMemoryMessage {
    pub(crate) const fn size_of_fields() -> usize {
        size_of::<u64>() + size_of::<u32>() + size_of::<usize>() * 2 + size_of::<bool>()
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

        let new_version = content.version.wrapping_add(1);
        content.version = new_version;
        content.message_size = data_to_send.len();
        unsafe {
            ptr::copy_nonoverlapping(
                data_to_send.as_ptr(),
                content.data.as_mut_ptr(),
                data_to_send.len(),
            );
        }
        Ok(new_version)
    }
}

#[repr(C)]
pub struct SharedMemoryMessageContent {
    pub(crate) version: usize,
    pub(crate) message_size: usize,
    pub(crate) data: [u8],
}

impl SharedMemoryMessageContent {
    #[inline]
    pub(crate) fn get_message_metadata(&self, last_read_version: &mut usize) -> ReadingMetadata {
        let message_version = self.version;
        if message_version == *last_read_version {
            return ReadingMetadata::SameVersion;
        }
        let size = self.message_size;
        *last_read_version = message_version;

        ReadingMetadata::NewMessage(size)
    }
}
