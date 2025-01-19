use crate::circular_queue::CircularQueue;
use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::python::OpenMode;
use pyo3::exceptions::PyValueError;
use pyo3::{pyclass, pymethods, PyResult};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

#[pyclass]
#[pyo3(frozen, name = "SharedMemoryQueue")]
pub struct SharedQueue {
    shared_memory: Arc<SharedMemoryHolder>,
    sender: Mutex<Option<Sender<Vec<u8>>>>,
    name: String,
    max_element_size: usize,
    open_mode: OpenMode,
}
fn deref_queue(memory_holder: &SharedMemoryHolder) -> &CircularQueue {
    unsafe { &*(memory_holder.slice_ptr() as *const CircularQueue) }
}

#[pymethods]
impl SharedQueue {
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
            max_element_size,
            sender: Mutex::default(),
            open_mode: mode,
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
            max_element_size: deref_queue(&shared_memory).max_element_size(),
            shared_memory,
            sender: Mutex::default(),
            open_mode: mode,
        })
    }

    fn write(&self, data: Vec<u8>) -> PyResult<()> {
        self.open_mode.check_write_permission();
        
        let mut guard = self.sender.lock().unwrap();
        let sender = guard.get_or_insert_with(|| {
            let (sender, receiver) = channel::<Vec<u8>>();

            let shared_memory_copy = self.shared_memory.clone();
            std::thread::spawn(move || {
                let shared_memory = shared_memory_copy;
                let queue = deref_queue(&shared_memory);

                loop {
                    let Ok(data) = receiver.recv() else {
                        break;
                    };
                    if !queue.blocking_write(&data) {
                        break;
                    }
                }
            });

            sender
        });

        if sender.send(data).is_err() {
            return Err(PyValueError::new_err(
                "Failed to send data, the queue has been closed".to_string(),
            ));
        }

        Ok(())
    }

    fn try_read(&self) -> Option<Vec<u8>> {
        self.open_mode.check_read_permission();
        
        let mut buffer = vec![0u8; self.max_element_size];

        let size = deref_queue(&self.shared_memory).try_read(&mut buffer)?;
        buffer.truncate(size);

        Some(buffer)
    }

    fn blocking_read(&self) -> Option<Vec<u8>> {
        self.open_mode.check_read_permission();
        let mut buffer = vec![0u8; self.max_element_size];

        let size = deref_queue(&self.shared_memory).blocking_read(&mut buffer)?;
        buffer.truncate(size);

        Some(buffer)
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
        deref_queue(&self.shared_memory).close();
    }
}
