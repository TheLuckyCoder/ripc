use crate::python::circular_queue::SharedCircularQueue;
use crate::python::reader::SharedReader;
use crate::python::writer::SharedWriter;
use pyo3::prelude::*;
use pyo3::{pymodule, Bound, PyResult};

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;
    
    const NAME: &str = "/test";
    const DEFAULT_SIZE:u32 = 1024;

    fn init(name: &str, size: u32) -> (SharedWriter, SharedReader) {
        let writer = SharedWriter::new(name.to_string(), NonZero::new(size).unwrap()).unwrap();
        let reader = SharedReader::new(name.to_string()).unwrap();

        pyo3::append_to_inittab!(ripc);
        pyo3::prepare_freethreaded_python();

        (writer, reader)
    }

    #[test]
    fn simple_write_try_read() {
        let (writer, reader) = init(NAME, DEFAULT_SIZE);
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let none = reader.try_read(py);
            assert!(none.is_none());

            writer.write(&data, py).unwrap();
            let version = writer.last_written_version();

            let bytes = reader.try_read(py).unwrap();
            assert_eq!(bytes.as_bytes(), data);
            assert_eq!(version, reader.last_read_version());

            let none = reader.try_read(py);
            assert!(none.is_none());
        });
    }
}
