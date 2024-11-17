use std::cell::RefCell;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::circular_queue::CircularQueue;
use crate::memory_holder::SharedMemoryHolder;
use crate::shared_memory::{ReadingResult, SharedMemoryMessage};
use crate::utils::pthread_lock::pthread_lock_initialize_at;
use crate::utils::pthread_rw_lock::pthread_rw_lock_initialize_at;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

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

    #[pyo3(signature = (micros_between_reads=1000))]
    fn blocking_read(
        &self,
        py: Python<'_>,
        micros_between_reads: u64,
    ) -> PyResult<Option<PyObject>> {
        let sleep_duration = Duration::from_micros(if micros_between_reads == 0 {
            1
        } else {
            micros_between_reads
        });
        let mut last_version = self.last_version_read.load(Ordering::Relaxed);
        let mut buffer = self.buffer.borrow_mut();

        let bytes_read: usize = {
            let message = unsafe { &*(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
            let buffer = buffer.as_mut_slice();

            py.allow_threads(|| loop {
                match message.read_message(&mut last_version, buffer) {
                    ReadingResult::MessageSize(size) => break Ok(size),
                    ReadingResult::SameVersion => {}
                    ReadingResult::Closed => break Ok(0),
                    ReadingResult::FailedCreatingLock(e) => break Err(e),
                }

                std::thread::sleep(sleep_duration);
            })?
        };

        if bytes_read == 0 {
            return Ok(None);
        }

        self.last_version_read
            .store(last_version, Ordering::Relaxed);
        Ok(Some(PyBytes::new(py, &buffer[..bytes_read]).into_py(py)))
    }

    #[pyo3(signature = (ignore_same_version=true))]
    fn try_read(&self, py: Python<'_>, ignore_same_version: bool) -> PyResult<Option<PyObject>> {
        let mut last_version = self.last_version_read.load(Ordering::Relaxed);

        let message = unsafe { &*(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };

        let content = message.lock.read_lock()?;

        let bytes_read = {
            let message_version = content.version;
            if ignore_same_version && message_version == last_version {
                0 // There is no new data
            } else {
                last_version = message_version;
                content.message_size
            }
        };

        if bytes_read == 0 {
            return Ok(None);
        }

        self.last_version_read
            .store(last_version, Ordering::Relaxed);
        Ok(Some(
            PyBytes::new(py, &content.data[..bytes_read]).into_py(py),
        ))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> usize {
        self.message_length
    }

    fn check_message_available(&self) -> PyResult<bool> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let content = shared_memory.lock.read_lock()?;

        Ok(content.version != self.last_version_read.load(Ordering::Relaxed))
    }

    fn last_read_version(&self) -> usize {
        self.last_version_read.load(Ordering::Relaxed)
    }

    fn is_closed(&self) -> PyResult<bool> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let content = shared_memory.lock.read_lock()?;

        Ok(content.closed)
    }
}

unsafe impl Sync for SharedMemoryReader {}

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
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

        let c_name = CString::new(name.clone())?;
        if max_element_size == 0 || capacity == 0 {
            return Err(PyValueError::new_err("Size cannot be 0"));
        }
        let shared_memory = SharedMemoryHolder::create(
            c_name,
            CircularQueue::compute_size_for(max_element_size, capacity),
        )?;

        unsafe { pthread_lock_initialize_at(shared_memory.ptr().cast())? };

        let queue = unsafe { &mut *(shared_memory.slice_ptr() as *mut CircularQueue) };
        queue.init(max_element_size, capacity);

        Ok(Self {
            shared_memory,
            name,
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

        Ok(Self {
            shared_memory,
            name,
            buffer: RefCell::new(vec![0u8; queue.max_element_size()]),
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

        py.allow_threads(|| queue.blocking_read(buffer));

        PyBytes::new(py, buffer).into_py(py)
    }

    fn read_all(&self) -> Vec<Vec<u8>> {
        let mut borrowed_buffer = self.buffer.borrow_mut();
        self.get_queue().read_all(borrowed_buffer.as_mut_slice())
    }

    fn try_write(&self, data: &[u8]) -> bool {
        self.get_queue().try_write(data)
    }

    fn blocking_write(&self, py: Python<'_>, data: &[u8]) {
        let queue = self.get_queue();

        py.allow_threads(|| queue.blocking_write(data));
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
