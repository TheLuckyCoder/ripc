use std::cmp::Ordering;
use std::mem::size_of;
use std::time::Duration;
use crate::pthread_lock::PThreadLock;

#[repr(C)]
pub(crate) struct CircularBuffer {
    lock: PThreadLock,
    writer_index: usize,
    reader_index: usize,
    elem_size: usize,
    size: usize,
    full: bool,
    buffer: [u8],
}

// Public API
impl CircularBuffer {
    pub(crate) fn len(&mut self) -> usize {
        let _guard = unsafe { self.lock.acquire_lock().unwrap() };
        
        self.len_internal()
    }
    
    pub(crate) fn is_full(&mut self) -> bool {
        let _guard = unsafe { self.lock.acquire_lock().unwrap() };
        
        self.full
    }

    pub(crate) fn try_write(&mut self, value: &[u8]) -> bool {
        // DO less than size check and think about handling
        let _guard = unsafe { self.lock.acquire_lock().unwrap() };
        if self.full {
            return false;
        }
        
        self.write_internal(value);
        
        true
    }
    
    pub(crate) fn blocking_write(&mut self, value: &[u8]) {
        loop {
            let _guard = unsafe { self.lock.acquire_lock().unwrap() };
            if self.full {
                std::thread::sleep(Duration::from_micros(100));
                continue;
            }

            self.write_internal(value);
            break;
        }
    }
    
    pub(crate) fn try_read(&mut self, value: &mut [u8]) -> bool {
        let _guard = unsafe { self.lock.acquire_lock().unwrap() };
        
        if self.len_internal() == 0 {
            return false;
        }
        
        self.read_internal(value);
        
        true
    }
    
    pub(crate) fn blocking_read(&mut self, value: &mut [u8]) {
        loop {
            let _guard = unsafe { self.lock.acquire_lock().unwrap() };
            if self.len_internal() == 0 {
                std::thread::sleep(Duration::from_micros(100));
                continue;
            }

            self.write_internal(value);
            break;
        }
    }
    
    pub(crate) const fn size_of_fields() -> usize {
        size_of::<PThreadLock>() + size_of::<usize>() * 4 + size_of::<bool>()
    }
}

impl CircularBuffer {
    fn len_internal(&self) -> usize {
        if self.full {
            return self.size;
        }
        
        match self.writer_index.cmp(&self.reader_index) {
            Ordering::Less => self.size - self.reader_index + self.writer_index,
            Ordering::Equal => 0,
            Ordering::Greater => self.writer_index - self.reader_index
        }
    }
    
    fn write_internal(&mut self, value: &[u8]) {
        assert!(value.len() < self.elem_size);
        let buffer_index = self.writer_index * self.elem_size;
        self.inc_write_index();
        self.buffer[buffer_index..buffer_index + value.len()].clone_from_slice(value);
    }

    fn read_internal(&mut self, value: &mut [u8]) {
        let buffer_index = self.reader_index * self.elem_size;
        self.inc_read_index();
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
