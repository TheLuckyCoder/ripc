use crate::utils::pthread_lock::PThreadLock;
use libc::pthread_mutex_t;
use std::cmp::Ordering;
use std::mem::size_of;
use std::time::Duration;

#[repr(C)]
pub(crate) struct CircularBuffer {
    content: PThreadLock<CircularBufferContent>,
}

impl CircularBuffer {
    pub(crate) fn init(&self, max_element_size: usize, capacity: usize) {
        let mut content = self.content.lock().unwrap();

        content.max_element_size = max_element_size as u32;
        content.capacity = capacity as u32;
    }

    pub(crate) fn len(&self) -> usize {
        let content = self.content.lock().unwrap();

        content.len() as usize
    }

    pub(crate) fn is_full(&self) -> bool {
        let content = self.content.lock().unwrap();

        content.full
    }

    pub(crate) fn try_write(&self, value: &[u8]) -> bool {
        let mut content = self.content.lock().unwrap();
        if content.full {
            return false;
        }

        content.write(value);

        true
    }

    pub(crate) fn blocking_write(&self, value: &[u8]) {
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

    pub(crate) fn try_read(&self, value: &mut [u8]) -> bool {
        let mut content = self.content.lock().unwrap();

        if content.len() == 0 {
            return false;
        }

        content.read(value);

        true
    }

    pub(crate) fn blocking_read(&self, value: &mut [u8]) {
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

    pub(crate) fn max_element_size(&self) -> usize {
        let content = self.content.lock().unwrap();
        content.max_element_size as usize
    }

    pub(crate) fn read_all(&self) -> Vec<Vec<u8>> {
        let mut content = self.content.lock().unwrap();
        let size = content.len() as usize;
        let mut buffer = Vec::with_capacity(content.capacity as usize);

        (0..size).map(|_| {
            let length = content.read(&mut buffer);
            buffer[..length].to_vec()
        }).collect()
    }

    pub(crate) const fn size_of_fields() -> usize {
        size_of::<pthread_mutex_t>() + size_of::<u32>() * 4 + size_of::<bool>()
    }
}

#[repr(C)]
struct CircularBufferContent {
    writer_index: u32,
    reader_index: u32,
    max_element_size: u32,
    capacity: u32,
    full: bool,
    buffer: [u8],
}

pub type ElementSizeType = usize;

const ELEMENT_SIZE_TYPE: usize = size_of::<ElementSizeType>();

impl CircularBufferContent {
    pub(crate) fn len(&self) -> u32 {
        if self.full {
            return self.capacity;
        }

        match self.writer_index.cmp(&self.reader_index) {
            Ordering::Less => self.capacity - self.reader_index + self.writer_index,
            Ordering::Equal => 0,
            Ordering::Greater => self.writer_index - self.reader_index,
        }
    }

    pub(crate) fn write(&mut self, value: &[u8]) {
        self.writer_index = self.next_inc(self.writer_index);
        self.full = self.writer_index == self.reader_index;

        let buffer_index = (self.writer_index * (ELEMENT_SIZE_TYPE as u32 + self.max_element_size)) as usize;
        let data_index = buffer_index + ELEMENT_SIZE_TYPE;
        let element_size = value.len();

        self.buffer[buffer_index..data_index].clone_from_slice(&(element_size as ElementSizeType).to_ne_bytes());

        self.buffer[data_index..data_index + element_size].clone_from_slice(value);
    }

    pub(crate) fn read(&mut self, value: &mut [u8]) -> usize {
        self.reader_index = self.next_inc(self.reader_index);
        self.full = false;

        let buffer_index = (self.reader_index * (ELEMENT_SIZE_TYPE as u32 + self.max_element_size)) as usize;
        let data_index = buffer_index + ELEMENT_SIZE_TYPE;
        let element_size = ElementSizeType::from_ne_bytes(
            self.buffer[buffer_index..buffer_index + ELEMENT_SIZE_TYPE]
                .try_into()
                .unwrap(),
        );

        value.clone_from_slice(&self.buffer[data_index..data_index + element_size]);

        element_size
    }

    #[inline]
    fn next_inc(&self, i: u32) -> u32 {
        (i + 1) % self.capacity
    }
}
