
mod circular_queue;
mod primitives;
mod shared_memory;
mod python;


// #[pymodule]
// fn ripc(m: &Bound<'_, PyModule>) -> PyResult<()> {
//     m.add_class::<SharedMemoryWriter>()?;
//     m.add_class::<SharedMemoryReader>()?;
//     m.add_class::<SharedMemoryCircularQueue>()?;
//     Ok(())
// }
