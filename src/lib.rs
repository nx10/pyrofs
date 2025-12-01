mod fs;
mod pytypes;
mod tree;

use pyo3::prelude::*;
use pytypes::{PyFilesystem, PyMountHandle};
use tree::{PyDirectory, PyFile, PySymlink};

/// A Python module for mounting Python-owned memory as a FUSE filesystem
#[pymodule]
fn _pyrofs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize logging
    let _ = env_logger::try_init();

    m.add_class::<PyFilesystem>()?;
    m.add_class::<PyFile>()?;
    m.add_class::<PyDirectory>()?;
    m.add_class::<PySymlink>()?;
    m.add_class::<PyMountHandle>()?;

    Ok(())
}
