use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::circle_buffer::CircularBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use v4l::buffer::Type;
use v4l::io::traits::CaptureStream;
use v4l::prelude::MmapStream;
use v4l::video::Capture;
use v4l::{Device, FourCC};

use crate::memory_holder::SharedMemoryHolder;
use crate::pthread_lock::{PThreadLock, PThreadRwLock};
use crate::shared_memory::{read_message, SharedMemory};

mod circle_buffer;
mod memory_holder;
mod pthread_lock;
mod shared_memory;
mod utils;

#[pyclass]
#[pyo3(frozen)]
struct SharedMemoryWriter {
    shared_memory: SharedMemoryHolder,
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

        let c_name = CString::new(name.clone())?;
        let shared_memory = SharedMemoryHolder::create(c_name, size as usize)?;
        unsafe { PThreadRwLock::initialize_at(shared_memory.ptr().cast())? };

        Ok(Self {
            shared_memory,
            name,
        })
    }

    fn write(&self, data: &[u8], py: Python<'_>) -> PyResult<()> {
        let shared_memory = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut SharedMemory) };
        py.allow_threads(|| {
            shared_memory
                .write_message(data)
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
    shared_memory: SharedMemoryHolder,
    name: String,
    last_version_read: Cell<usize>,
    buffer: RefCell<Vec<u8>>,
}

#[pymethods]
impl SharedMemoryReader {
    #[new]
    fn new(name: String) -> PyResult<Self> {
        let c_name = &CString::new(name.clone())?;
        let shared_memory = SharedMemoryHolder::open(c_name)?;

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
            let message = unsafe { &*(self.shared_memory.slice_ptr() as *mut SharedMemory) };
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

    #[pyo3(signature = (ignore_same_version=false))]
    fn read_in_place(&self, py: Python<'_>, ignore_same_version: bool) -> Option<PyObject> {
        let mut last_version = self.last_version_read.get();

        let message = unsafe { &*(self.shared_memory.slice_ptr() as *mut SharedMemory) };

        let _guard = unsafe { message.lock.read_lock() };
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
struct SharedMemoryCircularQueue {
    shared_memory: SharedMemoryHolder,
    name: String,
    buffer: RefCell<Vec<u8>>,
}

#[pymethods]
impl SharedMemoryCircularQueue {
    #[new]
    #[pyo3(signature = (name, create=false, element_size=0, elements_count=0))]
    fn new(name: String, create: bool, element_size: u32, elements_count: u32) -> PyResult<Self> {
        if name.is_empty() {
            return Err(PyValueError::new_err("Topic cannot be empty"));
        }

        let c_name = CString::new(name.clone())?;
        let memory_holder = if create {
            if element_size == 0 || elements_count == 0 {
                return Err(PyValueError::new_err("Size cannot be 0"));
            }

            let buffer_size = element_size as usize * elements_count as usize;
            SharedMemoryHolder::create(c_name, CircularBuffer::size_of_fields() + buffer_size)
        } else {
            SharedMemoryHolder::open(c_name.as_c_str())
        }?;

        if create {
            unsafe { PThreadLock::initialize_at(memory_holder.ptr().cast())? };
        }

        Ok(Self {
            shared_memory: memory_holder,
            name,
            buffer: RefCell::new(vec![0u8; element_size as usize]),
        })
    }

    fn __len__(&self) -> usize {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };
        queue.len()
    }

    fn is_full(&self) -> bool {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };
        queue.is_full()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn try_read(&self, py: Python<'_>) -> Option<PyObject> {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        if queue.try_read(buffer) {
            Some(PyBytes::new_bound(py, buffer).into_py(py))
        } else {
            None
        }
    }

    fn blocking_read(&self, py: Python<'_>) -> PyObject {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        let mut borrowed_buffer = self.buffer.borrow_mut();
        let buffer = borrowed_buffer.as_mut_slice();

        py.allow_threads(|| queue.blocking_read(buffer));

        PyBytes::new_bound(py, buffer).into_py(py)
    }

    fn try_write(&self, data: &[u8]) -> bool {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        queue.try_write(data)
    }

    fn blocking_write(&self, py: Python<'_>, data: &[u8]) {
        let queue = unsafe { &mut *(self.shared_memory.slice_ptr() as *mut CircularBuffer) };

        py.allow_threads(|| {
            queue.blocking_write(data);
        });
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
        let shared_memory_manager = SharedMemoryHolder::create(c_topic.clone(), size)?;

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
            let shared_memory =
                unsafe { &mut *(shared_memory_manager.slice_ptr() as *mut SharedMemory) };
            let mut buffer = vec![0u8; size];

            while is_running.load(Ordering::Relaxed) {
                let (frame_data, _meta_data) = stream.next().unwrap();
                let now = Instant::now();
                let mut decoder = zune_jpeg::JpegDecoder::new(frame_data);
                if let Err(e) = decoder.decode_into(&mut buffer) {
                    println!("Failed to decode frame: {e}")
                } else {
                    let i = decoder.output_buffer_size().unwrap();
                    shared_memory.write_message(&buffer[..i]).unwrap();
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
