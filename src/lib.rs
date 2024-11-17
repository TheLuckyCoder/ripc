use crate::circular_queue::CircularQueue;
use crate::memory_holder::SharedMemoryHolder;
use crate::shared_memory::{ReadingMetadata, SharedMemoryMessage};
use crate::utils::pthread_lock::pthread_lock_initialize_at;
use crate::utils::pthread_rw_lock::pthread_rw_lock_initialize_at;
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::cell::RefCell;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::ptr::slice_from_raw_parts_mut;
use std::sync::atomic::{AtomicUsize, Ordering};

mod circular_queue;
mod memory_holder;
mod shared_memory;
mod utils;

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryWriter {
    shared_memory: SharedMemoryHolder,
    name: String,
    message_length: usize,
    last_written_version: AtomicUsize,
}

#[pymethods]
impl SharedMemoryWriter {
    #[new]
    fn new(name: String, size: NonZeroU32) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;
        let size = size.get() as usize;

        let shared_memory =
            SharedMemoryHolder::create(c_name, SharedMemoryMessage::size_of_fields() + size)?;
        unsafe { pthread_rw_lock_initialize_at(shared_memory.ptr().cast())? };

        Ok(Self {
            shared_memory,
            name,
            message_length: size,
            last_written_version: AtomicUsize::default(),
        })
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };

        let version_count = py.allow_threads(|| shared_memory.write_message(data))?;
        self.last_written_version
            .store(version_count, Ordering::Relaxed);

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn total_allocated_size(&self) -> usize {
        self.shared_memory.memory_size()
    }

    fn size(&self) -> usize {
        self.message_length
    }

    fn last_written_version(&self) -> usize {
        self.last_written_version.load(Ordering::Relaxed)
    }

    fn close(&self) -> PyResult<()> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let mut content = shared_memory.lock.write_lock()?;

        content.closed = true;

        Ok(())
    }
}

unsafe impl Sync for SharedMemoryWriter {}

impl Drop for SharedMemoryWriter {
    fn drop(&mut self) {
        self.close().unwrap();
    }
}

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryReader {
    shared_memory: SharedMemoryHolder,
    name: String,
    message_length: usize,
    last_version_read: AtomicUsize,
    buffer: RefCell<Vec<u8>>,
}

impl SharedMemoryReader {
    fn get_memory(&self) -> &SharedMemoryMessage {
        unsafe { &*(self.shared_memory.slice_ptr() as *const SharedMemoryMessage) }
    }
}

#[pymethods]
impl SharedMemoryReader {
    #[new]
    fn new(name: String) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let shared_memory = SharedMemoryHolder::open(CString::new(name.clone())?)?;

        let message = unsafe { &*(shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let message_length = message.lock.read_lock()?.data.len();

        Ok(Self {
            buffer: RefCell::new(vec![0u8; shared_memory.memory_size()]),
            name,
            message_length,
            shared_memory,
            last_version_read: AtomicUsize::default(),
        })
    }

    fn try_read(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let mut last_version = self.last_version_read.load(Ordering::Relaxed);

        let content = self.get_memory().lock.read_lock()?;

        let bytes_read = {
            let message_version = content.version;
            if message_version == last_version {
                return Ok(None);
            } else {
                last_version = message_version;
                content.message_size
            }
        };

        self.last_version_read
            .store(last_version, Ordering::Relaxed);
        Ok(Some(
            PyBytes::new(py, &content.data[..bytes_read]).into_py(py),
        ))
    }

    fn try_read_into(&self, buffer_obj: &Bound<'_, PyAny>) -> PyResult<usize> {
        let buffer = PyBuffer::<u8>::get(buffer_obj)?;
        if buffer.readonly() {
            return Err(PyValueError::new_err("Buffer is readonly"));
        }
        if !buffer.is_c_contiguous() {
            return Err(PyValueError::new_err("Buffer is not contiguous"));
        }

        let buffer_ptr = buffer.buf_ptr() as *mut u8;
        let item_count = buffer.item_count();
        let slice = unsafe { &mut *slice_from_raw_parts_mut(buffer_ptr, item_count) };

        let mut last_version = self.last_version_read.load(Ordering::Relaxed);

        let result = match self.get_memory().read_message(&mut last_version, slice)? {
            ReadingMetadata::NewMessage(size) => Ok(size),
            ReadingMetadata::SameVersion => Ok(0),
            ReadingMetadata::Closed => Ok(0),
        };

        self.last_version_read
            .store(last_version, Ordering::Relaxed);

        result
    }

    fn blocking_read(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let mut last_version = self.last_version_read.load(Ordering::Relaxed);
        let mut buffer = self.buffer.borrow_mut();
        let buffer_slice = buffer.as_mut_slice();
        let message = self.get_memory();

        let result = py.allow_threads(|| message.blocking_read(&mut last_version, buffer_slice))?;
        let length = match result {
            ReadingMetadata::NewMessage(size) => size,
            _ => return Ok(None),
        };

        self.last_version_read
            .store(last_version, Ordering::Relaxed);
        Ok(Some(PyBytes::new(py, &buffer[..length]).into_py(py)))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> usize {
        self.message_length
    }

    fn check_message_available(&self) -> PyResult<bool> {
        let last_read_version = self.last_version_read.load(Ordering::Relaxed);
        let content = self.get_memory().lock.read_lock()?;
        Ok(content.version != last_read_version)
    }

    fn last_read_version(&self) -> usize {
        self.last_version_read.load(Ordering::Relaxed)
    }

    fn is_closed(&self) -> PyResult<bool> {
        let content = self.get_memory().lock.read_lock()?;
        Ok(content.closed)
    }
}

unsafe impl Sync for SharedMemoryReader {}

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    max_element_size: usize,
    buffer: RefCell<Vec<u8>>,
}

impl SharedMemoryCircularQueue {
    fn get_queue(&self) -> &CircularQueue {
        unsafe { &*(self.shared_memory.slice_ptr() as *const CircularQueue) }
    }
}

#[pymethods]
impl SharedMemoryCircularQueue {
    #[staticmethod]
    fn create(name: String, max_element_size: NonZeroU32, capacity: NonZeroU32) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let max_element_size = max_element_size.get() as usize;
        let capacity = capacity.get() as usize;

        let shared_memory = SharedMemoryHolder::create(
            CString::new(name.clone())?,
            CircularQueue::compute_size_for(max_element_size, capacity),
        )?;

        unsafe { pthread_lock_initialize_at(shared_memory.ptr().cast())? };

        let queue = unsafe { &mut *(shared_memory.slice_ptr() as *mut CircularQueue) };
        queue.init(max_element_size, capacity);

        Ok(Self {
            shared_memory,
            name,
            max_element_size,
            buffer: RefCell::new(vec![0u8; max_element_size]),
        })
    }

    #[staticmethod]
    fn open(name: String) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let shared_memory = SharedMemoryHolder::open(CString::new(name.clone())?)?;

        let queue = unsafe { &mut *(shared_memory.slice_ptr() as *mut CircularQueue) };
        let max_element_size = queue.max_element_size();

        Ok(Self {
            shared_memory,
            name,
            max_element_size,
            buffer: RefCell::new(vec![0u8; max_element_size]),
        })
    }

    fn __len__(&self) -> usize {
        self.get_queue().len()
    }

    fn is_full(&self) -> bool {
        self.get_queue().is_full()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn try_read(&self, py: Python<'_>) -> Option<PyObject> {
        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        let length = self.get_queue().try_read(buffer);
        if length != 0 {
            Some(PyBytes::new(py, &buffer[..length]).into_py(py))
        } else {
            None
        }
    }

    fn blocking_read(&self, py: Python<'_>) -> PyObject {
        let queue = self.get_queue();

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();
        let length = py.allow_threads(|| queue.blocking_read(buffer));

        PyBytes::new(py, &buffer[..length]).into_py(py)
    }

    fn read_all(&self) -> Vec<Vec<u8>> {
        let mut borrowed_buffer = self.buffer.borrow_mut();
        self.get_queue().read_all(borrowed_buffer.as_mut_slice())
    }

    fn try_write(&self, data: &[u8]) -> PyResult<bool> {
        if data.len() > self.max_element_size {
            return Err(PyValueError::new_err(format!(
                "Data size {} exceeds max element size {}",
                data.len(),
                self.max_element_size
            )));
        }
        Ok(self.get_queue().try_write(data))
    }

    fn blocking_write(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if data.len() > self.max_element_size {
            return Err(PyValueError::new_err(format!(
                "Data size {} exceeds max element size {}",
                data.len(),
                self.max_element_size
            )));
        }
        let queue = self.get_queue();

        py.allow_threads(|| queue.blocking_write(data));
        Ok(())
    }
}

unsafe impl Sync for SharedMemoryCircularQueue {}

#[pymodule]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SharedMemoryWriter>()?;
    m.add_class::<SharedMemoryReader>()?;
    m.add_class::<SharedMemoryCircularQueue>()?;
    Ok(())
}
