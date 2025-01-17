use crate::python::circular_queue::SharedCircularQueue;
use crate::python::shared_memory::PythonSharedMemory;
use pyo3::prelude::*;
use pyo3::{pymodule, Bound, PyResult};
use pyo3::exceptions::PyValueError;

mod circular_queue;
mod shared_memory;

#[pyclass(eq, eq_int)]
#[derive(Copy, Clone, PartialEq)]
enum MemoryPermission {
    ReadOnly,
    WriteOnly,
    ReadWrite
}

impl MemoryPermission {
    fn can_read(self) -> bool {
        self == Self::ReadOnly || self == Self::ReadWrite
    }
    
    fn can_write(self) -> bool {
        self == Self::WriteOnly || self == Self::ReadWrite
    }
}

fn no_read_permission_err() -> PyErr {
    PyValueError::new_err("Shared memory was opened as write-only")
}

fn no_write_permission_err() -> PyErr {
    PyValueError::new_err("Shared memory was opened as read-only")
}

#[pymodule]
fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PythonSharedMemory>()?;
    m.add_class::<SharedCircularQueue>()?;
    Ok(())
}

/*#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZero;

    const NAME: &str = "/test";
    const DEFAULT_SIZE: u32 = 1024;

    fn init(name: &str, size: u32) -> (PythonSharedMemory, PythonSharedMemory) {
        let writer =
            PythonSharedMemory::create(name.to_string(), NonZero::new(size).unwrap()).unwrap();
        let reader = PythonSharedMemory::open(name.to_string(), true).unwrap();

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

            writer.close().unwrap();
            assert!(reader.blocking_read(py).is_none());
            assert!(reader.try_read(py).is_none());
        });
    }

    #[test]
    fn multiple_writes() {
        Python::with_gil(|py| {
            let (writer, reader) = init(NAME, DEFAULT_SIZE);
            assert!(reader.try_read(py).is_none());

            for i in 10..100 {
                let data = (0u8..i).collect::<Vec<_>>();
                writer.write(&data, py).unwrap();
                let version = writer.last_written_version();

                let bytes = reader.try_read(py).unwrap();
                assert_eq!(version, reader.last_read_version());
                assert_eq!(bytes.as_bytes(), data);

                assert!(reader.try_read(py).is_none());
            }
        });
    }
}*/
