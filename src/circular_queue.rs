use crate::primitives::condvar::SharedCondvar;
use crate::primitives::mutex::SharedMutex;
use std::cmp::Ordering;
use std::mem::size_of;

#[repr(C)]
pub(crate) struct CircularQueue<T: ?Sized = CircularQueueContent> {
    wait_for_read: SharedCondvar,
    wait_for_write: SharedCondvar,
    content: SharedMutex<T>,
}

impl CircularQueue {
    pub(crate) fn init(&self, max_element_size: usize, capacity: usize) {
        let mut content = self.content.lock();

        content.max_element_size = max_element_size as u32;
        content.capacity = capacity as u32;
    }

    pub(crate) fn len(&self) -> usize {
        let content = self.content.lock();

        content.len() as usize
    }

    pub(crate) fn is_full(&self) -> bool {
        let content = self.content.lock();

        content.full
    }

    pub(crate) fn try_write(&self, value: &[u8]) -> bool {
        let mut content = self.content.lock();
        if content.full {
            return false;
        }

        content.write(value);
        self.wait_for_write.notify_one();

        true
    }

    pub(crate) fn blocking_write(&self, value: &[u8]) {
        let mut content = self.content.lock();
        if content.full {
            content = self.wait_for_read.wait_while(content, |guard| guard.full);
        }

        content.write(value);
        self.wait_for_write.notify_one();
    }

    pub(crate) fn try_read(&self, value: &mut [u8]) -> usize {
        let mut content = self.content.lock();

        if content.len() == 0 {
            return 0;
        }

        let size = content.read(value);
        self.wait_for_read.notify_one();
        size
    }

    pub(crate) fn blocking_read(&self, value: &mut [u8]) -> usize {
        let mut content = self.content.lock();
        if content.len() == 0 {
            content = self
                .wait_for_write
                .wait_while(content, |guard| guard.len() == 0);
        }

        let size = content.read(value);
        self.wait_for_read.notify_one();
        size
    }

    pub(crate) fn max_element_size(&self) -> usize {
        let content = self.content.lock();
        content.max_element_size as usize
    }

    pub(crate) fn read_all(&self, buffer: &mut [u8]) -> Vec<Vec<u8>> {
        let mut content = self.content.lock();
        let size = content.len() as usize;

        let data = (0..size)
            .map(|_| {
                let length = content.read(buffer);
                buffer[..length].to_vec()
            })
            .collect();

        self.wait_for_read.notify_all();
        data
    }

    pub(crate) fn compute_size_for(max_element_size: usize, capacity: usize) -> usize {
        CircularQueue::size_of_fields()
            + (size_of::<ElementSizeType>() + max_element_size) * capacity
    }

    const fn size_of_fields() -> usize {
        #[repr(C)]
        struct CircularQueueContentSized {
            writer_index: u32,
            reader_index: u32,
            max_element_size: u32,
            capacity: u32,
            full: bool,
        }
        size_of::<CircularQueue<CircularQueueContentSized>>()
    }
}

#[repr(C)]
pub(crate) struct CircularQueueContent {
    writer_index: u32,
    reader_index: u32,
    max_element_size: u32,
    capacity: u32,
    full: bool,
    buffer: [u8],
}

pub type ElementSizeType = usize;

const ELEMENT_SIZE_TYPE: usize = size_of::<ElementSizeType>();

impl CircularQueueContent {
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

        let buffer_index =
            self.writer_index as usize * (ELEMENT_SIZE_TYPE + self.max_element_size as usize);
        let data_index = buffer_index + ELEMENT_SIZE_TYPE;
        let element_size = value.len();

        self.buffer[buffer_index..data_index]
            .clone_from_slice(&(element_size as ElementSizeType).to_ne_bytes());

        self.buffer[data_index..data_index + element_size].clone_from_slice(value);
    }

    pub(crate) fn read(&mut self, value: &mut [u8]) -> usize {
        assert_eq!(value.len(), self.max_element_size as usize);
        self.reader_index = self.next_inc(self.reader_index);
        self.full = false;

        let buffer_index =
            self.reader_index as usize * (ELEMENT_SIZE_TYPE + self.max_element_size as usize);
        let data_index = buffer_index + ELEMENT_SIZE_TYPE;
        let element_size = ElementSizeType::from_ne_bytes(
            self.buffer[buffer_index..data_index].try_into().unwrap(),
        );

        value[..element_size].clone_from_slice(&self.buffer[data_index..data_index + element_size]);

        element_size
    }

    #[inline]
    fn next_inc(&self, i: u32) -> u32 {
        (i + 1) % self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
        unsafe { core::slice::from_raw_parts((p as *const T) as *const u8, size_of::<T>()) }
    }

    #[test]
    fn read_write() {
        #[derive(Debug, PartialEq)]
        struct Data {
            v1: i32,
            v2: f64,
        }

        const ELEMENT_SIZE: usize = size_of::<Data>();
        let capacity = 5;

        let init_buffer_size = CircularQueue::compute_size_for(ELEMENT_SIZE, capacity);
        let mut init_vec = vec![0u8; init_buffer_size];
        let init_buffer = init_vec.as_mut_slice() as *mut [u8];

        let queue = unsafe { &mut *(init_buffer as *mut CircularQueue) };
        queue.init(ELEMENT_SIZE, capacity);

        let write = |data: &Data| queue.try_write(any_as_u8_slice(data));

        let read = || {
            let mut buffer = [0u8; ELEMENT_SIZE];
            assert_eq!(queue.try_read(&mut buffer), ELEMENT_SIZE);
            unsafe { std::mem::transmute(buffer) }
        };

        let expected_d1 = Data { v1: 1, v2: 2.0 };
        write(&expected_d1);
        assert_eq!(expected_d1, read());

        for i in 0..1000 {
            let data = Data {
                v1: i,
                v2: i as f64,
            };
            write(&data);
            assert_eq!(data, read());
        }

        for i in 0..capacity {
            let data = Data {
                v1: i as i32,
                v2: i as f64,
            };
            write(&data);
        }
        for i in 0..capacity {
            let data = Data {
                v1: i as i32,
                v2: i as f64,
            };
            assert_eq!(data, read());
        }
    }
}
