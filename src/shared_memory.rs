use crate::primitives::condvar::SharedCondvar;
use crate::primitives::mutex::SharedMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[repr(C)]
pub struct SharedMemory<T: ?Sized = SharedMemoryData> {
    pub closed: AtomicBool,
    pub condvar: SharedCondvar,
    pub version: AtomicUsize,
    pub data: SharedMutex<T>,
}

#[repr(C)]
pub struct SharedMemoryData {
    pub(crate) size: usize,
    pub(crate) bytes: [u8],
}

impl SharedMemory {
    pub(crate) const fn size_of_fields() -> usize {
        #[repr(C)]
        struct SharedMemoryDataSized {
            size: usize,
        }
        size_of::<SharedMemory<SharedMemoryDataSized>>()
    }

    pub(crate) fn write_message(&self, data_to_send: &[u8]) -> usize {
        let mut content = self.data.lock();
        let data_len = data_to_send.len();

        let old_version = self.version.fetch_add(1, Ordering::Relaxed);
        content.size = data_len;
        content.bytes[..data_len].copy_from_slice(data_to_send);

        self.condvar.notify_all();

        old_version + 1
    }
}
