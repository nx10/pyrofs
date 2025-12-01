"""
pyrofs - Mount Python-owned memory as an ephemeral FUSE filesystem.

Example usage:

    from pyrofs import MemFS

    # Create the filesystem
    fs = MemFS()

    # Add some files
    fs.create_file("/hello.txt", b"Hello, World!")
    fs.create_dir("/subdir")
    fs.create_file("/subdir/data.bin", b"\\x00\\x01\\x02\\x03")

    # Mount it
    with fs.mount("/tmp/mymount") as mount:
        # Now accessible at /tmp/mymount
        # Other processes can read/write files there

        # Changes from either side are visible to the other
        file = fs.get("/hello.txt")
        file.write(b"Updated content")

        # ... do stuff with mounted filesystem ...

    # Automatically unmounted when context exits
"""

from ._pyrofs import MemFS, File, Directory, Symlink, MountHandle

__all__ = ["MemFS", "File", "Directory", "Symlink", "MountHandle"]
__version__ = "0.1.1"
