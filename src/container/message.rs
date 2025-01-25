use crate::primitives::condvar::SharedCondvar;
use crate::primitives::memory_holder::SlicePtrCast;
use crate::primitives::mutex::SharedMutex;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// 32 - readers_count
// 32 - write_condvar
// 32 - read_condvar
// 8 + 24pad - closed
// 64 - version
// 32 + 32pad - mutex
// 32 + 32pad - reader_version_count
// 64 - current message size
// 64 - max message size
// N - bytes

#[repr(C)]
pub struct SharedMessage<T: ?Sized = SharedMessageData> {
    version: AtomicUsize,
    closed: AtomicBool,
    write_condvar: SharedCondvar,
    read_condvar: SharedCondvar,
    data: SharedMutex<T>,
}

#[repr(C)]
pub struct SharedMessageData {
    consumer_count: u32,
    read_count: u32,
    size: usize,
    // Note: The current way the void pointer is cast to a struct, leads to len() returning an incorrect value
    payload: [u8],
}

impl SharedMessageData {
    #[inline]
    fn copy(&mut self, data: &[u8]) {
        let data_len = data.len();

        self.read_count = 0;
        self.size = data_len;
        self.payload[..data_len].copy_from_slice(data);
    }
}

impl SharedMessage {
    pub(crate) const fn size_of_fields() -> usize {
        #[repr(C)]
        struct SharedMemoryDataSized {
            consumer_count: u32,
            read_count: u32,
            size: usize,
        }
        size_of::<SharedMessage<SharedMemoryDataSized>>()
    }

    pub(crate) fn write(&self, data: &[u8]) -> usize {
        let mut content = self.data.lock();

        let old_version = self.version.fetch_add(1, Ordering::Relaxed);
        content.copy(data);

        self.write_condvar.notify_all();

        old_version + 1
    }

    pub(crate) fn write_waiting_for_readers(
        &self,
        data: &[u8],
        wait_for: Option<NonZeroU32>,
    ) -> usize {
        let mut content = self.data.lock();
        let wait_for_count = wait_for.map(|v| v.get().min(content.consumer_count));

        if self.version.load(Ordering::Relaxed) != 0 {
            content = self.read_condvar.wait_while(content, |lock| {
                lock.read_count < wait_for_count.unwrap_or(lock.consumer_count)
            });
        }

        let old_version = self.version.fetch_add(1, Ordering::Relaxed);
        content.copy(data);

        self.write_condvar.notify_all();

        old_version + 1
    }

    pub(crate) fn try_read(&self, current_version: usize, mut read: impl FnMut(usize, &[u8])) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }

        // Read the version to check if there is a new one
        if current_version == self.version.load(Ordering::Relaxed) {
            return;
        }

        let mut data = self.data.lock();
        // Read the version again after the lock has been acquired
        let new_version = self.version.load(Ordering::Relaxed);

        read(new_version, &data.payload[..data.size]);
        data.read_count += 1;
        self.read_condvar.notify_all();
    }

    pub(crate) fn blocking_read(&self, current_version: usize, mut read: impl FnMut(usize, &[u8])) {
        let mut data = self.data.lock();
        loop {
            if self.closed.load(Ordering::Relaxed) {
                return;
            }

            let new_version = self.version.load(Ordering::Relaxed);
            if new_version != current_version {
                read(new_version, &data.payload[..data.size]);
                data.read_count += 1;
                self.read_condvar.notify_all();
                return;
            }

            // Wait for new version
            data = self.write_condvar.wait(data);
        }
    }

    pub(crate) fn is_new_version_available(&self, current_version: usize) -> bool {
        let version = self.version.load(Ordering::Relaxed);

        version != current_version
    }

    pub(crate) fn add_reader(&self) {
        let mut content = self.data.lock();
        content.consumer_count += 1;
    }

    pub(crate) fn remove_reader(&self) {
        let mut content = self.data.lock();
        content.consumer_count -= 1;
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub(crate) fn close(&self) {
        let _ = self.data.lock();
        self.closed.store(true, Ordering::Relaxed);
        self.write_condvar.notify_all();
    }
}

impl SlicePtrCast for SharedMessage {
    fn cast_from_slice_ptr(slice_ptr: *mut [u8]) -> *const Self {
        slice_ptr as *const Self
    }
}
