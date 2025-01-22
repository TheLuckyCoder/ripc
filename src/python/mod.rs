use crate::helpers::bytes::RustBytes;
use crate::python::circular_queue::PythonSharedCircularQueue;
use crate::python::dynamic_queue::PythonSharedQueue;
use crate::python::message::PythonSharedMessage;
use crate::python::open_mode::OpenMode;
use pyo3::prelude::*;
use pyo3::types::PyFunction;
use pyo3::{pymodule, Bound, PyResult};
use rayon::prelude::*;

mod circular_queue;
mod dynamic_queue;
mod message;
mod open_mode;

#[pymodule(gil_used = false)]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<OpenMode>()?;
    m.add_class::<PythonSharedMessage>()?;
    m.add_class::<PythonSharedCircularQueue>()?;
    m.add_class::<PythonSharedQueue>()?;
    
    m.add_function(wrap_pyfunction!(read_all, m)?)?;
    m.add_function(wrap_pyfunction!(read_all_map, m)?)?;
    
    Ok(())
}

#[pyfunction]
fn read_all(readers: Vec<Py<PythonSharedMessage>>, py: Python<'_>) -> Vec<Option<RustBytes>> {
    py.allow_threads(|| {
        readers
            .into_par_iter()
            .map(|reader| reader.get().try_read())
            .collect()
    })
}

#[pyfunction]
fn read_all_map(
    readers: Vec<Py<PythonSharedMessage>>,
    map: Py<PyFunction>,
    py: Python<'_>,
) -> Vec<Option<Py<PyAny>>> {
    py.allow_threads(|| {
        readers
            .into_par_iter()
            .map(|reader| reader.get().try_read())
            .map(|bytes| bytes.map(|bytes| Python::with_gil(|py| map.call1(py, (bytes,)).unwrap())))
            .collect()
    })
}
