use pyo3::prelude::*;
use pyo3::{pymodule, Bound, PyResult};
use crate::python::circular_queue::SharedCircularQueue;
use crate::python::reader::SharedReader;
use crate::python::writer::SharedWriter;

mod circular_queue;
mod reader;
mod writer;

#[pymodule]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SharedWriter>()?;
    m.add_class::<SharedReader>()?;
    m.add_class::<SharedCircularQueue>()?;
    Ok(())
}
