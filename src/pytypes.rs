use crate::fs::MemFs;
use crate::tree::{InodeTable, NodeRef, PyDirectory, PyFile, PySymlink, ROOT_INO};
use fuser::MountOption;
use parking_lot::Mutex;
use pyo3::exceptions::{PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

/// Handle to a mounted filesystem - used as context manager
#[pyclass(name = "MountHandle")]
pub struct PyMountHandle {
    session: Option<fuser::BackgroundSession>,
    mount_point: PathBuf,
}

#[pymethods]
impl PyMountHandle {
    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &mut self,
        _exc_type: Option<Py<PyAny>>,
        _exc_val: Option<Py<PyAny>>,
        _exc_tb: Option<Py<PyAny>>,
    ) -> PyResult<bool> {
        self.unmount()?;
        Ok(false)
    }

    /// Unmount the filesystem
    fn unmount(&mut self) -> PyResult<()> {
        if let Some(session) = self.session.take() {
            drop(session);
        }
        Ok(())
    }

    /// Get the mount point path
    #[getter]
    fn mount_point(&self) -> &str {
        self.mount_point.to_str().unwrap_or("")
    }

    /// Check if the filesystem is still mounted
    #[getter]
    fn is_mounted(&self) -> bool {
        self.session.is_some()
    }
}

/// The main filesystem object
#[pyclass(name = "MemFS")]
pub struct PyFilesystem {
    inodes: Arc<Mutex<InodeTable>>,
    root: Py<PyDirectory>,
}

#[pymethods]
impl PyFilesystem {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let mut table = InodeTable::new(uid, gid);
        let root = table.init_root(py)?;

        Ok(Self {
            inodes: Arc::new(Mutex::new(table)),
            root,
        })
    }

    /// Get the root directory
    #[getter]
    fn root(&self, py: Python<'_>) -> Py<PyDirectory> {
        self.root.clone_ref(py)
    }

    /// Create a file in the filesystem
    #[pyo3(signature = (path, content=None, mode=0o644))]
    fn create_file(
        &self,
        py: Python<'_>,
        path: &str,
        content: Option<&[u8]>,
        mode: u16,
    ) -> PyResult<Py<PyFile>> {
        let (parent_ino, name) = self.resolve_parent(py, path)?;

        let file = PyFile::new(py, name.to_string(), content, mode)?;
        let file_py = Py::new(py, file)?;

        let mut inodes = self.inodes.lock();

        // Check if already exists
        if inodes.lookup(py, parent_ino, name).is_some() {
            return Err(PyValueError::new_err(format!(
                "File already exists: {}",
                path
            )));
        }

        inodes.insert_file(py, parent_ino, file_py.clone_ref(py))?;
        Ok(file_py)
    }

    /// Create a directory in the filesystem
    #[pyo3(signature = (path, mode=0o755))]
    fn create_dir(&self, py: Python<'_>, path: &str, mode: u16) -> PyResult<Py<PyDirectory>> {
        let (parent_ino, name) = self.resolve_parent(py, path)?;

        let dir = PyDirectory::new(name.to_string(), mode);
        let dir_py = Py::new(py, dir)?;

        let mut inodes = self.inodes.lock();

        // Check if already exists
        if inodes.lookup(py, parent_ino, name).is_some() {
            return Err(PyValueError::new_err(format!(
                "Directory already exists: {}",
                path
            )));
        }

        inodes.insert_dir(py, parent_ino, dir_py.clone_ref(py))?;
        Ok(dir_py)
    }

    /// Create directories recursively (like mkdir -p)
    #[pyo3(signature = (path, mode=0o755))]
    fn makedirs(&self, py: Python<'_>, path: &str, mode: u16) -> PyResult<Py<PyDirectory>> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return Ok(self.root.clone_ref(py));
        }

        let mut current_ino = ROOT_INO;

        for (i, part) in parts.iter().enumerate() {
            let mut inodes = self.inodes.lock();

            if let Some(child_ino) = inodes.lookup(py, current_ino, part) {
                // Directory exists, continue
                match inodes.get(child_ino) {
                    Some(NodeRef::Dir(_)) => {
                        current_ino = child_ino;
                    }
                    Some(NodeRef::File(_)) | Some(NodeRef::Symlink(_)) => {
                        return Err(PyValueError::new_err(format!(
                            "Path component is a file, not a directory: {}",
                            part
                        )));
                    }
                    None => {
                        return Err(PyRuntimeError::new_err("Internal error: dangling inode"));
                    }
                }
            } else {
                // Create the directory
                let dir = PyDirectory::new(part.to_string(), mode);
                let dir_py = Py::new(py, dir)?;
                let new_ino = inodes.insert_dir(py, current_ino, dir_py.clone_ref(py))?;
                current_ino = new_ino;

                // If this is the last component, return it
                if i == parts.len() - 1 {
                    return Ok(dir_py);
                }
            }
        }

        // Return the last directory
        let inodes = self.inodes.lock();
        match inodes.get_dir(current_ino) {
            Some(d) => Ok(d.clone_ref(py)),
            None => Err(PyRuntimeError::new_err(
                "Internal error: directory not found",
            )),
        }
    }

    /// Get a file or directory by path
    fn get(&self, py: Python<'_>, path: &str) -> PyResult<Py<PyAny>> {
        let ino = self.resolve_path(py, path)?;
        let inodes = self.inodes.lock();

        match inodes.get(ino) {
            Some(NodeRef::File(f)) => Ok(f.clone_ref(py).into_any()),
            Some(NodeRef::Dir(d)) => Ok(d.clone_ref(py).into_any()),
            Some(NodeRef::Symlink(s)) => Ok(s.clone_ref(py).into_any()),
            None => Err(PyValueError::new_err(format!("Path not found: {}", path))),
        }
    }

    /// Check if a path exists
    fn exists(&self, py: Python<'_>, path: &str) -> bool {
        self.resolve_path(py, path).is_ok()
    }

    /// Create a symbolic link
    fn symlink(&self, py: Python<'_>, target: &str, path: &str) -> PyResult<Py<PySymlink>> {
        let (parent_ino, name) = self.resolve_parent(py, path)?;

        let symlink = PySymlink::new(name.to_string(), target.to_string());
        let symlink_py = Py::new(py, symlink)?;

        let mut inodes = self.inodes.lock();

        // Check if already exists
        if inodes.lookup(py, parent_ino, name).is_some() {
            return Err(PyValueError::new_err(format!(
                "Path already exists: {}",
                path
            )));
        }

        inodes.insert_symlink(py, parent_ino, symlink_py.clone_ref(py))?;
        Ok(symlink_py)
    }

    /// Read the target of a symbolic link
    fn readlink(&self, py: Python<'_>, path: &str) -> PyResult<String> {
        let ino = self.resolve_path(py, path)?;
        let inodes = self.inodes.lock();

        match inodes.get_symlink(ino) {
            Some(s) => Ok(s.borrow(py).target.clone()),
            None => Err(PyValueError::new_err("Path is not a symlink")),
        }
    }

    /// Check if path is a symlink
    fn is_symlink(&self, py: Python<'_>, path: &str) -> bool {
        if let Ok(ino) = self.resolve_path(py, path) {
            let inodes = self.inodes.lock();
            matches!(inodes.get(ino), Some(NodeRef::Symlink(_)))
        } else {
            false
        }
    }

    /// Remove a file or symlink
    fn remove_file(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let ino = self.resolve_path(py, path)?;
        let mut inodes = self.inodes.lock();

        match inodes.get(ino) {
            Some(NodeRef::File(_)) | Some(NodeRef::Symlink(_)) => {
                inodes.remove(py, ino)?;
                Ok(())
            }
            Some(NodeRef::Dir(_)) => Err(PyValueError::new_err("Path is a directory")),
            None => Err(PyValueError::new_err(format!("Path not found: {}", path))),
        }
    }

    /// Remove a directory (must be empty)
    fn remove_dir(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        let ino = self.resolve_path(py, path)?;
        let mut inodes = self.inodes.lock();

        match inodes.get(ino) {
            Some(NodeRef::Dir(d)) => {
                if !d.borrow(py).children.is_empty() {
                    return Err(PyValueError::new_err("Directory not empty"));
                }
                inodes.remove(py, ino)?;
                Ok(())
            }
            Some(NodeRef::File(_)) | Some(NodeRef::Symlink(_)) => {
                Err(PyValueError::new_err("Path is a file, not a directory"))
            }
            None => Err(PyValueError::new_err(format!("Path not found: {}", path))),
        }
    }

    /// List contents of a directory
    fn listdir(&self, py: Python<'_>, path: &str) -> PyResult<Vec<String>> {
        let ino = self.resolve_path(py, path)?;
        let inodes = self.inodes.lock();

        match inodes.get_dir(ino) {
            Some(d) => Ok(d.borrow(py).children.keys().cloned().collect()),
            None => Err(PyValueError::new_err("Path is not a directory")),
        }
    }

    /// Mount the filesystem at the given path
    #[pyo3(signature = (mount_point, allow_other=false))]
    fn mount(&self, mount_point: &str, allow_other: bool) -> PyResult<PyMountHandle> {
        let mount_path = PathBuf::from(mount_point);

        // Ensure mount point exists
        if !mount_path.exists() {
            return Err(PyOSError::new_err(format!(
                "Mount point does not exist: {}",
                mount_point
            )));
        }

        let fs = MemFs::new(Arc::clone(&self.inodes));

        let mut options = vec![
            MountOption::FSName("pyrofs".to_string()),
            MountOption::AutoUnmount,
            MountOption::DefaultPermissions,
        ];

        if allow_other {
            options.push(MountOption::AllowOther);
        }

        let session = fuser::spawn_mount2(fs, &mount_path, &options)
            .map_err(|e| PyOSError::new_err(format!("Failed to mount filesystem: {}", e)))?;

        Ok(PyMountHandle {
            session: Some(session),
            mount_point: mount_path,
        })
    }

    /// Rename/move a file or directory
    fn rename(&self, py: Python<'_>, old_path: &str, new_path: &str) -> PyResult<()> {
        let (old_parent_ino, old_name) = self.resolve_parent(py, old_path)?;
        let (new_parent_ino, new_name) = self.resolve_parent(py, new_path)?;

        let mut inodes = self.inodes.lock();

        // Check source exists
        if inodes.lookup(py, old_parent_ino, old_name).is_none() {
            return Err(PyValueError::new_err(format!(
                "Source path not found: {}",
                old_path
            )));
        }

        // Check if destination exists - if so, handle appropriately
        if let Some(existing_ino) = inodes.lookup(py, new_parent_ino, new_name) {
            let src_ino = inodes.lookup(py, old_parent_ino, old_name).unwrap();
            let src_is_dir = matches!(inodes.get(src_ino), Some(NodeRef::Dir(_)));
            let dst_is_dir = matches!(inodes.get(existing_ino), Some(NodeRef::Dir(_)));

            if src_is_dir != dst_is_dir {
                return Err(PyValueError::new_err(
                    "Cannot overwrite directory with file or vice versa",
                ));
            }

            if dst_is_dir
                && let Some(NodeRef::Dir(d)) = inodes.get(existing_ino)
                && !d.borrow(py).children.is_empty()
            {
                return Err(PyValueError::new_err("Destination directory not empty"));
            }

            // Remove the destination
            inodes.remove(py, existing_ino)?;
        }

        inodes.rename(py, old_parent_ino, old_name, new_parent_ino, new_name)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let root = self.root.borrow(py);
        format!("MemFS(files={})", root.children.len())
    }
}

impl PyFilesystem {
    /// Resolve a path to its inode
    fn resolve_path(&self, py: Python<'_>, path: &str) -> PyResult<u64> {
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        if parts.is_empty() {
            return Ok(ROOT_INO);
        }

        let inodes = self.inodes.lock();
        let mut current = ROOT_INO;

        for part in parts {
            match inodes.lookup(py, current, part) {
                Some(ino) => current = ino,
                None => {
                    return Err(PyValueError::new_err(format!("Path not found: {}", path)));
                }
            }
        }

        Ok(current)
    }

    /// Resolve path to parent directory inode and final component name
    fn resolve_parent<'a>(&self, py: Python<'_>, path: &'a str) -> PyResult<(u64, &'a str)> {
        let path = path.trim_matches('/');
        if path.is_empty() {
            return Err(PyValueError::new_err("Cannot operate on root directory"));
        }

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let name = parts
            .last()
            .ok_or_else(|| PyValueError::new_err("Invalid path"))?;

        if parts.len() == 1 {
            return Ok((ROOT_INO, name));
        }

        let parent_path = parts[..parts.len() - 1].join("/");
        let parent_ino = self.resolve_path(py, &parent_path)?;

        // Verify parent is a directory
        let inodes = self.inodes.lock();
        match inodes.get(parent_ino) {
            Some(NodeRef::Dir(_)) => Ok((parent_ino, name)),
            _ => Err(PyValueError::new_err("Parent is not a directory")),
        }
    }
}
