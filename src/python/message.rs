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
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

#[pyclass]
#[pyo3(frozen, name = "SharedMessage")]
pub struct PythonSharedMessage {
    shared_memory: Arc<SharedMemoryHolder<SharedMessage>>,
    name: String,
    memory_size: usize,
    open_mode: OpenMode,
    last_written_version: Arc<AtomicUsize>,
    last_read_version: AtomicUsize,
    sender: Mutex<Option<Sender<SenderQueueData>>>,
}

impl PythonSharedMessage {
    pub fn new(
        shared_memory: SharedMemoryHolder<SharedMessage>,
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
    fn create(name: String, size: NonZeroU32, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;

        let shared_memory = unsafe {
            SharedMemoryHolder::<SharedMessage>::create(
                c_name,
                SharedMessage::size_of_fields() + size.get() as usize,
            )?
        };

        let memory_size = shared_memory.data.lock().bytes.len();

        if mode.can_read() {
            shared_memory.readers_count.fetch_add(1, Ordering::Relaxed);
        }

        Ok(Self::new(shared_memory, name, memory_size, mode))
    }

    #[staticmethod]
    #[pyo3(signature = (name, mode=OpenMode::ReadWrite))]
    fn open(name: String, mode: OpenMode) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Name cannot be empty"));
        }
        let shared_memory =
            unsafe { SharedMemoryHolder::<SharedMessage>::open(CString::new(name.clone())?)? };

        let memory_size = shared_memory.data.lock().bytes.len();

        if mode.can_read() {
            shared_memory.readers_count.fetch_add(1, Ordering::Relaxed);
        }

        Ok(Self::new(shared_memory, name, memory_size, mode))
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        self.open_mode.check_write_permission();

        if data.len() > self.memory_size {
            return Err(PyValueError::new_err(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                self.memory_size,
                data.len()
            )));
        }

        py.allow_threads(|| {
            let version = self.shared_memory.write(data);
            self.last_written_version.store(version, Ordering::Relaxed);
        });

        Ok(())
    }

    fn write_async(&self, data: Bound<'_, PyBytes>) -> PyResult<()> {
        self.open_mode.check_write_permission();
        let queue_data = SenderQueueData::new(data);

        if queue_data.bytes().len() > self.memory_size {
            return Err(PyValueError::new_err(format!(
                "Message is too large to be sent! Max size: {}. Current message size: {}",
                self.memory_size,
                queue_data.bytes().len()
            )));
        }

        let mut guard = self.sender.lock().unwrap();
        let sender = guard.get_or_insert_with(|| {
            let (sender, receiver) = channel::<SenderQueueData>();

            let last_written_version = self.last_written_version.clone();
            let shared_memory = self.shared_memory.clone();
            std::thread::spawn(move || loop {
                let Ok(data) = receiver.recv() else {
                    break;
                };
                let data = receiver.try_iter().last().unwrap_or(data);

                if shared_memory.closed.load(Ordering::Relaxed) {
                    break;
                }
                let new_version = shared_memory.write(data.bytes());

                last_written_version.store(new_version, Ordering::Relaxed);
            });

            sender
        });

        sender.send(queue_data).map_err(|_| {
            PyValueError::new_err("Failed to send data, the queue has been closed".to_string())
        })
    }

    pub fn try_read(&self) -> Option<RustPyBytes> {
        self.open_mode.check_read_permission();
        let last_read_version = self.last_read_version.load(Ordering::Relaxed);

        let mut result = None;

        self.shared_memory
            .try_read(last_read_version, |new_version, data| {
                self.last_read_version.store(new_version, Ordering::Relaxed);
                result = Some(RustPyBytes::new(data));
            });

        result
    }

    fn blocking_read(&self, py: Python<'_>) -> Option<RustPyBytes> {
        self.open_mode.check_read_permission();

        let mut result = None;

        py.allow_threads(|| {
            let last_read_version = self.last_read_version.load(Ordering::Relaxed);
            self.shared_memory
                .blocking_read(last_read_version, |new_version, data| {
                    self.last_read_version.store(new_version, Ordering::Relaxed);
                    result = Some(RustPyBytes::new(data));
                });
        });

        result
    }

    fn is_new_version_available(&self) -> bool {
        self.open_mode.check_read_permission();

        let last_read_version = self.last_read_version.load(Ordering::Relaxed);
        self.shared_memory
            .is_new_version_available(last_read_version)
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
        self.shared_memory.closed.load(Ordering::Relaxed)
    }

    fn close(&self) {
        self.open_mode.check_write_permission();
        self.shared_memory.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;
    use std::thread;
    use std::time::Duration;

    const DEFAULT_SIZE: u32 = 1024;

    fn init(name: &str, size: u32) -> PythonSharedMessage {
        PythonSharedMessage::create(
            name.to_string(),
            NonZero::new(size).unwrap(),
            OpenMode::ReadWrite,
        )
        .unwrap()
    }

    #[test]
    fn simple_write_try_read() {
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let memory = init("simple_write_try_read", DEFAULT_SIZE);
            let none = memory.try_read();
            assert!(none.is_none());

            memory.write(&data, py).unwrap();
            let version = memory.last_written_version();

            let bytes = memory.try_read().unwrap();
            assert_eq!(bytes.0.as_ref(), data);
            assert_eq!(version, memory.last_read_version());

            assert!(memory.try_read().is_none());
        });
    }

    #[test]
    fn simple_write_blocking_read() {
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let memory = init("simple_write_blocking_read", DEFAULT_SIZE);
            assert!(memory.try_read().is_none());

            memory.write(&data, py).unwrap();
            let version = memory.last_written_version();

            let bytes = memory.blocking_read(py).unwrap();
            assert_eq!(bytes.0.as_ref(), data);
            assert_eq!(version, memory.last_read_version());

            assert!(memory.try_read().is_none());
        });
    }

    #[test]
    fn simple_write_blocking_read_close() {
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let memory = init("simple_write_blocking_read_close", DEFAULT_SIZE);
            assert!(memory.try_read().is_none());

            memory.write(&data, py).unwrap();
            let version = memory.last_written_version();

            let bytes = memory.blocking_read(py).unwrap();
            assert_eq!(bytes.0.as_ref(), data);
            assert_eq!(version, memory.last_read_version());

            assert!(memory.try_read().is_none());

            memory.close();
            assert!(memory.blocking_read(py).is_none());
            assert!(memory.try_read().is_none());
        });
    }

    #[test]
    fn async_write() {
        Python::with_gil(|py| {
            let memory = init("async_write", DEFAULT_SIZE);

            memory.write_async(PyBytes::new(py, &[1])).unwrap();
            memory.write_async(PyBytes::new(py, &[2])).unwrap();
            memory.write_async(PyBytes::new(py, &[3])).unwrap();
            memory.write_async(PyBytes::new(py, &[4])).unwrap();
            thread::sleep(Duration::from_millis(100));
            assert!(memory.is_new_version_available());
            assert_eq!(memory.blocking_read(py).unwrap(), RustPyBytes::new(&[4]));
        });
    }
}
