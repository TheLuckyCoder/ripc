use crate::circular_queue::CircularQueue;
use crate::shared_memory::SharedMemory;
use primitives::memory_holder::SharedMemoryHolder;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::cell::RefCell;
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};

mod circular_queue;
mod primitives;
mod shared_memory;

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryWriter {
    shared_memory: SharedMemoryHolder,
    name: String,
    memory_size: usize,
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

        let shared_memory = SharedMemoryHolder::create(
            c_name,
            SharedMemory::size_of_fields() + size.get() as usize,
        )?;

        let memory = unsafe { &*(shared_memory.slice_ptr() as *mut SharedMemory) };
        let memory_size = memory.data.lock()?.data.len();

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
    memory_size: usize,
    last_version_read: AtomicUsize,
}

impl SharedMemoryReader {
    fn get_memory(&self) -> &SharedMemory {
        unsafe { &*(self.shared_memory.slice_ptr() as *const SharedMemory) }
    }
}

#[pymethods]
impl SharedMemoryReader {
    #[new]
    fn new(name: String) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let shared_memory = SharedMemoryHolder::open(CString::new(name.clone())?)?;

        let memory = unsafe { &*(shared_memory.slice_ptr() as *mut SharedMemory) };
        let memory_size = memory.data.lock()?.data.len();

        Ok(Self {
            name,
            memory_size,
            shared_memory,
            last_version_read: AtomicUsize::default(),
        })
    }

    fn try_read<'p>(&self, py: Python<'p>) -> PyResult<Option<Bound<'p, PyBytes>>> {
        let last_read_version = self.last_version_read.load(Ordering::Relaxed);

        let memory = self.get_memory();

        if memory.closed.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let new_version = memory.version.load(Ordering::Relaxed);
        if new_version != last_read_version {
            let data_guard = memory.data.lock()?;

            self.last_version_read.store(new_version, Ordering::Relaxed);

            return Ok(Some(PyBytes::new(
                py,
                &data_guard.data[..data_guard.message_size],
            )));
        }

        Ok(None)
    }

    fn blocking_read<'p>(&self, py: Python<'p>) -> PyResult<Option<Bound<'p, PyBytes>>> {
        let last_read_version = self.last_version_read.load(Ordering::Relaxed);
        let memory = self.get_memory();

        let mut data = memory.data.lock()?;
        loop {
            if memory.closed.load(Ordering::Relaxed) {
                return Ok(None);
            }

            if memory.version.load(Ordering::Relaxed) != last_read_version {
                self.last_version_read
                    .store(last_read_version, Ordering::Relaxed);

                return Ok(Some(PyBytes::new(py, &data.data[..data.message_size])));
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
        let version = self.get_memory().version.load(Ordering::Relaxed);
        version != last_read_version
    }

    fn last_read_version(&self) -> usize {
        self.last_version_read.load(Ordering::Relaxed)
    }

    fn is_closed(&self) -> bool {
        self.get_memory().closed.load(Ordering::Relaxed)
    }
}

unsafe impl Sync for SharedMemoryReader {}

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    max_element_size: usize,
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

unsafe impl Sync for SharedMemoryCircularQueue {}

#[pymodule]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SharedMemoryWriter>()?;
    m.add_class::<SharedMemoryReader>()?;
    m.add_class::<SharedMemoryCircularQueue>()?;
    Ok(())
}
