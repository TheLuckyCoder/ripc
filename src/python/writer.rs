use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::shared_memory::SharedMemory;
use pyo3::exceptions::PyValueError;
use pyo3::{pyclass, pymethods, PyResult, Python};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
#[pyo3(frozen, name = "SharedMemoryWriter")]
pub struct SharedWriter {
    shared_memory: SharedMemoryHolder,
    name: String,
    memory_size: usize,
    last_written_version: AtomicUsize,
}

pub(crate) fn deref_shared_memory(shared_memory: &SharedMemoryHolder) -> &SharedMemory {
    unsafe { &*(shared_memory.slice_ptr() as *const SharedMemory) }
}

#[pymethods]
impl SharedWriter {
    #[new]
    pub fn new(name: String, size: NonZeroU32) -> PyResult<Self> {
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
        })
    }

    pub fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        let shared_memory = deref_shared_memory(&self.shared_memory);

        if data.len() > self.memory_size {
            return Err(PyValueError::new_err(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                self.memory_size,
                data.len()
            )));
        }

        let version_count = py.allow_threads(|| shared_memory.write_message(data));
        self.last_written_version
            .store(version_count, Ordering::Relaxed);

        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn memory_size(&self) -> usize {
        self.memory_size
    }

    pub fn last_written_version(&self) -> usize {
        self.last_written_version.load(Ordering::Relaxed)
    }

    pub fn close(&self) -> PyResult<()> {
        let memory = deref_shared_memory(&self.shared_memory);

        let _ = memory.data.lock();
        memory.closed.store(true, Ordering::Relaxed);
        memory.condvar.notify_all();

        Ok(())
    }
}

unsafe impl Sync for SharedWriter {}

impl Drop for SharedWriter {
    fn drop(&mut self) {
        self.close().unwrap();
    }
}
