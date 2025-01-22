use std::ffi::c_int;
use pyo3::{pyclass, pymethods, Bound, PyErr, PyRef, PyResult, Python};
use pyo3::types::PyBytes;

#[pyclass]
#[pyo3(frozen)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustBytes(pub Box<[u8]>);

impl RustBytes {
    pub fn new(slice: &[u8]) -> Self {
        Self(Box::from(slice))
    }
}

#[pymethods]
impl RustBytes {
    /// The number of bytes in this Bytes
    fn __len__(&self) -> usize {
        self.0.len()
    }

    unsafe fn __getbuffer__(
        slf: PyRef<Self>,
        view: *mut pyo3::ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        let bytes = slf.0.as_ref();
        let ret = pyo3::ffi::PyBuffer_FillInfo(
            view,
            slf.as_ptr() as *mut _,
            bytes.as_ptr() as *mut _,
            bytes.len().try_into()?,
            1, // read only
            flags,
        );
        if ret == -1 {
            return Err(PyErr::fetch(slf.py()));
        }
        Ok(())
    }

    unsafe fn __releasebuffer__(&self, _view: *mut pyo3::ffi::Py_buffer) {}

    /// Copy this buffer's contents to a Python `bytes` object
    fn to_bytes<'py>(&'py self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.0)
    }
}

