use crate::circular_queue::CircularQueue;
use crate::primitives::memory_holder::SharedMemoryHolder;
use pyo3::exceptions::{PyPermissionError, PyValueError};
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use std::cell::RefCell;
use std::ffi::CString;
use std::num::NonZeroU32;

#[pyclass]
#[pyo3(frozen, name = "SharedMemoryCircularQueue")]
pub struct SharedCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    max_element_size: usize,
    buffer: RefCell<Vec<u8>>,
    read_only: bool,
}

fn deref_queue(memory_holder: &SharedMemoryHolder) -> &CircularQueue {
    unsafe { &*(memory_holder.slice_ptr() as *const CircularQueue) }
}

#[pymethods]
impl SharedCircularQueue {
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

        deref_queue(&shared_memory).init(max_element_size, capacity);

        Ok(Self {
            shared_memory,
            name,
            max_element_size,
            buffer: RefCell::new(vec![0u8; max_element_size]),
            read_only: false,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (name, read_only=false))]
    fn open(name: String, read_only: bool) -> PyResult<Self> {
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
            read_only,
        })
    }

    fn __len__(&self) -> usize {
        deref_queue(&self.shared_memory).len()
    }

    fn is_full(&self) -> bool {
        deref_queue(&self.shared_memory).is_full()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn try_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        let length = deref_queue(&self.shared_memory).try_read(buffer);
        if length != 0 {
            Some(PyBytes::new(py, &buffer[..length]))
        } else {
            None
        }
    }

    fn blocking_read<'p>(&self, py: Python<'p>) -> Bound<'p, PyBytes> {
        let queue = deref_queue(&self.shared_memory);

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();
        let length = py.allow_threads(|| queue.blocking_read(buffer));

        PyBytes::new(py, &buffer[..length])
    }

    fn read_all(&self) -> Vec<Vec<u8>> {
        let mut borrowed_buffer = self.buffer.borrow_mut();
        deref_queue(&self.shared_memory).read_all(borrowed_buffer.as_mut_slice())
    }

    fn try_write(&self, data: &[u8]) -> PyResult<bool> {
        if self.read_only {
            return Err(PyPermissionError::new_err("Queue was opened as read-only"));
        }
        if data.len() > self.max_element_size {
            return Err(PyValueError::new_err(format!(
                "Data size {} exceeds max element size {}",
                data.len(),
                self.max_element_size
            )));
        }
        Ok(deref_queue(&self.shared_memory).try_write(data))
    }

    fn blocking_write(&self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        if self.read_only {
            return Err(PyPermissionError::new_err("Queue was opened as read-only"));
        }
        if data.len() > self.max_element_size {
            return Err(PyValueError::new_err(format!(
                "Data size {} exceeds max element size {}",
                data.len(),
                self.max_element_size
            )));
        }
        let queue = deref_queue(&self.shared_memory);

        py.allow_threads(|| queue.blocking_write(data));
        Ok(())
    }
}

unsafe impl Sync for SharedCircularQueue {}
