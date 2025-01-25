use crate::container::message::SharedMessage;
use crate::helpers::bytes::RustPyBytes;
use crate::helpers::queue_data::SenderQueueData;
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
    data: RustPyBytes,
}

#[pyclass]
#[pyo3(frozen, name = "SharedQueue")]
pub struct PythonSharedQueue {
    shared_memory: Arc<SharedMemoryHolder<SharedMessage>>,
    sender: Mutex<Option<Sender<SenderQueueData>>>,
    receiver: Mutex<Option<Receiver<ReceiverQueueData>>>,
    name: String,
    open_mode: OpenMode,
    last_written_version: Arc<AtomicUsize>,
    last_read_version: Arc<AtomicUsize>,
}

impl PythonSharedQueue {
    fn new(
        shared_memory: Arc<SharedMemoryHolder<SharedMessage>>,
        name: String,
        open_mode: OpenMode,
    ) -> Self {
        if open_mode.can_read() {
            shared_memory.add_reader();
        }

        let last_read_version = Arc::new(AtomicUsize::default());

        let receiver = open_mode
            .can_read()
            .then(|| Self::start_reader_thread(shared_memory.clone(), last_read_version.clone()));

        Self {
            shared_memory,
            name,
            sender: Mutex::default(),
            receiver: Mutex::new(receiver),
            open_mode,
            last_written_version: Arc::default(),
            last_read_version,
        }
    }
}

#[pymethods]
impl PythonSharedQueue {
    #[staticmethod]
    fn create(name: String, max_element_size: NonZeroU32, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let max_element_size = max_element_size.get() as usize;

        let shared_memory = unsafe {
            Arc::new(SharedMemoryHolder::<SharedMessage>::create(
                CString::new(name.clone())?,
                SharedMessage::size_of_fields() + max_element_size,
            )?)
        };

        Ok(Self::new(shared_memory, name, mode))
    }

    #[staticmethod]
    fn open(name: String, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let shared_memory = unsafe {
            Arc::new(SharedMemoryHolder::<SharedMessage>::open(CString::new(
                name.clone(),
            )?)?)
        };

        Ok(Self::new(shared_memory, name, mode))
    }

    fn write(&self, data: Bound<'_, PyBytes>) -> PyResult<()> {
        self.open_mode.check_write_permission();
        let queue_data = SenderQueueData::new(data);

        let mut guard = self.sender.lock().unwrap();
        let sender = guard.get_or_insert_with(|| {
            let (sender, receiver) = channel::<SenderQueueData>();

            let last_written_version = self.last_written_version.clone();
            let shared_memory = self.shared_memory.clone();
            std::thread::spawn(move || loop {
                let Ok(data) = receiver.recv() else {
                    break;
                };
                let new_version = shared_memory.write_waiting_for_readers(data.bytes(), None);

                last_written_version.store(new_version, Ordering::Relaxed);
            });

            sender
        });

        sender.send(queue_data).map_err(|_| {
            PyValueError::new_err("Failed to send data, the queue has been closed".to_string())
        })
    }

    fn try_read(&self) -> Option<RustPyBytes> {
        self.open_mode.check_read_permission();

        let guard = self.receiver.lock().unwrap();
        let receiver = guard.as_ref().expect("A reader must have a receiver");

        receiver.try_recv().ok().map(|message| {
            self.last_read_version
                .store(message.version, Ordering::Relaxed);
            message.data
        })
    }

    fn blocking_read(&self, py: Python<'_>) -> Option<RustPyBytes> {
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
        self.shared_memory.mapped_memory_size()
    }

    fn is_closed(&self) -> bool {
        self.shared_memory.is_closed()
    }

    fn close(&self) {
        self.open_mode.check_write_permission();

        self.shared_memory.close();
    }
}

impl PythonSharedQueue {
    fn start_reader_thread(
        shared_memory: Arc<SharedMemoryHolder<SharedMessage>>,
        last_read_version: Arc<AtomicUsize>,
    ) -> Receiver<ReceiverQueueData> {
        let (sender, receiver) = channel();
        let mut local_last_reader_version = last_read_version.load(Ordering::Relaxed);

        std::thread::spawn(move || {
            while !shared_memory.is_closed() {
                let mut queue_data = None;
                shared_memory.blocking_read(local_last_reader_version, |new_version, data| {
                    queue_data = Some(ReceiverQueueData {
                        version: new_version,
                        data: RustPyBytes::new(data),
                    });
                });

                if let Some(queue_data) = queue_data {
                    local_last_reader_version = queue_data.version;
                    last_read_version.store(local_last_reader_version, Ordering::Relaxed);
                    let _ = sender.send(queue_data);
                }
            }
        });

        receiver
    }
}

impl Drop for PythonSharedQueue {
    fn drop(&mut self) {
        if self.open_mode.can_read() {
            self.shared_memory.remove_reader();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;
    use std::thread;
    use std::time::Duration;

    const DEFAULT_SIZE: u32 = 1024;

    fn init(name: &str, size: u32) -> PythonSharedQueue {
        PythonSharedQueue::create(
            name.to_string(),
            NonZero::new(size).unwrap(),
            OpenMode::ReadWrite,
        )
        .unwrap()
    }

    #[test]
    fn simple_write_try_read() {
        Python::with_gil(|py| {
            let data = PyBytes::new(py, &(0u8..255u8).collect::<Vec<_>>());

            let memory = init("queue_simple_write_try_read", DEFAULT_SIZE);
            let none = memory.try_read();
            assert!(none.is_none());

            memory.write(data.clone()).unwrap();
            thread::sleep(Duration::from_millis(200));
            let version = memory.last_written_version();

            let bytes = memory.try_read().unwrap();
            assert_eq!(bytes.0.as_ref(), data);
            assert_eq!(version, memory.last_read_version());

            assert!(memory.try_read().is_none());
            memory.close();
        });
    }

    #[test]
    fn simple_write_blocking_read() {
        Python::with_gil(|py| {
            let data = PyBytes::new(py, &(0u8..255u8).collect::<Vec<_>>());

            let memory = init("queue_simple_write_blocking_read", DEFAULT_SIZE);

            memory.write(data.clone()).unwrap();
            thread::sleep(Duration::from_millis(200));
            let version = memory.last_written_version();

            let bytes = memory.blocking_read(py).unwrap();
            assert_eq!(bytes.0.as_ref(), data);
            assert_eq!(version, memory.last_read_version());

            memory.close();
        });
    }

    #[test]
    fn multiple_writes() {
        Python::with_gil(|py| {
            let memory = init("queue_multiple_writes", DEFAULT_SIZE);

            for i in 0..100 {
                memory.write(PyBytes::new(py, &[i])).unwrap();
            }

            for i in 0..100 {
                assert_eq!(memory.blocking_read(py).unwrap(), RustPyBytes::new(&[i]));
            }
            memory.close();
        });
    }
}
