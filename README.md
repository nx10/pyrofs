# pyrofs

A Python library for mounting in-memory filesystems via FUSE. Create files and directories in Python, mount them as a real filesystem, and let external tools read/write them directly.

## Installation

Requires Linux with FUSE.

```bash
# Linux
sudo apt-get install libfuse3-dev fuse3
```

```bash
pip install pyrofs
```

## Usage

```python
from pyrofs import MemFS

fs = MemFS()

# Create files and directories
fs.create_file("/input.csv", b"id,value\n1,100\n2,200")
fs.create_dir("/output")
fs.symlink("/input.csv", "/data.csv")

# Mount and use with external tools
with fs.mount("/tmp/data") as mount:
    import subprocess
    subprocess.run(["some-legacy-tool", "--input", "/tmp/data/input.csv"])
    
    # Read results back
    result = fs.get("/output/result.csv").read()
```

Changes sync bidirectionally—writes from external tools are immediately visible in Python and vice versa.

## API

### MemFS

```python
fs = MemFS()

# Files
fs.create_file(path, content=None, mode=0o644) -> File
fs.get(path) -> File | Directory | Symlink
fs.exists(path) -> bool
fs.remove_file(path)
fs.rename(old_path, new_path)

# Directories  
fs.create_dir(path, mode=0o755) -> Directory
fs.makedirs(path, mode=0o755) -> Directory
fs.listdir(path) -> list[str]
fs.remove_dir(path)

# Symlinks
fs.symlink(target, path) -> Symlink
fs.readlink(path) -> str
fs.is_symlink(path) -> bool

# Mount
fs.mount(mount_point, allow_other=False) -> MountHandle
```

### File

```python
file.name      # filename
file.size      # size in bytes
file.mode      # permission bits
file.read()    # returns bytes
file.write(data)
file.truncate(size)
```

## Requirements

- Python 3.9+
- Linux with FUSE 2.6+
- For development: Rust toolchain, maturin

## License

MIT