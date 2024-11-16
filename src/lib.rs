use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::time::Duration;

use crate::circular_queue::CircularBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use crate::memory_holder::SharedMemoryHolder;
use crate::shared_memory::{ReadingResult, SharedMemoryMessage};
use crate::utils::pthread_lock::pthread_lock_initialize_at;
use crate::utils::pthread_rw_lock::pthread_rw_lock_initialize_at;

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
    last_written_version: Cell<usize>,
}

#[pymethods]
impl SharedMemoryWriter {
    #[new]
    fn new(name: String, size: u32) -> PyResult<Self> {
        if size == 0 {
            return Err(PyValueError::new_err("Size cannot be 0"));
        }
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;
        let shared_memory = SharedMemoryHolder::create(
            c_name,
            SharedMemoryMessage::size_of_fields() + size as usize,
        )?;
        unsafe { pthread_rw_lock_initialize_at(shared_memory.ptr().cast())? };

        Ok(Self {
            shared_memory,
            name,
            message_length: size as usize,
            last_written_version: Cell::new(0),
        })
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };

        let version_count = py.allow_threads(|| {
            shared_memory
                .write_message(data)
                .map_err(PyValueError::new_err)
        })?;
        self.last_written_version.set(version_count);

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
        self.last_written_version.get()
    }

    fn close(&self) -> PyResult<()> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let mut content = shared_memory.lock.write_lock()?;

        content.closed = true;

        Ok(())
    }
}

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
    last_version_read: Cell<usize>,
    buffer: RefCell<Vec<u8>>,
}

#[pymethods]
impl SharedMemoryReader {
    #[new]
    fn new(name: String) -> PyResult<Self> {
        let c_name = &CString::new(name.clone())?;
        let shared_memory = SharedMemoryHolder::open(c_name)?;

        // Read the message length
        let message = unsafe { &*(shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let message_length = message.lock.read_lock()?.data.len();

        Ok(Self {
            buffer: RefCell::new(vec![0u8; shared_memory.memory_size()]),
            name,
            message_length,
            shared_memory,
            last_version_read: Cell::new(0),
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
        let mut last_version = self.last_version_read.get();
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

        self.last_version_read.set(last_version);
        Ok(Some(
            PyBytes::new_bound(py, &buffer[..bytes_read]).into_py(py),
        ))
    }

    #[pyo3(signature = (ignore_same_version=true))]
    fn try_read(&self, py: Python<'_>, ignore_same_version: bool) -> PyResult<Option<PyObject>> {
        let mut last_version = self.last_version_read.get();

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

        self.last_version_read.set(last_version);
        Ok(Some(
            PyBytes::new_bound(py, &content.data[..bytes_read]).into_py(py),
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

        Ok(content.version != self.last_version_read.get())
    }

    fn last_read_version(&self) -> usize {
        self.last_version_read.get()
    }

    fn is_closed(&self) -> PyResult<bool> {
        let shared_memory =
            unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemoryMessage) };
        let content = shared_memory.lock.read_lock()?;

        Ok(content.closed)
    }
}

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    buffer: RefCell<Vec<u8>>,
}

#[pymethods]
impl SharedMemoryCircularQueue {
    #[staticmethod]
    fn create(
        name: String,
        max_element_size: usize,
        capacity: usize,
    ) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;
        if max_element_size == 0 || capacity == 0 {
            return Err(PyValueError::new_err("Size cannot be 0"));
        }
        let buffer_size = (size_of::<circular_queue::ElementSizeType>() + max_element_size) * capacity;
        let memory_holder =
            SharedMemoryHolder::create(c_name, CircularBuffer::size_of_fields() + buffer_size)?;

        unsafe { pthread_lock_initialize_at(memory_holder.ptr().cast())? };
        
        let queue = unsafe { &mut *(memory_holder.slice_ptr() as *mut CircularBuffer) };
        queue.init(max_element_size, capacity);

        Ok(Self {
            shared_memory: memory_holder,
            name,
            buffer: RefCell::new(vec![0u8; max_element_size]),
        })
    }
    
    #[staticmethod]
    fn open(name: String) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;
        let memory_holder = SharedMemoryHolder::open(c_name.as_c_str())?;
        
        let queue = unsafe { &mut *(memory_holder.slice_ptr() as *mut CircularBuffer) };

        Ok(Self {
            shared_memory: memory_holder,
            name,
            buffer: RefCell::new(vec![0u8; queue.max_element_size()]),
        })
    }

    fn __len__(&self) -> usize {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };
        queue.len()
    }

    fn is_full(&self) -> bool {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };
        queue.is_full()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn try_read(&self, py: Python<'_>) -> Option<PyObject> {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        if queue.try_read(buffer) {
            Some(PyBytes::new_bound(py, buffer).into_py(py))
        } else {
            None
        }
    }

    fn blocking_read(&self, py: Python<'_>) -> PyObject {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        py.allow_threads(|| queue.blocking_read(buffer));

        PyBytes::new_bound(py, buffer).into_py(py)
    }
    
    fn read_all(&self) -> Vec<Vec<u8>> {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };
        
        queue.read_all()
    }

    fn try_write(&self, data: &[u8]) -> bool {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        queue.try_write(data)
    }

    fn blocking_write(&self, py: Python<'_>, data: &[u8]) {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        py.allow_threads(|| queue.blocking_write(data));
    }
}

#[pymodule]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SharedMemoryWriter>()?;
    m.add_class::<SharedMemoryReader>()?;
    m.add_class::<SharedMemoryCircularQueue>()?;
    Ok(())
}
