use pyo3::exceptions::PyValueError;
use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::prelude::MmapStream;
use v4l::video::Capture;
use v4l::{Device, FourCC};

use crate::memory_manager::SharedMemoryManager;
use crate::shared_memory::read_message;

mod memory_manager;
mod shared_memory;

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryWriter {
    shared_memory: SharedMemoryManager,
    name: String,
}

#[pymethods]
impl SharedMemoryWriter {
    #[new]
    fn new(name: String, size: u32) -> PyResult<Self> {
        if size == 0 {
            return Err(PyValueError::new_err("Size cannot be 0"));
        }
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }
        let name = if name.starts_with('/') {
            name
        } else {
            format!("/{name}")
        };
        
        let c_name = CString::new(name.clone())?;
        let shared_memory = SharedMemoryManager::create(c_name, size as usize)?;

        Ok(Self { shared_memory, name })
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        let shared_memory = unsafe { &mut *self.shared_memory.get_ptr() };
        py.allow_threads(|| {
            shared_memory
                .write_message_safe(data)
                .map_err(PyValueError::new_err)
        })?;
        Ok(())
    }
    
    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> usize {
        self.shared_memory.memory_size()
    }
}

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryReader {
    shared_memory: SharedMemoryManager,
    name: String,
    last_version_read: Cell<usize>,
    buffer: RefCell<Vec<u8>>,
}

#[pymethods]
impl SharedMemoryReader {
    #[new]
    fn new(name: String) -> PyResult<Self> {
        let name = if name.starts_with('/') {
            name
        } else {
            format!("/{name}")
        };
        let c_name = &CString::new(name.clone())?;
        let shared_memory = SharedMemoryManager::open(c_name)?;

        Ok(Self {
            buffer: RefCell::new(vec![0u8; shared_memory.memory_size()]),
            name,
            shared_memory,
            last_version_read: Cell::new(0),
        })
    }

    fn read(&self, py: Python<'_>) -> Option<PyObject> {
        let mut last_version = self.last_version_read.get();

        let mut buffer = self.buffer.borrow_mut();

        let bytes_read = {
            let message = unsafe { &*self.shared_memory.get_ptr() };
            let buffer = buffer.as_mut_slice();

            py.allow_threads(|| {
                read_message(message, &mut last_version, buffer).unwrap_or_default()
            })
        };

        if bytes_read == 0 {
            return None;
        }

        self.last_version_read.set(last_version);
        Some(PyBytes::new_bound(py, &buffer[..bytes_read]).into_py(py))
    }

    fn read_in_place(&self, py: Python<'_>, ignore_same_version: bool) -> Option<PyObject> {
        let mut last_version = self.last_version_read.get();

        let message = unsafe { &*self.shared_memory.get_ptr() };

        let _guard = unsafe { message.read_lock() };
        let bytes_read = {
            let message_version = message.version;
            if ignore_same_version && message_version == last_version {
                0 // There is no new data
            } else {
                last_version = message_version;
                message.message_size
            }
        };

        if bytes_read == 0 {
            return None;
        }

        self.last_version_read.set(last_version);
        Some(PyBytes::new_bound(py, &message.data[..bytes_read]).into_py(py))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn size(&self) -> usize {
        self.shared_memory.memory_size()
    }
}

#[pyclass]
#[pyo3(frozen)]
struct V4lSharedMemoryWriter {
    is_running: Arc<AtomicBool>,
}

#[pymethods]
impl V4lSharedMemoryWriter {
    #[new]
    fn new(
        device_path: &str,
        video_width: u32,
        video_height: u32,
        memory_topic: &str,
    ) -> PyResult<Self> {
        if video_width == 0 || video_height == 0 {
            return Err(PyValueError::new_err("Invalid width or height"));
        }
        if memory_topic.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let c_topic = CString::new(memory_topic)?;
        let size = video_width as usize * video_height as usize * 10;
        let shared_memory_manager = SharedMemoryManager::create(c_topic.clone(), size)?;

        let dev = Device::with_path(device_path).expect("Failed to open device");
        let mut fmt = dev.format().expect("Failed to read format");
        fmt.width = video_width;
        fmt.height = video_height;
        fmt.fourcc = FourCC::new(b"MJPG");

        let fmt = dev.set_format(&fmt).expect("Failed to write format");
        println!("Configured device with format: {}", fmt);
        let mut stream = MmapStream::with_buffers(&dev, Type::VideoCapture, 2)
            .expect("Failed to create buffer stream");

        let is_running = Arc::new(AtomicBool::new(true));
        let this = Self {
            is_running: is_running.clone(),
        };

        std::thread::spawn(move || {
            let shared_memory_manager = shared_memory_manager;
            let shared_memory = unsafe { &mut *shared_memory_manager.get_ptr() };
            let mut buffer = vec![0u8; size];

            while is_running.load(Ordering::Relaxed) {
                let (frame_data, _meta_data) = stream.next().unwrap();
                let now = Instant::now();
                let mut decoder = zune_jpeg::JpegDecoder::new(frame_data);
                if let Err(e) = decoder.decode_into(&mut buffer) {
                    println!("Failed to decode frame: {e}")
                } else {
                    let i = decoder.output_buffer_size().unwrap();
                    shared_memory.write_message_safe(&buffer[..i]).unwrap();
                }
                println!("Writing took: {:?}", now.elapsed());
            }
        });

        Ok(this)
    }

    fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

#[pymodule]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SharedMemoryWriter>()?;
    m.add_class::<SharedMemoryReader>()?;
    m.add_class::<V4lSharedMemoryWriter>()?;
    Ok(())
}
