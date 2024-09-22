use libc::pthread_mutex_t;
use std::cmp::Ordering;
use std::mem::size_of;
use std::time::Duration;
use crate::utils::pthread_lock::PThreadLock;

#[repr(C)]
pub(crate) struct CircularBuffer {
    content: PThreadLock<CircularBufferContent>,
}

impl CircularBuffer {

    pub(crate) fn len(&self) -> usize {
        let content = self.content.lock().unwrap();

        content.len()
    }

    pub(crate) fn is_full(&self) -> bool {
        let content = self.content.lock().unwrap();

        content.full
    }

    pub(crate) fn try_write(&mut self, value: &[u8]) -> bool {
        // TODO less than size check and think about handling
        let mut content = self.content.lock().unwrap();
        if content.full {
            return false;
        }

        content.write(value);

        true
    }

    pub(crate) fn blocking_write(&mut self, value: &[u8]) {
        loop {
            let mut content = self.content.lock().unwrap();
            if content.full {
                std::thread::sleep(Duration::from_micros(100));
                continue;
            }

            content.write(value);
            break;
        }
    }

    pub(crate) fn try_read(&mut self, value: &mut [u8]) -> bool {
        let mut content = self.content.lock().unwrap();

        if content.len() == 0 {
            return false;
        }

        content.read(value);

        true
    }

    pub(crate) fn blocking_read(&mut self, value: &mut [u8]) {
        loop {
            let mut content = self.content.lock().unwrap();
            if content.len() == 0 {
                std::thread::sleep(Duration::from_micros(100));
                continue;
            }

            content.write(value);
            break;
        }
    }

    pub(crate) const fn size_of_fields() -> usize {
        size_of::<pthread_mutex_t>() + size_of::<usize>() * 4 + size_of::<bool>()
    }
}

#[repr(C)]
struct CircularBufferContent {
    writer_index: usize,
    reader_index: usize,
    elem_size: usize,
    size: usize,
    full: bool,
    buffer: [u8],
}

impl CircularBufferContent {
    pub(crate) fn len(&self) -> usize {
        if self.full {
            return self.size;
        }

        match self.writer_index.cmp(&self.reader_index) {
            Ordering::Less => self.size - self.reader_index + self.writer_index,
            Ordering::Equal => 0,
            Ordering::Greater => self.writer_index - self.reader_index,
        }
    }

    pub(crate) fn write(&mut self, value: &[u8]) {
        assert!(value.len() < self.elem_size);
        let buffer_index = self.writer_index * self.elem_size;
        self.inc_write_index();
        self.full = self.writer_index == self.reader_index;
        
        self.buffer[buffer_index..buffer_index + value.len()].clone_from_slice(value);
    }

    pub(crate) fn read(&mut self, value: &mut [u8]) {
        let buffer_index = self.reader_index * self.elem_size;
        self.inc_read_index();
        self.full = false;
        
        value.clone_from_slice(&self.buffer[buffer_index..buffer_index + value.len()]);
    }

    #[inline]
    fn next_inc(&self, i: usize) -> usize {
        (i + 1) % self.size
    }

    fn inc_write_index(&mut self) {
        self.writer_index = self.next_inc(self.writer_index);
    }

    fn inc_read_index(&mut self) {
        self.reader_index = self.next_inc(self.reader_index);
    }
}
