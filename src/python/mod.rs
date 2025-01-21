use crate::python::circular_queue::PythonSharedCircularQueue;
use crate::python::message::PythonSharedMessage;
use crate::python::dynamic_queue::PythonSharedQueue;
use pyo3::prelude::*;
use pyo3::{pymodule, Bound, PyResult};
use crate::python::open_mode::OpenMode;

mod circular_queue;
mod message;
mod dynamic_queue;
mod open_mode;

#[pymodule(gil_used = false)]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<OpenMode>()?;
    m.add_class::<PythonSharedMessage>()?;
    m.add_class::<PythonSharedCircularQueue>()?;
    m.add_class::<PythonSharedQueue>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;

    const NAME: &str = "/test";
    const DEFAULT_SIZE: u32 = 1024;

    fn init(name: &str, size: u32) -> (PythonSharedMessage, PythonSharedMessage) {
        let writer = PythonSharedMessage::create(
            name.to_string(),
            NonZero::new(size).unwrap(),
            OpenMode::WriteOnly,
        )
        .unwrap();
        let reader = PythonSharedMessage::open(name.to_string(), OpenMode::ReadOnly).unwrap();

        (writer, reader)
    }

    #[test]
    fn simple_write_try_read() {
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let (writer, reader) = init(NAME, DEFAULT_SIZE);
            let none = reader.try_read(py);
            assert!(none.is_none());

            writer.write(&data, py).unwrap();
            let version = writer.last_written_version();

            let bytes = reader.try_read(py).unwrap();
            assert_eq!(bytes.as_bytes(), data);
            assert_eq!(version, reader.last_read_version());

            assert!(reader.try_read(py).is_none());
        });
    }

    #[test]
    fn simple_write_blocking_read() {
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let (writer, reader) = init(NAME, DEFAULT_SIZE);
            assert!(reader.try_read(py).is_none());

            writer.write(&data, py).unwrap();
            let version = writer.last_written_version();

            let bytes = reader.blocking_read(py).unwrap();
            assert_eq!(bytes.as_bytes(), data);
            assert_eq!(version, reader.last_read_version());

            assert!(reader.try_read(py).is_none());
        });
    }

    #[test]
    fn simple_write_blocking_read_close() {
        let data = (0u8..255u8).collect::<Vec<_>>();

        Python::with_gil(|py| {
            let (writer, reader) = init(NAME, DEFAULT_SIZE);
            assert!(reader.try_read(py).is_none());

            writer.write(&data, py).unwrap();
            let version = writer.last_written_version();

            let bytes = reader.blocking_read(py).unwrap();
            assert_eq!(bytes.as_bytes(), data);
            assert_eq!(version, reader.last_read_version());

            assert!(reader.try_read(py).is_none());

            writer.close();
            assert!(reader.blocking_read(py).is_none());
            assert!(reader.try_read(py).is_none());
        });
    }
}
