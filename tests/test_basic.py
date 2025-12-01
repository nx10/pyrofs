"""Tests for pyrofs."""

import os
import tempfile
import time
import pytest


def test_import():
    """Test that the module can be imported."""
    from pyrofs import MemFS, File, Directory

    assert MemFS is not None
    assert File is not None
    assert Directory is not None


def test_create_filesystem():
    """Test creating an empty filesystem."""
    from pyrofs import MemFS

    fs = MemFS()
    assert fs.root is not None


def test_create_file():
    """Test creating a file."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/test.txt", b"Hello, World!")

    assert f.name == "test.txt"
    assert f.size == 13
    assert f.read() == b"Hello, World!"


def test_create_file_empty():
    """Test creating an empty file."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/empty.txt")

    assert f.name == "empty.txt"
    assert f.size == 0
    assert f.read() == b""


def test_create_directory():
    """Test creating a directory."""
    from pyrofs import MemFS

    fs = MemFS()
    d = fs.create_dir("/subdir")

    assert d.name == "subdir"


def test_create_nested_file():
    """Test creating a file in a subdirectory."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_dir("/subdir")
    f = fs.create_file("/subdir/nested.txt", b"Nested content")

    assert f.name == "nested.txt"
    assert f.read() == b"Nested content"


def test_makedirs():
    """Test recursive directory creation."""
    from pyrofs import MemFS

    fs = MemFS()
    d = fs.makedirs("/a/b/c/d")

    assert d.name == "d"
    assert fs.exists("/a")
    assert fs.exists("/a/b")
    assert fs.exists("/a/b/c")
    assert fs.exists("/a/b/c/d")


def test_get_file():
    """Test getting a file by path."""
    from pyrofs import MemFS, File

    fs = MemFS()
    fs.create_file("/test.txt", b"Content")

    f = fs.get("/test.txt")
    assert isinstance(f, File)
    assert f.read() == b"Content"


def test_get_directory():
    """Test getting a directory by path."""
    from pyrofs import MemFS, Directory

    fs = MemFS()
    fs.create_dir("/mydir")

    d = fs.get("/mydir")
    assert isinstance(d, Directory)


def test_exists():
    """Test path existence check."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/exists.txt", b"I exist")

    assert fs.exists("/exists.txt")
    assert not fs.exists("/does_not_exist.txt")


def test_listdir():
    """Test directory listing."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/a.txt", b"a")
    fs.create_file("/b.txt", b"b")
    fs.create_dir("/subdir")

    contents = fs.listdir("/")
    assert set(contents) == {"a.txt", "b.txt", "subdir"}


def test_remove_file():
    """Test file removal."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/temp.txt", b"temporary")

    assert fs.exists("/temp.txt")
    fs.remove_file("/temp.txt")
    assert not fs.exists("/temp.txt")


def test_remove_dir():
    """Test directory removal."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_dir("/emptydir")

    assert fs.exists("/emptydir")
    fs.remove_dir("/emptydir")
    assert not fs.exists("/emptydir")


def test_remove_dir_not_empty():
    """Test that removing non-empty directory fails."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_dir("/notempty")
    fs.create_file("/notempty/file.txt", b"content")

    with pytest.raises(ValueError, match="not empty"):
        fs.remove_dir("/notempty")


def test_file_write():
    """Test writing to a file."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/writable.txt", b"initial")

    f.write(b"updated content")
    assert f.read() == b"updated content"
    assert f.size == 15


def test_file_truncate():
    """Test truncating a file."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/truncate.txt", b"Hello, World!")

    f.truncate(5)
    assert f.read() == b"Hello"
    assert f.size == 5


def test_file_truncate_extend():
    """Test truncating a file to a larger size."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/extend.txt", b"Hi")

    f.truncate(10)
    assert f.size == 10
    assert f.read() == b"Hi\x00\x00\x00\x00\x00\x00\x00\x00"


def test_file_mode():
    """Test file permissions."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/perms.txt", b"", mode=0o600)

    assert f.mode == 0o600


def test_directory_mode():
    """Test directory permissions."""
    from pyrofs import MemFS

    fs = MemFS()
    d = fs.create_dir("/private", mode=0o700)

    assert d.mode == 0o700


def test_duplicate_file():
    """Test that creating duplicate file fails."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/dup.txt", b"first")

    with pytest.raises(ValueError, match="already exists"):
        fs.create_file("/dup.txt", b"second")


def test_duplicate_dir():
    """Test that creating duplicate directory fails."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_dir("/dupdir")

    with pytest.raises(ValueError, match="already exists"):
        fs.create_dir("/dupdir")


# FUSE mount tests - these require FUSE to be available
@pytest.fixture
def mount_dir():
    """Create a temporary directory for mounting."""
    tmpdir = tempfile.mkdtemp()
    yield tmpdir
    # Give FUSE time to fully unmount before cleanup
    for _ in range(10):
        try:
            os.rmdir(tmpdir)
            break
        except OSError:
            time.sleep(0.2)
    else:
        # Force unmount if still busy
        import subprocess

        subprocess.run(["fusermount", "-uz", tmpdir], capture_output=True)
        time.sleep(0.1)
        try:
            os.rmdir(tmpdir)
        except OSError:
            pass  # Best effort cleanup


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_mount_unmount(mount_dir):
    """Test mounting and unmounting the filesystem."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/test.txt", b"Test content")

    with fs.mount(mount_dir, allow_other=False) as handle:
        assert handle.is_mounted
        assert handle.mount_point == mount_dir

        # Give FUSE a moment to set up
        time.sleep(0.1)

        # Check that the file is visible
        mount_path = os.path.join(mount_dir, "test.txt")
        assert os.path.exists(mount_path)

        with open(mount_path, "rb") as f:
            assert f.read() == b"Test content"

    # After context exit, should be unmounted
    assert not handle.is_mounted


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_write_through_fuse(mount_dir):
    """Test writing through the FUSE mount."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/writable.txt", b"initial")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        mount_path = os.path.join(mount_dir, "writable.txt")

        # Write through FUSE
        with open(mount_path, "wb") as fh:
            fh.write(b"written through fuse")

        # Verify Python side sees the change
        assert f.read() == b"written through fuse"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_python_write_visible_in_fuse(mount_dir):
    """Test that Python writes are visible through FUSE."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/sync.txt", b"original")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        mount_path = os.path.join(mount_dir, "sync.txt")

        # Modify from Python side
        f.write(b"modified from python")

        # Read through FUSE
        with open(mount_path, "rb") as fh:
            assert fh.read() == b"modified from python"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_create_file_through_fuse(mount_dir):
    """Test creating a file through the FUSE mount."""
    from pyrofs import MemFS

    fs = MemFS()

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        mount_path = os.path.join(mount_dir, "newfile.txt")

        # Create through FUSE
        with open(mount_path, "wb") as fh:
            fh.write(b"created through fuse")

        # Verify Python side sees it
        assert fs.exists("/newfile.txt")
        f = fs.get("/newfile.txt")
        assert f.read() == b"created through fuse"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_mkdir_through_fuse(mount_dir):
    """Test creating a directory through the FUSE mount."""
    from pyrofs import MemFS

    fs = MemFS()

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        new_dir = os.path.join(mount_dir, "newdir")
        os.mkdir(new_dir)

        # Verify Python side sees it
        assert fs.exists("/newdir")


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_listdir_through_fuse(mount_dir):
    """Test listing directory through FUSE."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/a.txt", b"a")
    fs.create_file("/b.txt", b"b")
    fs.create_dir("/subdir")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        contents = os.listdir(mount_dir)
        assert set(contents) == {"a.txt", "b.txt", "subdir"}


def test_rename_file():
    """Test renaming a file via Python API."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/old.txt", b"content")

    fs.rename("/old.txt", "/new.txt")

    assert not fs.exists("/old.txt")
    assert fs.exists("/new.txt")
    assert fs.get("/new.txt").read() == b"content"


def test_rename_file_to_subdir():
    """Test moving a file to a subdirectory."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/file.txt", b"content")
    fs.create_dir("/subdir")

    fs.rename("/file.txt", "/subdir/file.txt")

    assert not fs.exists("/file.txt")
    assert fs.exists("/subdir/file.txt")
    assert fs.get("/subdir/file.txt").read() == b"content"


def test_rename_directory():
    """Test renaming a directory."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_dir("/olddir")
    fs.create_file("/olddir/file.txt", b"content")

    fs.rename("/olddir", "/newdir")

    assert not fs.exists("/olddir")
    assert fs.exists("/newdir")
    assert fs.exists("/newdir/file.txt")


def test_rename_overwrite_file():
    """Test renaming over an existing file."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/src.txt", b"new content")
    fs.create_file("/dst.txt", b"old content")

    fs.rename("/src.txt", "/dst.txt")

    assert not fs.exists("/src.txt")
    assert fs.get("/dst.txt").read() == b"new content"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_rename_through_fuse(mount_dir):
    """Test renaming through FUSE mount (atomic write pattern)."""
    from pyrofs import MemFS

    fs = MemFS()

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        # Write to temp file, then rename (atomic write pattern)
        tmp_path = os.path.join(mount_dir, "file.tmp")
        final_path = os.path.join(mount_dir, "file.txt")

        with open(tmp_path, "wb") as f:
            f.write(b"atomic content")

        os.rename(tmp_path, final_path)

        # Verify
        assert not os.path.exists(tmp_path)
        assert os.path.exists(final_path)
        with open(final_path, "rb") as f:
            assert f.read() == b"atomic content"

        # Verify Python side sees it
        assert not fs.exists("/file.tmp")
        assert fs.exists("/file.txt")
        assert fs.get("/file.txt").read() == b"atomic content"


def test_symlink_python_api():
    """Test creating and reading symlinks via Python API."""
    from pyrofs import MemFS, Symlink

    fs = MemFS()
    fs.create_file("/target.txt", b"content")

    link = fs.symlink("/target.txt", "/link.txt")

    assert isinstance(link, Symlink)
    assert link.name == "link.txt"
    assert link.target == "/target.txt"
    assert fs.is_symlink("/link.txt")
    assert not fs.is_symlink("/target.txt")
    assert fs.readlink("/link.txt") == "/target.txt"


def test_symlink_in_listdir():
    """Test that symlinks appear in directory listings."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/file.txt", b"content")
    fs.symlink("/file.txt", "/link.txt")

    contents = fs.listdir("/")
    assert set(contents) == {"file.txt", "link.txt"}


def test_remove_symlink():
    """Test removing a symlink."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/target.txt", b"content")
    fs.symlink("/target.txt", "/link.txt")

    fs.remove_file("/link.txt")

    assert not fs.exists("/link.txt")
    assert fs.exists("/target.txt")  # Target should still exist


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_symlink_through_fuse(mount_dir):
    """Test creating and reading symlinks through FUSE."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/target.txt", b"symlink target content")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        target_path = os.path.join(mount_dir, "target.txt")
        link_path = os.path.join(mount_dir, "link.txt")

        # Create symlink through FUSE
        os.symlink("/target.txt", link_path)

        # Verify it's a symlink
        assert os.path.islink(link_path)
        assert os.readlink(link_path) == "/target.txt"

        # Verify Python side sees it
        assert fs.is_symlink("/link.txt")
        assert fs.readlink("/link.txt") == "/target.txt"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_symlink_python_visible_in_fuse(mount_dir):
    """Test that symlinks created in Python are visible through FUSE."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/target.txt", b"content")
    fs.symlink("/target.txt", "/link.txt")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        link_path = os.path.join(mount_dir, "link.txt")

        assert os.path.islink(link_path)
        assert os.readlink(link_path) == "/target.txt"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_statfs(mount_dir):
    """Test that statfs works (df command)."""
    from pyrofs import MemFS

    fs = MemFS()

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        # os.statvfs should work
        stat = os.statvfs(mount_dir)
        assert stat.f_bsize > 0
        assert stat.f_blocks > 0
        assert stat.f_bfree > 0


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_fsync(mount_dir):
    """Test that fsync works."""
    from pyrofs import MemFS

    fs = MemFS()

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        file_path = os.path.join(mount_dir, "syncme.txt")

        fd = os.open(file_path, os.O_CREAT | os.O_WRONLY)
        os.write(fd, b"data to sync")
        os.fsync(fd)  # Should not raise
        os.close(fd)


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_utimens(mount_dir):
    """Test that setting file times works."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/timed.txt", b"content")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)

        file_path = os.path.join(mount_dir, "timed.txt")

        # Set specific atime/mtime
        os.utime(file_path, (1000000, 2000000))

        stat = os.stat(file_path)
        assert stat.st_atime == 1000000
        assert stat.st_mtime == 2000000


def test_concurrent_access():
    """Test that multiple threads can access the filesystem."""
    from pyrofs import MemFS
    import threading

    fs = MemFS()
    fs.create_file("/shared.txt", b"initial")

    def writer(content):
        f = fs.get("/shared.txt")
        f.write(content)

    threads = [
        threading.Thread(target=writer, args=(b"thread1",)),
        threading.Thread(target=writer, args=(b"thread2",)),
    ]

    for t in threads:
        t.start()
    for t in threads:
        t.join()

    # File should have one of the contents
    f = fs.get("/shared.txt")
    assert f.read() in [b"thread1", b"thread2"]


def test_large_file():
    """Test handling of larger files."""
    from pyrofs import MemFS

    fs = MemFS()
    # Create a 1MB file
    large_content = b"x" * (1024 * 1024)
    f = fs.create_file("/large.txt", large_content)
    assert f.size == 1024 * 1024
    assert f.read() == large_content


def test_deep_directory_nesting():
    """Test deeply nested directories."""
    from pyrofs import MemFS

    fs = MemFS()
    # Create a very deep path
    deep_path = "/" + "/".join(f"level{i}" for i in range(50))
    fs.makedirs(deep_path)
    assert fs.exists(deep_path)


def test_many_files_in_directory():
    """Test directory with many files."""
    from pyrofs import MemFS

    fs = MemFS()
    # Create 1000 files
    for i in range(1000):
        fs.create_file(f"/file{i}.txt", f"content{i}".encode())

    contents = fs.listdir("/")
    assert len(contents) == 1000


def test_rename_to_existing_directory_fails():
    """Test that renaming file to existing directory fails."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/file.txt", b"content")
    fs.create_dir("/dir")
    with pytest.raises(ValueError, match="directory"):
        fs.rename("/file.txt", "/dir")


def test_symlink_broken():
    """Test broken symlink (target doesn't exist)."""
    from pyrofs import MemFS

    fs = MemFS()
    # Create symlink to non-existent target
    link = fs.symlink("/nonexistent.txt", "/broken.txt")
    assert fs.is_symlink("/broken.txt")
    assert fs.readlink("/broken.txt") == "/nonexistent.txt"


def test_symlink_chain():
    """Test chain of symlinks."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/target.txt", b"final content")
    fs.symlink("/target.txt", "/link1.txt")
    fs.symlink("/link1.txt", "/link2.txt")
    assert fs.readlink("/link1.txt") == "/target.txt"
    assert fs.readlink("/link2.txt") == "/link1.txt"


def test_path_with_special_characters():
    """Test files with special characters in names."""
    from pyrofs import MemFS

    fs = MemFS()
    # Test various special characters
    names = [
        "file with spaces.txt",
        "file-with-dashes.txt",
        "file_with_underscores.txt",
    ]
    for name in names:
        fs.create_file(f"/{name}", b"content")
        assert fs.exists(f"/{name}")


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_multiple_sequential_mounts(mount_dir):
    """Test mounting, unmounting, and remounting the same filesystem."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/persistent.txt", b"data")

    # First mount
    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)
        assert os.path.exists(os.path.join(mount_dir, "persistent.txt"))

    # Second mount - data should still be there
    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)
        with open(os.path.join(mount_dir, "persistent.txt"), "rb") as f:
            assert f.read() == b"data"


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_remove_through_fuse(mount_dir):
    """Test removing files through FUSE."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/removeme.txt", b"content")

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)
        file_path = os.path.join(mount_dir, "removeme.txt")
        os.remove(file_path)

        # Verify removal on Python side
        assert not fs.exists("/removeme.txt")


@pytest.mark.fuse
@pytest.mark.skipif(not os.path.exists("/dev/fuse"), reason="FUSE not available")
def test_chmod_through_fuse(mount_dir):
    """Test changing permissions through FUSE."""
    from pyrofs import MemFS

    fs = MemFS()
    fs.create_file("/perms.txt", b"content", mode=0o644)

    with fs.mount(mount_dir, allow_other=False):
        time.sleep(0.1)
        file_path = os.path.join(mount_dir, "perms.txt")

        # Change permissions
        os.chmod(file_path, 0o600)

        # Verify change
        stat = os.stat(file_path)
        assert stat.st_mode & 0o777 == 0o600


def test_file_read_write_offset():
    """Test that file operations maintain content correctly."""
    from pyrofs import MemFS

    fs = MemFS()
    f = fs.create_file("/offset.txt", b"Hello, World!")

    # Multiple writes should replace content
    f.write(b"Hi")
    assert f.read() == b"Hi"
    assert f.size == 2


def test_root_is_directory():
    """Test that root behaves like a directory."""
    from pyrofs import MemFS, Directory

    fs = MemFS()
    root = fs.root
    assert isinstance(root, Directory)
    assert root.name == ""


def test_empty_path_handling():
    """Test handling of empty and root paths."""
    from pyrofs import MemFS

    fs = MemFS()

    # These should all refer to root
    assert fs.exists("/")
    assert fs.exists("")
