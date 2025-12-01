use crate::tree::{FileKind, InodeTable, NodeRef, PyDirectory, PyFile, PySymlink};
use fuser::{
    FileAttr as FuserAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEntry, ReplyOpen, ReplyWrite, Request, TimeOrNow,
};
use libc::{EEXIST, EINVAL, EISDIR, ENOENT, ENOTDIR, ENOTEMPTY};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TTL: Duration = Duration::from_secs(1);

fn system_time_to_timespec(st: SystemTime) -> (i64, u32) {
    match st.duration_since(UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, d.subsec_nanos()),
        Err(_) => (0, 0),
    }
}

fn to_fuser_attr(attr: &crate::tree::FileAttr) -> FuserAttr {
    let (atime_secs, atime_nsecs) = system_time_to_timespec(attr.atime);
    let (mtime_secs, mtime_nsecs) = system_time_to_timespec(attr.mtime);
    let (ctime_secs, ctime_nsecs) = system_time_to_timespec(attr.ctime);
    let (crtime_secs, crtime_nsecs) = system_time_to_timespec(attr.crtime);

    FuserAttr {
        ino: attr.ino,
        size: attr.size,
        blocks: attr.blocks,
        atime: UNIX_EPOCH + Duration::new(atime_secs as u64, atime_nsecs),
        mtime: UNIX_EPOCH + Duration::new(mtime_secs as u64, mtime_nsecs),
        ctime: UNIX_EPOCH + Duration::new(ctime_secs as u64, ctime_nsecs),
        crtime: UNIX_EPOCH + Duration::new(crtime_secs as u64, crtime_nsecs),
        kind: match attr.kind {
            FileKind::File => FileType::RegularFile,
            FileKind::Directory => FileType::Directory,
            FileKind::Symlink => FileType::Symlink,
        },
        perm: attr.perm,
        nlink: attr.nlink,
        uid: attr.uid,
        gid: attr.gid,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

/// The FUSE filesystem implementation that wraps the Python-owned tree
pub struct MemFs {
    pub(crate) inodes: Arc<parking_lot::Mutex<InodeTable>>,
}

impl MemFs {
    pub fn new(inodes: Arc<parking_lot::Mutex<InodeTable>>) -> Self {
        Self { inodes }
    }
}

impl Filesystem for MemFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        Python::attach(|py| {
            let inodes = self.inodes.lock();
            if let Some(ino) = inodes.lookup(py, parent, name)
                && let Some(attr) = inodes.getattr(py, ino)
            {
                reply.entry(&TTL, &to_fuser_attr(&attr), 0);
                return;
            }
            reply.error(ENOENT);
        });
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        Python::attach(|py| {
            let inodes = self.inodes.lock();
            if let Some(attr) = inodes.getattr(py, ino) {
                reply.attr(&TTL, &to_fuser_attr(&attr));
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        Python::attach(|py| {
            let inodes = self.inodes.lock();

            // Handle truncation
            if let Some(new_size) = size
                && let Some(file_py) = inodes.get_file(ino)
            {
                let mut file = file_py.borrow_mut(py);
                if file.truncate(py, new_size as usize).is_err() {
                    reply.error(EINVAL);
                    return;
                }
            }

            // Handle mode change
            if let Some(new_mode) = mode {
                match inodes.get(ino) {
                    Some(NodeRef::File(f)) => {
                        f.borrow_mut(py).mode = (new_mode & 0o7777) as u16;
                    }
                    Some(NodeRef::Dir(d)) => {
                        d.borrow_mut(py).mode = (new_mode & 0o7777) as u16;
                    }
                    Some(NodeRef::Symlink(_)) => {
                        // Symlinks don't have mode - ignore
                    }
                    None => {
                        reply.error(ENOENT);
                        return;
                    }
                }
            }

            // Handle atime/mtime changes (utimens)
            if atime.is_some() || mtime.is_some() {
                let now = SystemTime::now();
                let resolve_time = |t: TimeOrNow| -> SystemTime {
                    match t {
                        TimeOrNow::SpecificTime(st) => st,
                        TimeOrNow::Now => now,
                    }
                };

                match inodes.get(ino) {
                    Some(NodeRef::File(f)) => {
                        let mut file = f.borrow_mut(py);
                        if let Some(t) = atime {
                            file.atime = resolve_time(t);
                        }
                        if let Some(t) = mtime {
                            file.mtime = resolve_time(t);
                        }
                        file.ctime = now;
                    }
                    Some(NodeRef::Dir(d)) => {
                        let mut dir = d.borrow_mut(py);
                        if let Some(t) = atime {
                            dir.atime = resolve_time(t);
                        }
                        if let Some(t) = mtime {
                            dir.mtime = resolve_time(t);
                        }
                        dir.ctime = now;
                    }
                    Some(NodeRef::Symlink(s)) => {
                        let mut sym = s.borrow_mut(py);
                        if let Some(t) = atime {
                            sym.atime = resolve_time(t);
                        }
                        if let Some(t) = mtime {
                            sym.mtime = resolve_time(t);
                        }
                        sym.ctime = now;
                    }
                    None => {
                        reply.error(ENOENT);
                        return;
                    }
                }
            }

            if let Some(attr) = inodes.getattr(py, ino) {
                reply.attr(&TTL, &to_fuser_attr(&attr));
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        Python::attach(|py| {
            let inodes = self.inodes.lock();
            if let Some(file_py) = inodes.get_file(ino) {
                let file = file_py.borrow(py);
                let content = file.content.bind(py).as_bytes();
                let start = offset as usize;
                if start >= content.len() {
                    reply.data(&[]);
                } else {
                    let end = (start + size as usize).min(content.len());
                    reply.data(&content[start..end]);
                }
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        Python::attach(|py| {
            let inodes = self.inodes.lock();
            if let Some(file_py) = inodes.get_file(ino) {
                let mut file = file_py.borrow_mut(py);
                let current = file.content.bind(py).as_bytes().to_vec();
                let offset = offset as usize;

                // Extend if necessary
                let needed_size = offset + data.len();
                let mut new_content = if needed_size > current.len() {
                    let mut v = current;
                    v.resize(needed_size, 0);
                    v
                } else {
                    current
                };

                // Write the data
                new_content[offset..offset + data.len()].copy_from_slice(data);
                file.content = PyBytes::new(py, &new_content).into();
                file.mtime = SystemTime::now();
                file.ctime = SystemTime::now();

                reply.written(data.len() as u32);
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        Python::attach(|py| {
            let inodes = self.inodes.lock();

            if let Some(dir_py) = inodes.get_dir(ino) {
                let dir = dir_py.borrow(py);
                let mut entries: Vec<(u64, FileType, String)> = vec![
                    (ino, FileType::Directory, ".".to_string()),
                    (dir.parent_ino, FileType::Directory, "..".to_string()),
                ];

                for (name, &child_ino) in &dir.children {
                    let kind = match inodes.get(child_ino) {
                        Some(NodeRef::File(_)) => FileType::RegularFile,
                        Some(NodeRef::Dir(_)) => FileType::Directory,
                        Some(NodeRef::Symlink(_)) => FileType::Symlink,
                        None => continue,
                    };
                    entries.push((child_ino, kind, name.clone()));
                }

                for (i, (child_ino, kind, name)) in entries.iter().enumerate().skip(offset as usize)
                {
                    if reply.add(*child_ino, (i + 1) as i64, *kind, name) {
                        break;
                    }
                }
                reply.ok();
            } else {
                reply.error(ENOTDIR);
            }
        });
    }

    fn create(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        Python::attach(|py| {
            let mut inodes = self.inodes.lock();

            // Check if name already exists
            if inodes.lookup(py, parent, name).is_some() {
                reply.error(EEXIST);
                return;
            }

            // Create the file
            match PyFile::new(py, name.to_string(), None, (mode & 0o7777) as u16) {
                Ok(file) => match Py::new(py, file) {
                    Ok(file_py) => match inodes.insert_file(py, parent, file_py) {
                        Ok(ino) => {
                            if let Some(attr) = inodes.getattr(py, ino) {
                                reply.created(&TTL, &to_fuser_attr(&attr), 0, 0, 0);
                            } else {
                                reply.error(ENOENT);
                            }
                        }
                        Err(_) => reply.error(EINVAL),
                    },
                    Err(_) => reply.error(EINVAL),
                },
                Err(_) => reply.error(EINVAL),
            }
        });
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        Python::attach(|py| {
            let mut inodes = self.inodes.lock();

            // Check if name already exists
            if inodes.lookup(py, parent, name).is_some() {
                reply.error(EEXIST);
                return;
            }

            // Create the directory
            let dir = PyDirectory::new(name.to_string(), (mode & 0o7777) as u16);
            match Py::new(py, dir) {
                Ok(dir_py) => match inodes.insert_dir(py, parent, dir_py) {
                    Ok(ino) => {
                        if let Some(attr) = inodes.getattr(py, ino) {
                            reply.entry(&TTL, &to_fuser_attr(&attr), 0);
                        } else {
                            reply.error(ENOENT);
                        }
                    }
                    Err(_) => reply.error(EINVAL),
                },
                Err(_) => reply.error(EINVAL),
            }
        });
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        Python::attach(|py| {
            let mut inodes = self.inodes.lock();

            if let Some(ino) = inodes.lookup(py, parent, name) {
                // Make sure it's a file, not a directory
                if let Some(NodeRef::Dir(_)) = inodes.get(ino) {
                    reply.error(EISDIR);
                    return;
                }

                match inodes.remove(py, ino) {
                    Ok(Some(_)) => reply.ok(),
                    _ => reply.error(ENOENT),
                }
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        Python::attach(|py| {
            let mut inodes = self.inodes.lock();

            if let Some(ino) = inodes.lookup(py, parent, name) {
                // Make sure it's a directory
                match inodes.get(ino) {
                    Some(NodeRef::Dir(d)) => {
                        if !d.borrow(py).children.is_empty() {
                            reply.error(ENOTEMPTY);
                            return;
                        }
                    }
                    Some(NodeRef::File(_)) | Some(NodeRef::Symlink(_)) => {
                        reply.error(ENOTDIR);
                        return;
                    }
                    None => {
                        reply.error(ENOENT);
                        return;
                    }
                }

                match inodes.remove(py, ino) {
                    Ok(Some(_)) => reply.ok(),
                    _ => reply.error(ENOENT),
                }
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn open(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        Python::attach(|_py| {
            let inodes = self.inodes.lock();
            if inodes.get_file(ino).is_some() {
                reply.opened(0, 0);
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn release(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        Python::attach(|_py| {
            let inodes = self.inodes.lock();
            if inodes.get_dir(ino).is_some() {
                reply.opened(0, 0);
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn releasedir(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        reply.ok();
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };
        let newname = match newname.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        Python::attach(|py| {
            let mut inodes = self.inodes.lock();

            // Find the source inode
            let ino = match inodes.lookup(py, parent, name) {
                Some(ino) => ino,
                None => {
                    reply.error(ENOENT);
                    return;
                }
            };

            // Check if destination exists - if so, remove it first
            if let Some(existing_ino) = inodes.lookup(py, newparent, newname) {
                // Can't overwrite directory with file or vice versa
                let src_is_dir = matches!(inodes.get(ino), Some(NodeRef::Dir(_)));
                let dst_is_dir = matches!(inodes.get(existing_ino), Some(NodeRef::Dir(_)));

                if src_is_dir != dst_is_dir {
                    reply.error(if dst_is_dir { EISDIR } else { ENOTDIR });
                    return;
                }

                // If destination is non-empty directory, fail
                if dst_is_dir
                    && let Some(NodeRef::Dir(d)) = inodes.get(existing_ino)
                    && !d.borrow(py).children.is_empty()
                {
                    reply.error(ENOTEMPTY);
                    return;
                }

                // Remove the destination
                if inodes.remove(py, existing_ino).is_err() {
                    reply.error(EINVAL);
                    return;
                }
            }

            // Perform the rename
            if inodes.rename(py, parent, name, newparent, newname).is_err() {
                reply.error(EINVAL);
                return;
            }

            reply.ok();
        });
    }

    fn symlink(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        link: &std::path::Path,
        reply: ReplyEntry,
    ) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(EINVAL);
                return;
            }
        };
        let target = match link.to_str() {
            Some(t) => t,
            None => {
                reply.error(EINVAL);
                return;
            }
        };

        Python::attach(|py| {
            let mut inodes = self.inodes.lock();

            // Check if name already exists
            if inodes.lookup(py, parent, name).is_some() {
                reply.error(EEXIST);
                return;
            }

            // Create the symlink
            let symlink = PySymlink::new(name.to_string(), target.to_string());
            match Py::new(py, symlink) {
                Ok(symlink_py) => match inodes.insert_symlink(py, parent, symlink_py) {
                    Ok(ino) => {
                        if let Some(attr) = inodes.getattr(py, ino) {
                            reply.entry(&TTL, &to_fuser_attr(&attr), 0);
                        } else {
                            reply.error(ENOENT);
                        }
                    }
                    Err(_) => reply.error(EINVAL),
                },
                Err(_) => reply.error(EINVAL),
            }
        });
    }

    fn readlink(&mut self, _req: &Request, ino: u64, reply: fuser::ReplyData) {
        Python::attach(|py| {
            let inodes = self.inodes.lock();
            if let Some(symlink_py) = inodes.get_symlink(ino) {
                let symlink = symlink_py.borrow(py);
                reply.data(symlink.target.as_bytes());
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn statfs(&mut self, _req: &Request, _ino: u64, reply: fuser::ReplyStatfs) {
        // Return a reasonable fake statfs - 1TB free space
        reply.statfs(
            1024 * 1024 * 1024, // blocks (1B blocks = 1TB with 1K block size)
            1024 * 1024 * 1024, // bfree
            1024 * 1024 * 1024, // bavail
            1_000_000,          // files (inodes)
            1_000_000,          // ffree
            1024,               // bsize (block size)
            255,                // namelen
            0,                  // frsize (fragment size)
        );
    }

    fn fsync(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        // In-memory filesystem - nothing to sync, just return success
        reply.ok();
    }

    fn fsyncdir(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: fuser::ReplyEmpty,
    ) {
        // In-memory filesystem - nothing to sync
        reply.ok();
    }

    fn access(&mut self, _req: &Request, ino: u64, _mask: i32, reply: fuser::ReplyEmpty) {
        // Simple access check - just verify the inode exists
        Python::attach(|py| {
            let inodes = self.inodes.lock();
            if inodes.getattr(py, ino).is_some() {
                reply.ok();
            } else {
                reply.error(ENOENT);
            }
        });
    }

    fn flush(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: fuser::ReplyEmpty,
    ) {
        // In-memory filesystem - nothing to flush
        reply.ok();
    }
}
