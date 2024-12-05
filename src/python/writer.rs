use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use pyo3::{pyclass, pymethods, PyResult, Python};
use pyo3::exceptions::PyValueError;
use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::shared_memory::SharedMemory;

#[pyclass]
#[pyo3(frozen)]
pub struct SharedWriter {
    shared_memory: SharedMemoryHolder,
    name: String,
    memory_size: usize,
    last_written_version: AtomicUsize,
}

#[pymethods]
impl SharedWriter {
    #[new]
    fn new(name: String, size: NonZeroU32) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;

        let shared_memory = SharedMemoryHolder::create(
            c_name,
            SharedMemory::size_of_fields() + size.get() as usize,
        )?;

        let memory = unsafe { &*(shared_memory.slice_ptr() as *mut SharedMemory) };
        let memory_size = memory.data.lock()?.bytes.len();

        Ok(Self {
            shared_memory,
            name,
            memory_size,
            last_written_version: AtomicUsize::default(),
        })
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        let shared_memory = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemory) };

        let version_count = py.allow_threads(|| shared_memory.write_message(data))?;
        self.last_written_version
            .store(version_count, Ordering::Relaxed);

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn memory_size(&self) -> usize {
        self.memory_size
    }

    fn last_written_version(&self) -> usize {
        self.last_written_version.load(Ordering::Relaxed)
    }

    fn close(&self) -> PyResult<()> {
        let memory = unsafe { &*(self.shared_memory.slice_ptr() as *const SharedMemory) };

        let _ = memory.data.lock()?;
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


