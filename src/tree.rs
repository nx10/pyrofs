use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::collections::HashMap;
use std::time::SystemTime;

/// Unique inode identifier
pub type Ino = u64;

/// Root inode is always 1 in FUSE
pub const ROOT_INO: Ino = 1;

/// File attributes mirroring stat(2)
#[derive(Clone, Debug)]
pub struct FileAttr {
    pub ino: Ino,
    pub size: u64,
    pub blocks: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub crtime: SystemTime,
    pub kind: FileKind,
    pub perm: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
}

impl FileAttr {
    #[allow(dead_code)]
    pub fn new_file(ino: Ino, uid: u32, gid: u32) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            size: 0,
            blocks: 0,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
            kind: FileKind::File,
            perm: 0o644,
            nlink: 1,
            uid,
            gid,
        }
    }

    #[allow(dead_code)]
    pub fn new_dir(ino: Ino, uid: u32, gid: u32) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            size: 0,
            blocks: 0,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
            kind: FileKind::Directory,
            perm: 0o755,
            nlink: 2,
            uid,
            gid,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
}

/// Reference to a Python-owned node
pub enum NodeRef {
    File(Py<PyFile>),
    Dir(Py<PyDirectory>),
    Symlink(Py<PySymlink>),
}

impl NodeRef {
    #[allow(dead_code)]
    pub fn kind(&self) -> FileKind {
        match self {
            NodeRef::File(_) => FileKind::File,
            NodeRef::Dir(_) => FileKind::Directory,
            NodeRef::Symlink(_) => FileKind::Symlink,
        }
    }

    #[allow(dead_code)]
    pub fn clone_ref(&self, py: Python<'_>) -> Self {
        match self {
            NodeRef::File(f) => NodeRef::File(f.clone_ref(py)),
            NodeRef::Dir(d) => NodeRef::Dir(d.clone_ref(py)),
            NodeRef::Symlink(s) => NodeRef::Symlink(s.clone_ref(py)),
        }
    }
}

/// A file in the filesystem, backed by Python-owned memory
#[pyclass(name = "File")]
pub struct PyFile {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get, set)]
    pub content: Py<PyBytes>,
    #[pyo3(get, set)]
    pub mode: u16,
    pub(crate) ino: Ino,
    pub(crate) parent_ino: Ino,
    pub(crate) mtime: SystemTime,
    pub(crate) atime: SystemTime,
    pub(crate) ctime: SystemTime,
}

#[pymethods]
impl PyFile {
    #[new]
    #[pyo3(signature = (name, content=None, mode=0o644))]
    pub fn new(py: Python<'_>, name: String, content: Option<&[u8]>, mode: u16) -> PyResult<Self> {
        let data = content.unwrap_or(b"");
        let content = PyBytes::new(py, data).into();
        let now = SystemTime::now();
        Ok(Self {
            name,
            content,
            mode,
            ino: 0, // Assigned when added to filesystem
            parent_ino: 0,
            mtime: now,
            atime: now,
            ctime: now,
        })
    }

    /// Get the size of the file in bytes
    #[getter]
    fn size(&self, py: Python<'_>) -> usize {
        self.content.bind(py).as_bytes().len()
    }

    /// Read file contents as bytes
    fn read<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        self.content.bind(py).clone()
    }

    /// Write new contents to the file
    fn write(&mut self, py: Python<'_>, data: &[u8]) -> PyResult<()> {
        self.content = PyBytes::new(py, data).into();
        self.mtime = SystemTime::now();
        self.ctime = SystemTime::now();
        Ok(())
    }

    /// Truncate the file to the given size
    pub fn truncate(&mut self, py: Python<'_>, size: usize) -> PyResult<()> {
        let current = self.content.bind(py).as_bytes();
        let new_data = if size <= current.len() {
            current[..size].to_vec()
        } else {
            let mut v = current.to_vec();
            v.resize(size, 0);
            v
        };
        self.content = PyBytes::new(py, &new_data).into();
        self.mtime = SystemTime::now();
        self.ctime = SystemTime::now();
        Ok(())
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        format!(
            "File(name={:?}, size={}, mode={:#o})",
            self.name,
            self.content.bind(py).as_bytes().len(),
            self.mode
        )
    }
}

/// A directory in the filesystem
#[pyclass(name = "Directory")]
pub struct PyDirectory {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get, set)]
    pub mode: u16,
    pub(crate) ino: Ino,
    pub(crate) parent_ino: Ino,
    pub(crate) children: HashMap<String, Ino>,
    pub(crate) mtime: SystemTime,
    pub(crate) atime: SystemTime,
    pub(crate) ctime: SystemTime,
}

#[pymethods]
impl PyDirectory {
    #[new]
    #[pyo3(signature = (name, mode=0o755))]
    pub fn new(name: String, mode: u16) -> Self {
        let now = SystemTime::now();
        Self {
            name,
            mode,
            ino: 0,
            parent_ino: 0,
            children: HashMap::new(),
            mtime: now,
            atime: now,
            ctime: now,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Directory(name={:?}, children={}, mode={:#o})",
            self.name,
            self.children.len(),
            self.mode
        )
    }
}

/// A symbolic link in the filesystem
#[pyclass(name = "Symlink")]
pub struct PySymlink {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get, set)]
    pub target: String,
    pub(crate) ino: Ino,
    pub(crate) parent_ino: Ino,
    pub(crate) mtime: SystemTime,
    pub(crate) atime: SystemTime,
    pub(crate) ctime: SystemTime,
}

#[pymethods]
impl PySymlink {
    #[new]
    pub fn new(name: String, target: String) -> Self {
        let now = SystemTime::now();
        Self {
            name,
            target,
            ino: 0,
            parent_ino: 0,
            mtime: now,
            atime: now,
            ctime: now,
        }
    }

    fn __repr__(&self) -> String {
        format!("Symlink(name={:?}, target={:?})", self.name, self.target)
    }
}

/// The in-memory inode table
pub struct InodeTable {
    inodes: HashMap<Ino, NodeRef>,
    next_ino: Ino,
    pub uid: u32,
    pub gid: u32,
}

impl InodeTable {
    pub fn new(uid: u32, gid: u32) -> Self {
        Self {
            inodes: HashMap::new(),
            next_ino: ROOT_INO + 1,
            uid,
            gid,
        }
    }

    /// Initialize with a root directory
    pub fn init_root(&mut self, py: Python<'_>) -> PyResult<Py<PyDirectory>> {
        let mut root = PyDirectory::new(String::new(), 0o755);
        root.ino = ROOT_INO;
        root.parent_ino = ROOT_INO; // Root is its own parent
        let root_py = Py::new(py, root)?;
        self.inodes
            .insert(ROOT_INO, NodeRef::Dir(root_py.clone_ref(py)));
        Ok(root_py)
    }

    fn alloc_ino(&mut self) -> Ino {
        let ino = self.next_ino;
        self.next_ino += 1;
        ino
    }

    pub fn get(&self, ino: Ino) -> Option<&NodeRef> {
        self.inodes.get(&ino)
    }

    pub fn get_file(&self, ino: Ino) -> Option<&Py<PyFile>> {
        match self.inodes.get(&ino)? {
            NodeRef::File(f) => Some(f),
            _ => None,
        }
    }

    pub fn get_dir(&self, ino: Ino) -> Option<&Py<PyDirectory>> {
        match self.inodes.get(&ino)? {
            NodeRef::Dir(d) => Some(d),
            _ => None,
        }
    }

    pub fn get_symlink(&self, ino: Ino) -> Option<&Py<PySymlink>> {
        match self.inodes.get(&ino)? {
            NodeRef::Symlink(s) => Some(s),
            _ => None,
        }
    }

    /// Insert a file into a directory
    pub fn insert_file(
        &mut self,
        py: Python<'_>,
        parent_ino: Ino,
        file: Py<PyFile>,
    ) -> PyResult<Ino> {
        let ino = self.alloc_ino();

        // Update file's inode info
        {
            let mut f = file.borrow_mut(py);
            f.ino = ino;
            f.parent_ino = parent_ino;
        }

        let name = file.borrow(py).name.clone();

        // Add to parent directory
        if let Some(NodeRef::Dir(parent)) = self.inodes.get(&parent_ino) {
            let mut p = parent.borrow_mut(py);
            p.children.insert(name, ino);
            p.mtime = SystemTime::now();
            p.ctime = SystemTime::now();
        }

        self.inodes.insert(ino, NodeRef::File(file));
        Ok(ino)
    }

    /// Insert a subdirectory into a directory
    pub fn insert_dir(
        &mut self,
        py: Python<'_>,
        parent_ino: Ino,
        dir: Py<PyDirectory>,
    ) -> PyResult<Ino> {
        let ino = self.alloc_ino();

        // Update dir's inode info
        {
            let mut d = dir.borrow_mut(py);
            d.ino = ino;
            d.parent_ino = parent_ino;
        }

        let name = dir.borrow(py).name.clone();

        // Add to parent directory
        if let Some(NodeRef::Dir(parent)) = self.inodes.get(&parent_ino) {
            let mut p = parent.borrow_mut(py);
            p.children.insert(name, ino);
            p.mtime = SystemTime::now();
            p.ctime = SystemTime::now();
        }

        self.inodes.insert(ino, NodeRef::Dir(dir));
        Ok(ino)
    }

    /// Insert a symlink into a directory
    pub fn insert_symlink(
        &mut self,
        py: Python<'_>,
        parent_ino: Ino,
        symlink: Py<PySymlink>,
    ) -> PyResult<Ino> {
        let ino = self.alloc_ino();

        // Update symlink's inode info
        {
            let mut s = symlink.borrow_mut(py);
            s.ino = ino;
            s.parent_ino = parent_ino;
        }

        let name = symlink.borrow(py).name.clone();

        // Add to parent directory
        if let Some(NodeRef::Dir(parent)) = self.inodes.get(&parent_ino) {
            let mut p = parent.borrow_mut(py);
            p.children.insert(name, ino);
            p.mtime = SystemTime::now();
            p.ctime = SystemTime::now();
        }

        self.inodes.insert(ino, NodeRef::Symlink(symlink));
        Ok(ino)
    }

    /// Remove a node from the filesystem
    pub fn remove(&mut self, py: Python<'_>, ino: Ino) -> PyResult<Option<NodeRef>> {
        if let Some(node) = self.inodes.remove(&ino) {
            // Get parent and name from the node
            let (parent_ino, name) = match &node {
                NodeRef::File(f) => {
                    let f = f.borrow(py);
                    (f.parent_ino, f.name.clone())
                }
                NodeRef::Dir(d) => {
                    let d = d.borrow(py);
                    (d.parent_ino, d.name.clone())
                }
                NodeRef::Symlink(s) => {
                    let s = s.borrow(py);
                    (s.parent_ino, s.name.clone())
                }
            };

            // Remove from parent's children
            if let Some(NodeRef::Dir(parent)) = self.inodes.get(&parent_ino) {
                let mut p = parent.borrow_mut(py);
                p.children.remove(&name);
                p.mtime = SystemTime::now();
                p.ctime = SystemTime::now();
            }

            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    /// Lookup a child by name in a directory
    pub fn lookup(&self, py: Python<'_>, parent_ino: Ino, name: &str) -> Option<Ino> {
        match self.inodes.get(&parent_ino)? {
            NodeRef::Dir(d) => d.borrow(py).children.get(name).copied(),
            _ => None,
        }
    }

    /// Get file attributes for an inode
    pub fn getattr(&self, py: Python<'_>, ino: Ino) -> Option<FileAttr> {
        let node = self.inodes.get(&ino)?;
        match node {
            NodeRef::File(f) => {
                let f = f.borrow(py);
                let size = f.content.bind(py).as_bytes().len() as u64;
                Some(FileAttr {
                    ino,
                    size,
                    blocks: size.div_ceil(512),
                    atime: f.atime,
                    mtime: f.mtime,
                    ctime: f.ctime,
                    crtime: f.ctime,
                    kind: FileKind::File,
                    perm: f.mode,
                    nlink: 1,
                    uid: self.uid,
                    gid: self.gid,
                })
            }
            NodeRef::Dir(d) => {
                let d = d.borrow(py);
                Some(FileAttr {
                    ino,
                    size: 0,
                    blocks: 0,
                    atime: d.atime,
                    mtime: d.mtime,
                    ctime: d.ctime,
                    crtime: d.ctime,
                    kind: FileKind::Directory,
                    perm: d.mode,
                    nlink: 2 + d.children.len() as u32,
                    uid: self.uid,
                    gid: self.gid,
                })
            }
            NodeRef::Symlink(s) => {
                let s = s.borrow(py);
                let size = s.target.len() as u64;
                Some(FileAttr {
                    ino,
                    size,
                    blocks: 0,
                    atime: s.atime,
                    mtime: s.mtime,
                    ctime: s.ctime,
                    crtime: s.ctime,
                    kind: FileKind::Symlink,
                    perm: 0o777, // Symlinks are always 777
                    nlink: 1,
                    uid: self.uid,
                    gid: self.gid,
                })
            }
        }
    }

    /// Rename/move a node from one location to another
    pub fn rename(
        &mut self,
        py: Python<'_>,
        old_parent: Ino,
        old_name: &str,
        new_parent: Ino,
        new_name: &str,
    ) -> PyResult<()> {
        // Get the inode being moved
        let ino = self
            .lookup(py, old_parent, old_name)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyValueError, _>("Source not found"))?;

        // Remove from old parent
        if let Some(NodeRef::Dir(parent)) = self.inodes.get(&old_parent) {
            let mut p = parent.borrow_mut(py);
            p.children.remove(old_name);
            p.mtime = SystemTime::now();
            p.ctime = SystemTime::now();
        }

        // Update the node's name and parent
        match self.inodes.get(&ino) {
            Some(NodeRef::File(f)) => {
                let mut file = f.borrow_mut(py);
                file.name = new_name.to_string();
                file.parent_ino = new_parent;
                file.ctime = SystemTime::now();
            }
            Some(NodeRef::Dir(d)) => {
                let mut dir = d.borrow_mut(py);
                dir.name = new_name.to_string();
                dir.parent_ino = new_parent;
                dir.ctime = SystemTime::now();
            }
            Some(NodeRef::Symlink(s)) => {
                let mut sym = s.borrow_mut(py);
                sym.name = new_name.to_string();
                sym.parent_ino = new_parent;
                sym.ctime = SystemTime::now();
            }
            None => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "Node not found",
                ));
            }
        }

        // Add to new parent
        if let Some(NodeRef::Dir(parent)) = self.inodes.get(&new_parent) {
            let mut p = parent.borrow_mut(py);
            p.children.insert(new_name.to_string(), ino);
            p.mtime = SystemTime::now();
            p.ctime = SystemTime::now();
        }

        Ok(())
    }
}
