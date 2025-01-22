use crate::container::shared_memory::SharedMemory;
use crate::helpers::bytes::RustBytes;
use crate::helpers::queue_data::QueueData;
use crate::primitives::memory_holder::SharedMemoryHolder;
use crate::python::OpenMode;
use pyo3::exceptions::PyValueError;
use pyo3::types::PyBytes;
use pyo3::{pyclass, pymethods, Bound, PyResult, Python};
use std::ffi::CString;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

struct ReceiverQueueData {
    version: usize,
    data: RustBytes,
}

#[pyclass]
#[pyo3(frozen, name = "SharedQueue")]
pub struct PythonSharedQueue {
    shared_memory: Arc<SharedMemoryHolder>,
    sender: Mutex<Option<Sender<QueueData>>>,
    receiver: Mutex<Option<Receiver<ReceiverQueueData>>>,
    name: String,
    open_mode: OpenMode,
    last_written_version: Arc<AtomicUsize>,
    last_read_version: Arc<AtomicUsize>,
}

fn deref_shared_memory(shared_memory: &SharedMemoryHolder) -> &SharedMemory {
    unsafe { &*(shared_memory.slice_ptr() as *const SharedMemory) }
}

#[pymethods]
impl PythonSharedQueue {
    #[staticmethod]
    fn create(name: String, max_element_size: NonZeroU32, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let max_element_size = max_element_size.get() as usize;

        let shared_memory = Arc::new(SharedMemoryHolder::create(
            CString::new(name.clone())?,
            SharedMemory::size_of_fields() + max_element_size,
        )?);
        
        let last_read_version = Arc::new(AtomicUsize::default());
        let receiver = mode.can_read().then(|| {
            Self::start_reader_thread(shared_memory.clone(), last_read_version.clone())
        });

        Ok(Self {
            shared_memory,
            name,
            sender: Mutex::default(),
            receiver: Mutex::new(receiver),
            open_mode: mode,
            last_written_version: Arc::default(),
            last_read_version,
        })
    }

    #[staticmethod]
    fn open(name: String, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let shared_memory = Arc::new(SharedMemoryHolder::open(CString::new(name.clone())?)?);
        
        let last_read_version = Arc::new(AtomicUsize::default());
        let receiver = mode.can_read().then(|| {
            Self::start_reader_thread(shared_memory.clone(), last_read_version.clone())
        });

        Ok(Self {
            name,
            shared_memory,
            sender: Mutex::default(),
            receiver: Mutex::new(receiver),
            open_mode: mode,
            last_written_version: Arc::default(),
            last_read_version,
        })
    }

    fn write(&self, data: Bound<'_, PyBytes>) -> PyResult<()> {
        self.open_mode.check_write_permission();
        let queue_data = QueueData::new(data);

        let mut guard = self.sender.lock().unwrap();
        let sender = guard.get_or_insert_with(|| {
            let (sender, receiver) = channel::<QueueData>();

            let last_written_version = self.last_written_version.clone();
            let shared_memory = self.shared_memory.clone();
            std::thread::spawn(move || {
                let message = deref_shared_memory(&shared_memory);

                loop {
                    let Ok(data) = receiver.recv() else {
                        break;
                    };
                    let lock = message.data.lock(); // TODO lock
                    if message.version.load(Ordering::Relaxed)
                        == message.reader_version.load(Ordering::Relaxed)
                    {
                        message.read_condvar.wait(lock);
                    } else {
                        drop(lock);
                    }
                    message.write(data.bytes());

                    last_written_version.fetch_add(1, Ordering::Relaxed);
                }
            });

            sender
        });

        sender.send(queue_data).map_err(|_| {
            PyValueError::new_err("Failed to send data, the queue has been closed".to_string())
        })
    }

    fn try_read(&self) -> Option<RustBytes> {
        self.open_mode.check_read_permission();

        let guard = self.receiver.lock().unwrap();
        let receiver = guard.as_ref().expect("A reader must have a receiver");

        receiver.try_recv().ok().map(|message| {
            self.last_read_version
                .store(message.version, Ordering::Relaxed);
            message.data
        })
    }

    fn blocking_read(&self, py: Python<'_>) -> Option<RustBytes> {
        self.open_mode.check_read_permission();

        py.allow_threads(|| {
            let guard = self.receiver.lock().unwrap();
            let receiver = guard.as_ref().expect("A reader must have a receiver");

            receiver.recv().ok().map(|message| {
                self.last_read_version
                    .store(message.version, Ordering::Relaxed);
                message.data
            })
        })
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn memory_size(&self) -> usize {
        self.shared_memory.slice_ptr().len()
    }

    fn is_closed(&self) -> bool {
        deref_shared_memory(&self.shared_memory).is_closed()
    }

    fn close(&self) {
        self.open_mode.check_write_permission();

        deref_shared_memory(&self.shared_memory).close();
    }
}

impl PythonSharedQueue {
    fn start_reader_thread(
        shared_memory: Arc<SharedMemoryHolder>,
        last_read_version: Arc<AtomicUsize>,
    ) -> Receiver<ReceiverQueueData> {
        let (sender, receiver) = channel();
        let local_last_reader_version = last_read_version.load(Ordering::Relaxed);

        std::thread::spawn(move || {
            let message = deref_shared_memory(&shared_memory);

            while !message.is_closed() {
                message.blocking_read(local_last_reader_version, |new_version, data| {
                    let data = ReceiverQueueData {
                        version: new_version,
                        data: RustBytes::new(data),
                    };

                    let _ = sender.send(data);
                });
            }
        });

        receiver
    }
}
