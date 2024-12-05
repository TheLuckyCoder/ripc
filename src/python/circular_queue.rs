use std::cell::RefCell;
use std::ffi::CString;
use std::num::NonZeroU32;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use pyo3::exceptions::PyValueError;
use pyo3::types::PyBytes;
use crate::circular_queue::CircularQueue;
use crate::primitives::memory_holder::SharedMemoryHolder;

#[pyclass]
#[pyo3(frozen)]
pub struct SharedCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    max_element_size: usize,
    buffer: RefCell<Vec<u8>>,
}

impl SharedCircularQueue {
    fn get_queue(&self) -> &CircularQueue {
        unsafe { &*(self.shared_memory.slice_ptr() as *const CircularQueue) }
    }
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

    fn try_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        let length = self.get_queue().try_read(buffer);
        if length != 0 {
            Some(PyBytes::new(py, &buffer[..length]))
        } else {
            None
        }
    }

    fn blocking_read<'p>(&self, py: Python<'p>) -> Bound<'p, PyBytes> {
        let queue = self.get_queue();

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();
        let length = py.allow_threads(|| queue.blocking_read(buffer));

        PyBytes::new(py, &buffer[..length])
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

unsafe impl Sync for SharedCircularQueue {}
