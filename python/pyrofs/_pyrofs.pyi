# python/pyrofs/_pyrofs.pyi
"""Type stubs for the _pyrofs native module"""

from typing import Optional, Literal, Union
from types import TracebackType

class MountHandle:
    """Handle to a mounted filesystem - used as context manager"""

    @property
    def mount_point(self) -> str:
        """Get the mount point path"""
        ...

    @property
    def is_mounted(self) -> bool:
        """Check if the filesystem is still mounted"""
        ...

    def __enter__(self) -> "MountHandle": ...
    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: Optional[TracebackType],
    ) -> Literal[False]: ...
    def unmount(self) -> None:
        """Unmount the filesystem"""
        ...

class File:
    """Represents a file in the virtual filesystem"""

    @property
    def name(self) -> str:
        """The name of the file"""
        ...

    @property
    def size(self) -> int:  # MISSING
        """The size of the file in bytes"""
        ...

    @property
    def content(self) -> bytes:
        """The content of the file"""
        ...

    @content.setter
    def content(self, value: bytes) -> None:
        """Set the content of the file"""
        ...

    @property
    def mode(self) -> int:
        """The file mode/permissions"""
        ...

    @mode.setter
    def mode(self, value: int) -> None:
        """Set the file mode/permissions"""
        ...

    # MISSING methods from your tests
    def read(self) -> bytes:
        """Read the entire content of the file"""
        ...

    def write(self, content: bytes) -> None:
        """Write content to the file, replacing existing content"""
        ...

    def truncate(self, size: int) -> None:
        """Truncate or extend the file to the specified size"""
        ...

    def __repr__(self) -> str: ...

class Directory:
    """Represents a directory in the virtual filesystem"""

    @property
    def name(self) -> str:
        """The name of the directory"""
        ...

    @property
    def mode(self) -> int:
        """The directory mode/permissions"""
        ...

    @mode.setter
    def mode(self, value: int) -> None:
        """Set the directory mode/permissions"""
        ...

    # MISSING - if you expose children
    @property
    def children(self) -> dict[str, Union[File, Directory, Symlink]]:  # If exposed
        """The children of this directory"""
        ...

    def __repr__(self) -> str: ...

class Symlink:
    """Represents a symbolic link in the virtual filesystem"""

    @property
    def name(self) -> str:
        """The name of the symlink"""
        ...

    @property
    def target(self) -> str:
        """The target path of the symlink"""
        ...

    def __repr__(self) -> str: ...

class MemFS:
    """The main filesystem object for managing an in-memory FUSE filesystem"""

    def __init__(self) -> None:
        """Create a new in-memory filesystem"""
        ...

    @property
    def root(self) -> Directory:
        """Get the root directory"""
        ...

    def create_file(
        self,
        path: str,
        content: Optional[bytes] = None,
        mode: int = 0o644,
    ) -> File:
        """
        Create a file in the filesystem

        Args:
            path: The path where the file should be created
            content: Optional initial content for the file
            mode: File permissions (default: 0o644)

        Returns:
            The created File object

        Raises:
            ValueError: If the file already exists or parent directory doesn't exist
        """
        ...

    def create_dir(self, path: str, mode: int = 0o755) -> Directory:
        """
        Create a directory in the filesystem

        Args:
            path: The path where the directory should be created
            mode: Directory permissions (default: 0o755)

        Returns:
            The created Directory object

        Raises:
            ValueError: If the directory already exists or parent doesn't exist
        """
        ...

    def makedirs(self, path: str, mode: int = 0o755) -> Directory:
        """
        Create directories recursively (like mkdir -p)

        Args:
            path: The path of directories to create
            mode: Directory permissions (default: 0o755)

        Returns:
            The final Directory object created

        Raises:
            ValueError: If a path component is a file
        """
        ...

    def get(self, path: str) -> Union[File, Directory, Symlink]:
        """
        Get a file, directory, or symlink by path

        Args:
            path: The path to look up

        Returns:
            The File, Directory, or Symlink object at that path

        Raises:
            ValueError: If the path doesn't exist
        """
        ...

    def exists(self, path: str) -> bool:
        """
        Check if a path exists

        Args:
            path: The path to check

        Returns:
            True if the path exists, False otherwise
        """
        ...

    def symlink(self, target: str, path: str) -> Symlink:
        """
        Create a symbolic link

        Args:
            target: The target path the symlink should point to
            path: The path where the symlink should be created

        Returns:
            The created Symlink object

        Raises:
            ValueError: If the path already exists
        """
        ...

    def readlink(self, path: str) -> str:
        """
        Read the target of a symbolic link

        Args:
            path: The path to the symlink

        Returns:
            The target path of the symlink

        Raises:
            ValueError: If the path is not a symlink
        """
        ...

    def is_symlink(self, path: str) -> bool:
        """
        Check if path is a symlink

        Args:
            path: The path to check

        Returns:
            True if the path is a symlink, False otherwise
        """
        ...

    def remove_file(self, path: str) -> None:
        """
        Remove a file or symlink

        Args:
            path: The path to remove

        Raises:
            ValueError: If the path is a directory or doesn't exist
        """
        ...

    def remove_dir(self, path: str) -> None:
        """
        Remove a directory (must be empty)

        Args:
            path: The path to the directory to remove

        Raises:
            ValueError: If the directory is not empty, is a file, or doesn't exist
        """
        ...

    def listdir(self, path: str) -> list[str]:
        """
        List contents of a directory

        Args:
            path: The path to the directory

        Returns:
            A list of names in the directory

        Raises:
            ValueError: If the path is not a directory
        """
        ...

    def mount(self, mount_point: str, allow_other: bool = False) -> MountHandle:
        """
        Mount the filesystem at the given path

        Args:
            mount_point: The path where the filesystem should be mounted
            allow_other: Whether to allow other users to access the mount

        Returns:
            A MountHandle that can be used as a context manager

        Raises:
            OSError: If the mount point doesn't exist or mounting fails
        """
        ...

    def rename(self, old_path: str, new_path: str) -> None:
        """
        Rename/move a file or directory

        Args:
            old_path: The current path
            new_path: The new path

        Raises:
            ValueError: If the source doesn't exist or the operation is invalid
        """
        ...

    def __repr__(self) -> str: ...
