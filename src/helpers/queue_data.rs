use pyo3::{Bound, Py};
use pyo3::types::PyBytes;

pub struct QueueData {
    _py_bytes: Py<PyBytes>,
    bytes: *const [u8],
}

unsafe impl Send for QueueData {}

impl QueueData {
    pub fn new(data: Bound<PyBytes>) -> Self {
        let gil = data.py();
        let py_bytes = data.unbind();
        let bytes = py_bytes.as_bytes(gil);

        // bypass the rust borrow checker
        Self {
            bytes: bytes as *const [u8],
            _py_bytes: py_bytes,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { &*self.bytes }
    }
}

