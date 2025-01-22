use pyo3::pyclass;

#[pyclass(eq, eq_int)]
#[derive(Copy, Clone, PartialEq)]
pub enum OpenMode {
    ReadOnly = 0,
    WriteOnly = 1,
    ReadWrite = 2,
}

impl OpenMode {
    pub fn can_read(self) -> bool {
        self == Self::ReadOnly || self == Self::ReadWrite
    }

    pub fn can_write(self) -> bool {
        self == Self::WriteOnly || self == Self::ReadWrite
    }

    pub fn check_read_permission(self) {
        if !self.can_read() {
            panic!("Shared memory was opened as write-only")
        }
    }

    pub fn check_write_permission(self) {
        if !self.can_write() {
            panic!("Shared memory was opened as read-only")
        }
    }
}

