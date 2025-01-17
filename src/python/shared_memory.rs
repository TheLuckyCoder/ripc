use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::shared_memory::SharedMemory;
use pyo3::exceptions::{PyPermissionError, PyValueError};
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::python::{no_read_permission_err, no_write_permission_err, MemoryPermission};

#[pyclass]
#[pyo3(frozen, name = "SharedMemory")]
pub struct PythonSharedMemory {
    shared_memory: SharedMemoryHolder,
    name: String,
    memory_size: usize,
    read_only: bool,
    last_written_version: AtomicUsize,
    last_read_version: AtomicUsize,
}

fn deref_shared_memory(shared_memory: &SharedMemoryHolder) -> &SharedMemory {
    unsafe { &*(shared_memory.slice_ptr() as *const SharedMemory) }
}

#[pymethods]
impl PythonSharedMemory {
    #[staticmethod]
    pub fn create(name: String, size: NonZeroU32) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;

        let shared_memory = SharedMemoryHolder::create(
            c_name,
            SharedMemory::size_of_fields() + size.get() as usize,
        )?;

        let memory = deref_shared_memory(&shared_memory);
        let memory_size = memory.data.lock().bytes.len();

        Ok(Self {
            shared_memory,
            name,
            memory_size,
            last_written_version: AtomicUsize::default(),
            last_read_version: AtomicUsize::default(),
            read_only: false,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (name, read_only=false))]
    pub fn open(name: String, read_only: bool) -> PyResult<Self> {
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
            last_written_version: AtomicUsize::default(),
            last_read_version: AtomicUsize::default(),
            read_only,
        })
    }

    pub fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        // if !self.permissions.can_write() {
        //     return Err(no_write_permission_err());
        // }
        let shared_memory = deref_shared_memory(&self.shared_memory);

        if data.len() > self.memory_size {
            return Err(PyValueError::new_err(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                self.memory_size,
                data.len()
            )));
        }

        let version = py.allow_threads(|| shared_memory.write_message(data));
        self.last_written_version.store(version, Ordering::Relaxed);

        Ok(())
    }

    pub fn try_read<'p>(&self, py: Python<'p>) -> PyResult<Option<Bound<'p, PyBytes>>> {
        // if !self.permissions.can_read() {
        //     return Err(no_read_permission_err());
        // }
        let last_read_version = self.last_read_version.load(Ordering::Relaxed);
        let memory = deref_shared_memory(&self.shared_memory);

        if memory.closed.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let new_version = memory.version.load(Ordering::Relaxed);
        if new_version != last_read_version {
            let data_guard = memory.data.lock();

            self.last_read_version.store(new_version, Ordering::Relaxed);

            return Ok(Some(PyBytes::new(py, &data_guard.bytes[..data_guard.size])));
        }

        Ok(None)
    }

    pub fn blocking_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        // if !self.permissions.can_read() {
        //     return Err(no_read_permission_err());
        // }
        let last_read_version = self.last_read_version.load(Ordering::Relaxed);
        let memory = deref_shared_memory(&self.shared_memory);

        let mut data = memory.data.lock();
        loop {
            if memory.closed.load(Ordering::Relaxed) {
                return None;
            }

            let new_version = memory.version.load(Ordering::Relaxed);
            if new_version != last_read_version {
                self.last_read_version.store(new_version, Ordering::Relaxed);

                return Some(PyBytes::new(py, &data.bytes[..data.size]));
            }

            // Wait for new version
            data = py.allow_threads(|| memory.condvar.wait(data));
        }
    }

    pub fn is_new_version_available(&self) -> bool {
        let last_read_version = self.last_read_version.load(Ordering::Relaxed);
        let version = deref_shared_memory(&self.shared_memory)
            .version
            .load(Ordering::Relaxed);
        version != last_read_version
    }

    pub fn last_written_version(&self) -> usize {
        self.last_written_version.load(Ordering::Relaxed)
    }

    pub fn last_read_version(&self) -> usize {
        self.last_read_version.load(Ordering::Relaxed)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn memory_size(&self) -> usize {
        self.memory_size
    }

    pub fn is_closed(&self) -> bool {
        deref_shared_memory(&self.shared_memory)
            .closed
            .load(Ordering::Relaxed)
    }

    pub fn close(&self) -> PyResult<()> {
        if self.read_only {
            return Err(PyPermissionError::new_err(
                "Shared memory was opened as read-only",
            ));
        }
        let memory = deref_shared_memory(&self.shared_memory);

        let _ = memory.data.lock();
        memory.closed.store(true, Ordering::Relaxed);
        memory.condvar.notify_all();

        Ok(())
    }
}

unsafe impl Sync for PythonSharedMemory {}
