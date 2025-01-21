use crate::container::circular_queue::CircularQueue;
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
#[pyo3(frozen, name = "SharedQueue")]
pub struct PythonSharedQueue {
    shared_memory: Arc<SharedMemoryHolder>,
    sender: Mutex<Option<Sender<QueueData>>>,
    name: String,
    open_mode: OpenMode,
    write_count: Arc<AtomicUsize>,
    read_count: AtomicUsize,
}
fn deref_queue(memory_holder: &SharedMemoryHolder) -> &CircularQueue {
    unsafe { &*(memory_holder.slice_ptr() as *const CircularQueue) }
}

#[pymethods]
impl PythonSharedQueue {
    #[staticmethod]
    #[pyo3(signature = (name, max_element_size, mode, buffer_size = NonZeroU32::new(8).unwrap()))]
    fn create(
        name: String,
        max_element_size: NonZeroU32,
        mode: OpenMode,
        buffer_size: NonZeroU32,
    ) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let max_element_size = max_element_size.get() as usize;
        let buffer_size = buffer_size.get() as usize;

        let shared_memory = Arc::new(SharedMemoryHolder::create(
            CString::new(name.clone())?,
            CircularQueue::compute_size_for(max_element_size, buffer_size),
        )?);

        deref_queue(&shared_memory).init(max_element_size, buffer_size);

        Ok(Self {
            shared_memory,
            name,
            sender: Mutex::default(),
            open_mode: mode,
            write_count: Arc::default(),
            read_count: AtomicUsize::default(),
        })
    }

    #[staticmethod]
    fn open(name: String, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let shared_memory = Arc::new(SharedMemoryHolder::open(CString::new(name.clone())?)?);

        Ok(Self {
            name,
            shared_memory,
            sender: Mutex::default(),
            open_mode: mode,
            write_count: Arc::default(),
            read_count: AtomicUsize::default(),
        })
    }

    fn write(&self, data: Bound<'_, PyBytes>) -> PyResult<()> {
        self.open_mode.check_write_permission();
        let queue_data = QueueData::new(data);

        let mut guard = self.sender.lock().unwrap();
        let sender = guard.get_or_insert_with(|| {
            let (sender, receiver) = channel::<QueueData>();

            let write_count = self.write_count.clone();
            let shared_memory = self.shared_memory.clone();
            std::thread::spawn(move || {
                let queue = deref_queue(&shared_memory);

                loop {
                    let Ok(data) = receiver.recv() else {
                        break;
                    };
                    if !queue.blocking_write(data.bytes()) {
                        break;
                    }

                    write_count.fetch_add(1, Ordering::Relaxed);
                }
            });

            sender
        });

        sender.send(queue_data).map_err(|_| {
            PyValueError::new_err("Failed to send data, the queue has been closed".to_string())
        })
    }

    fn try_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        self.open_mode.check_read_permission();

        let mut result = None;
        deref_queue(&self.shared_memory).try_read(|data| {
            result = Some(PyBytes::new(py, data));
        });

        if result.is_some() {
            self.read_count.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    fn blocking_read<'p>(&self, py: Python<'p>) -> Option<Bound<'p, PyBytes>> {
        self.open_mode.check_read_permission();

        let mut result = None;
        deref_queue(&self.shared_memory).blocking_read(|data| {
            result = Some(PyBytes::new(py, data));
        });

        if result.is_some() {
            self.read_count.fetch_add(1, Ordering::Relaxed);
        }

        result
    }
    
    fn is_empty(&self) -> bool {
        deref_queue(&self.shared_memory).len() == 0
    }

    #[getter]
    fn read_count(&self) -> usize {
        self.read_count.load(Ordering::Relaxed)
    }

    #[getter]
    fn write_count(&self) -> usize {
        self.write_count.load(Ordering::Relaxed)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn memory_size(&self) -> usize {
        self.shared_memory.slice_ptr().len()
    }

    fn is_closed(&self) -> bool {
        deref_queue(&self.shared_memory).is_closed()
    }

    fn close(&self) {
        self.open_mode.check_write_permission();

        deref_queue(&self.shared_memory).close();
    }
}
