use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::python::writer::deref_shared_memory;
use pyo3::exceptions::PyValueError;
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use std::ffi::CString;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
#[pyo3(frozen, name = "SharedMemoryReader")]
pub struct SharedReader {
    shared_memory: SharedMemoryHolder,
    name: String,
    memory_size: usize,
    last_version_read: AtomicUsize,
}

#[pymethods]
impl SharedReader {
    #[new]
    fn new(name: String) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let shared_memory = SharedMemoryHolder::open(CString::new(name.clone())?)?;

        let memory = deref_shared_memory(&shared_memory);
        let memory_size = memory.data.lock().bytes.len();

        Ok(Self {
            name,
            memory_size,
            shared_memory,
            last_version_read: AtomicUsize::default(),
        })
    }

    fn try_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        let last_read_version = self.last_version_read.load(Ordering::Relaxed);

        let memory = deref_shared_memory(&self.shared_memory);

        if memory.closed.load(Ordering::Relaxed) {
            return None;
        }

        let new_version = memory.version.load(Ordering::Relaxed);
        if new_version != last_read_version {
            let data_guard = memory.data.lock();

            self.last_version_read.store(new_version, Ordering::Relaxed);

            return Some(PyBytes::new(py, &data_guard.bytes[..data_guard.size]));
        }

        None
    }

    fn blocking_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        let last_read_version = self.last_version_read.load(Ordering::Relaxed);
        let memory = deref_shared_memory(&self.shared_memory);

        let mut data = memory.data.lock();
        loop {
            if memory.closed.load(Ordering::Relaxed) {
                return None;
            }

            let new_version = memory.version.load(Ordering::Relaxed);
            if new_version != last_read_version {
                self.last_version_read.store(new_version, Ordering::Relaxed);

                return Some(PyBytes::new(py, &data.bytes[..data.size]));
            }

            data = memory.condvar.wait(data);
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn memory_size(&self) -> usize {
        self.memory_size
    }

    fn new_version_available(&self) -> bool {
        let last_read_version = self.last_version_read.load(Ordering::Relaxed);
        let version = deref_shared_memory(&self.shared_memory)
            .version
            .load(Ordering::Relaxed);
        version != last_read_version
    }

    fn last_read_version(&self) -> usize {
        self.last_version_read.load(Ordering::Relaxed)
    }

    fn is_closed(&self) -> bool {
        deref_shared_memory(&self.shared_memory)
            .closed
            .load(Ordering::Relaxed)
    }
}

unsafe impl Sync for SharedReader {}
