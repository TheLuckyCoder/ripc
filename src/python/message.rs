use crate::container::shared_memory::SharedMemory;
use crate::helpers::queue_data::QueueData;
use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::python::OpenMode;
use pyo3::exceptions::PyValueError;
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

#[pyclass]
#[pyo3(frozen, name = "SharedMessage")]
pub struct PythonSharedMessage {
    shared_memory: Arc<SharedMemoryHolder>,
    name: String,
    memory_size: usize,
    open_mode: OpenMode,
    last_written_version: Arc<AtomicUsize>,
    last_read_version: AtomicUsize,
    sender: Mutex<Option<Sender<QueueData>>>,
}

fn deref_shared_memory(shared_memory: &SharedMemoryHolder) -> &SharedMemory {
    unsafe { &*(shared_memory.slice_ptr() as *const SharedMemory) }
}

impl PythonSharedMessage {
    pub fn new(
        shared_memory: SharedMemoryHolder,
        name: String,
        memory_size: usize,
        open_mode: OpenMode,
    ) -> Self {
        Self {
            shared_memory: Arc::new(shared_memory),
            name,
            memory_size,
            open_mode,
            last_written_version: Arc::default(),
            last_read_version: AtomicUsize::default(),
            sender: Mutex::default(),
        }
    }
}

#[pymethods]
impl PythonSharedMessage {
    #[staticmethod]
    #[pyo3(signature = (name, size, mode=OpenMode::ReadWrite))]
    pub fn create(name: String, size: NonZeroU32, mode: OpenMode) -> PyResult<Self> {
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

        Ok(Self::new(shared_memory, name, memory_size, mode))
    }

    #[staticmethod]
    #[pyo3(signature = (name, mode=OpenMode::ReadWrite))]
    pub fn open(name: String, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let shared_memory = SharedMemoryHolder::open(CString::new(name.clone())?)?;

        let memory = deref_shared_memory(&shared_memory);
        let memory_size = memory.data.lock().bytes.len();

        Ok(Self::new(shared_memory, name, memory_size, mode))
    }

    pub fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        self.open_mode.check_write_permission();

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

    pub fn write_async(&self, data: Bound<'_, PyBytes>) -> PyResult<()> {
        self.open_mode.check_write_permission();
        let queue_data = QueueData::new(data);

        if queue_data.bytes().len() > self.memory_size {
            return Err(PyValueError::new_err(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                self.memory_size,
                queue_data.bytes().len()
            )));
        }

        let mut guard = self.sender.lock().unwrap();
        let sender = guard.get_or_insert_with(|| {
            let (sender, receiver) = channel::<QueueData>();

            let last_written_version = self.last_written_version.clone();
            let shared_memory = self.shared_memory.clone();
            std::thread::spawn(move || {
                let memory = deref_shared_memory(&shared_memory);

                loop {
                    let Ok(data) = receiver.recv() else {
                        break;
                    };
                    if memory.closed.load(Ordering::Relaxed) {
                        break;
                    }
                    let new_version = memory.write_message(data.bytes());

                    last_written_version.store(new_version, Ordering::Relaxed);
                }
            });

            sender
        });

        sender.send(queue_data).map_err(|_| {
            PyValueError::new_err("Failed to send data, the queue has been closed".to_string())
        })
    }

    pub fn try_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        self.open_mode.check_read_permission();
        let last_read_version = self.last_read_version.load(Ordering::Relaxed);
        let memory = deref_shared_memory(&self.shared_memory);

        if memory.closed.load(Ordering::Relaxed) {
            return None;
        }

        let new_version = memory.version.load(Ordering::Relaxed);
        if new_version == last_read_version {
            return None;
        }
        let data_guard = memory.data.lock();

        self.last_read_version.store(new_version, Ordering::Relaxed);

        Some(PyBytes::new(py, &data_guard.bytes[..data_guard.size]))
    }

    pub fn blocking_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        self.open_mode.check_read_permission();

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
        self.open_mode.check_read_permission();

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

    pub fn close(&self) {
        self.open_mode.check_write_permission();
        let memory = deref_shared_memory(&self.shared_memory);

        let _ = memory.data.lock();
        memory.closed.store(true, Ordering::Relaxed);
        memory.condvar.notify_all();
    }
}
