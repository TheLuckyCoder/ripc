use crate::primitives::condvar::SharedCondvar;
use crate::primitives::mutex::SharedMutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[repr(C)]
pub struct SharedMemory<T: ?Sized = SharedMemoryData> {
    pub readers_count: AtomicUsize,
    pub closed: AtomicBool,
    pub write_condvar: SharedCondvar,
    pub read_condvar: SharedCondvar,
    pub version: AtomicUsize,
    pub reader_version: AtomicUsize,
    pub reader_version_count: AtomicUsize,
    pub data: SharedMutex<T>,
}

#[repr(C)]
pub struct SharedMemoryData {
    pub(crate) size: usize,
    pub(crate) bytes: [u8],
}

impl SharedMemoryData {
    pub(crate) fn copy(&mut self, data: &[u8]) {
        let data_len = data.len();

        self.size = data_len;
        self.bytes[..data_len].copy_from_slice(data);
    }
}

impl SharedMemory {
    pub(crate) const fn size_of_fields() -> usize {
        #[repr(C)]
        struct SharedMemoryDataSized {
            size: usize,
        }
        size_of::<SharedMemory<SharedMemoryDataSized>>()
    }

    pub(crate) fn write(&self, data: &[u8]) -> usize {
        let mut content = self.data.lock();

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

        let content = self.data.lock();
        // Read the version again after the lock has been acquired
        let new_version = self.version.load(Ordering::Relaxed);

        read(new_version, &content.bytes[..content.size]);
        self.update_reader_version(new_version);
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
                read(new_version, &data.bytes[..data.size]);
                self.update_reader_version(new_version);
                return;
            }

            // Wait for new version
            data = self.write_condvar.wait(data);
        }
    }

    pub(crate) fn is_new_version_available(&self, reference_version: usize) -> bool {
        let version = self.version.load(Ordering::Relaxed);

        version != reference_version
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub(crate) fn close(&self) {
        let _ = self.data.lock();
        self.closed.store(true, Ordering::Relaxed);
        self.write_condvar.notify_all();
    }

    fn update_reader_version(&self, new_version: usize) {
        let previous_reader_version = self.reader_version.swap(new_version, Ordering::Relaxed);
        if previous_reader_version == new_version {
            self.reader_version_count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.reader_version_count.store(1, Ordering::Relaxed);
        }
        self.read_condvar.notify_one();
    }
}
