use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::python::{no_read_permission_err, no_write_permission_err, OpenMode};
use crate::shared_memory::SharedMemory;
use pyo3::exceptions::PyValueError;
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
#[pyo3(frozen, name = "SharedMemory")]
pub struct PythonSharedMemory {
    shared_memory: SharedMemoryHolder,
    name: String,
    memory_size: usize,
    open_mode: OpenMode,
    last_written_version: AtomicUsize,
    last_read_version: AtomicUsize,
}

fn deref_shared_memory(shared_memory: &SharedMemoryHolder) -> &SharedMemory {
    unsafe { &*(shared_memory.slice_ptr() as *const SharedMemory) }
}

#[pymethods]
impl PythonSharedMemory {
    #[staticmethod]
    #[pyo3(signature = (name, size, mode=OpenMode::ReadWrite))]
    fn create(name: String, size: NonZeroU32, mode: OpenMode) -> PyResult<Self> {
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
            open_mode: mode,
        })
    }

    #[staticmethod]
    #[pyo3(signature = (name, mode=OpenMode::ReadWrite))]
    fn open(name: String, mode: OpenMode) -> PyResult<Self> {
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
            open_mode: mode,
        })
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        if !self.open_mode.can_write() {
            no_write_permission_err();
        }

        if data.len() > self.memory_size {
            return Err(PyValueError::new_err(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                self.memory_size,
                data.len()
            )));
        }

        let shared_memory = deref_shared_memory(&self.shared_memory);
        let version = py.allow_threads(|| shared_memory.write_message(data));
        self.last_written_version.store(version, Ordering::Relaxed);

        Ok(())
    }

    fn try_read<'p>(&self, py: Python<'p>) -> PyResult<Option<Bound<'p, PyBytes>>> {
        if !self.open_mode.can_read() {
            no_read_permission_err();
        }
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

    fn blocking_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        if !self.open_mode.can_read() {
            no_read_permission_err();
        }
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

    fn is_new_version_available(&self) -> bool {
        if !self.open_mode.can_read() {
            no_read_permission_err();
        }
        
        let last_read_version = self.last_read_version.load(Ordering::Relaxed);
        let version = deref_shared_memory(&self.shared_memory)
            .version
            .load(Ordering::Relaxed);
        version != last_read_version
    }

    fn last_written_version(&self) -> usize {
        self.last_written_version.load(Ordering::Relaxed)
    }

    fn last_read_version(&self) -> usize {
        self.last_read_version.load(Ordering::Relaxed)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn memory_size(&self) -> usize {
        self.memory_size
    }

    fn is_closed(&self) -> bool {
        deref_shared_memory(&self.shared_memory)
            .closed
            .load(Ordering::Relaxed)
    }

    fn close(&self) {
        if !self.open_mode.can_write() {
            no_write_permission_err();
        }
        let memory = deref_shared_memory(&self.shared_memory);

        let _ = memory.data.lock();
        memory.closed.store(true, Ordering::Relaxed);
        memory.condvar.notify_all();
    }
}

unsafe impl Sync for PythonSharedMemory {}
