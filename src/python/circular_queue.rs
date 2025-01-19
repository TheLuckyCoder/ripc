use crate::container::circular_queue::CircularQueue;
use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::python::OpenMode;
use pyo3::exceptions::PyValueError;
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, Py, PyResult, Python};
use std::ffi::CString;
use std::num::NonZeroU32;

#[pyclass]
#[pyo3(frozen, name = "SharedMemoryCircularQueue")]
pub struct SharedCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    max_element_size: usize,
    open_mode: OpenMode,
}

fn deref_queue(memory_holder: &SharedMemoryHolder) -> &CircularQueue {
    unsafe { &*(memory_holder.slice_ptr() as *const CircularQueue) }
}

#[pymethods]
impl SharedCircularQueue {
    #[staticmethod]
    #[pyo3(signature = (name, max_element_size, capacity, mode=OpenMode::ReadWrite))]
    fn create(
        name: String,
        max_element_size: NonZeroU32,
        capacity: NonZeroU32,
        mode: OpenMode,
    ) -> PyResult<Self> {
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
            open_mode: mode,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (name, mode=OpenMode::ReadWrite))]
    fn open(name: String, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let shared_memory = SharedMemoryHolder::open(CString::new(name.clone())?)?;

        let queue = deref_queue(&shared_memory);
        let max_element_size = queue.max_element_size();

        Ok(Self {
            shared_memory,
            name,
            max_element_size,
            open_mode: mode,
        })
    }

    fn __len__(&self) -> usize {
        deref_queue(&self.shared_memory).len()
    }

    fn is_full(&self) -> bool {
        deref_queue(&self.shared_memory).is_full()
    }

    fn try_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        self.open_mode.check_read_permission();

        let mut result = None;
        deref_queue(&self.shared_memory).try_read(|data| {
            result = Some(PyBytes::new(py, data));
        });

        result
    }

    fn blocking_read(&self, py: Python<'_>) -> Option<Py<PyBytes>> {
        self.open_mode.check_read_permission();

        let queue = deref_queue(&self.shared_memory);
        let mut result = None;

        py.allow_threads(|| {
            queue.blocking_read(|data| {
                Python::with_gil(|py| {
                    result = Some(PyBytes::new(py, data).unbind());
                })
            })
        });

        result
    }

    fn try_write(&self, data: &[u8]) -> PyResult<bool> {
        self.open_mode.check_write_permission();

        if data.len() > self.max_element_size {
            return Err(PyValueError::new_err(format!(
                "Data size {} exceeds max element size {}",
                data.len(),
                self.max_element_size
            )));
        }
        Ok(deref_queue(&self.shared_memory).try_write(data))
    }

    fn blocking_write(&self, py: Python<'_>, data: &[u8]) -> PyResult<bool> {
        self.open_mode.check_write_permission();

        if data.len() > self.max_element_size {
            return Err(PyValueError::new_err(format!(
                "Data size {} exceeds max element size {}",
                data.len(),
                self.max_element_size
            )));
        }
        let queue = deref_queue(&self.shared_memory);

        Ok(py.allow_threads(|| queue.blocking_write(data)))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn memory_size(&self) -> usize {
        self.shared_memory.slice_ptr().len()
    }

    fn is_closed(&self) -> bool {
        deref_queue(&self.shared_memory).is_closed()
    }

    fn close(&self) {
        self.open_mode.check_write_permission();
        deref_queue(&self.shared_memory).close()
    }
}
