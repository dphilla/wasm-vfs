# wasm-vfs

A Wasm-first Virtualized Filesystem

## Description

Am initial experimental implementation of a Virtual Filesystem with Syscall-like interfaces

Can be used with [wasm-libc](https://github.com/dphilla/wasm-libc) for (very basic) Wasm-first file i/o operations

**Not yet for production use - use at own risk**

## Implemented Calls (planned)

### File Descriptor Management
- `open`: Opens a file and returns a file descriptor.
- `close`: Closes a file descriptor.
- `creat`: Creates a new file or rewrites an existing one, returning a file descriptor.
- `openat`: Opens a file relative to a directory file descriptor.
- `dup`: Duplicates a file descriptor.
- `dup2`: Duplicates a file descriptor to a specific value.

### Reading and Writing
- `read`: Reads data from a file descriptor.
- `write`: Writes data to a file descriptor.
- `pread64`: Reads data from a file descriptor at a specific offset, without changing the file offset.
- `pwrite64`: Writes data to a file descriptor at a specific offset, without changing the file offset.
- `sendfile`: Transfers data between two file descriptors.
- `sendfile64`: Like sendfile but for large files.
- `splice`: Moves data from one file descriptor to another.
- `vmsplice`: Transfers data between user space and a pipe.
- `readlink`: Reads the value of a symbolic link.
- `readlinkat`: Like readlink but relative to a directory file descriptor.
- `getdents`: This system call reads the contents of a directory into a buffer. It returns multiple directory entries in a single system call.
- `getdents64`: Similar to getdents, but provides a larger structure for directory entries, allowing for larger filenames and additional metadata.

### Position and Status
- `lseek`: Changes the file offset for a file descriptor.
- `stat`: Gets file status.
- `fstat`: Gets file status for a file descriptor.
- `lstat`: Gets file status, but does not follow symbolic links.
- `fstatat`: Like stat but relative to a directory file descriptor.
- `getcwd`: Gets the current working directory.
- `chdir`: Changes the current working directory.
- `fchdir`: Changes the current working directory to the one associated with a file descriptor.

### Permissions and Ownership
- `chmod`: Changes permissions of a file.
- `fchmod`: Changes permissions of a file given by a file descriptor.
- `fchmodat`: Like chmod but relative to a directory file descriptor.
- `chown`: Changes ownership of a file.
- `lchown`: Changes ownership of a file, but does not follow symbolic links.
- `fchown`: Changes ownership of a file given by a file descriptor.
- `fchownat`: Like chown but relative to a directory file descriptor.
- `access`: Checks file permissions for the calling process.
- `faccessat`: Like access but relative to a directory file descriptor.
- `umask`: Sets the calling process's file mode creation mask.

### File Manipulation
- `rename`: Renames or moves a file within a filesystem.
- `renameat`: Like rename but relative to directory file descriptors.
- `renameat2`: Like renameat.
- `link`: Creates a new hard link to an existing file.
- `linkat`: Like link but relative to directory file descriptors.
- `unlink`: Deletes a name from the filesystem. If this name was the last link to a file and no processes have it open, the file is deleted.
- `unlinkat`: Like unlink but relative to a directory file descriptor.
- `symlink`: Creates a new symbolic link.
- `symlinkat`: Like symlink but relative to a directory file descriptor.
- `readlink`: Reads the value of a symbolic link.
- `readlinkat`: Like readlink but relative to a directory file descriptor.
- `mkdir`: Creates a new directory.
- `mkdirat`: Like mkdir but relative to a directory file descriptor.
- `rmdir`: Deletes a directory.
- `truncate`: Changes the size of a file to a specific value.
- `ftruncate`: Like truncate but operates on a file descriptor.
- `fallocate`: Allocates or deallocates space to a file descriptor.
- `posix_fallocate`: Allocates space to a file descriptor, unlike fallocate, this is a synchronous operation.
- `flock`: Apply or remove an advisory lock on the open file referred to by the file descriptor.

### Memory Mapping
- `mmap`: Maps a file into memory.
- `munmap`: Unmaps a file from memory.
- `mprotect`: Sets protection on a region of memory.
- `mlock`: Prevents memory from being paged to disk.
- `munlock`: Unlocks memory previously locked with mlock.
- `msync`: Synchronizes a region of mapped memory.

## Future Implementations

### File Notification
- `inotify_init`: Initializes an instance of inotify.
- `inotify_init1`: Like inotify_init but with additional flags.
- `inotify_add_watch`: Adds a watch to an inotify instance.
- `inotify_rm_watch`: Removes a watch from an inotify instance.

### File Synchronization
- `sync`: This system call causes all pending modifications to filesystem metadata and data to be written out to the disk. This ensures that the state of the filesystem matches what the system has in its buffers, but it operates on the whole system, which can be a broad stroke if you're focused on a specific file or filesystem.
- `fsync`: This system call is similar to sync, but it's more specific—it only affects the file referred to by the file descriptor passed to it. It causes all buffered data for that file to be written to the disk (or other permanent storage).
- `fdatasync`: This is similar to fsync, but it only writes out the file's data, not its metadata (unless the metadata is needed to retrieve the data). This can be a bit faster than fsync if the metadata hasn't changed.
- `syncfs`: This system call is again similar to sync and fsync, but it operates on a specific filesystem. You give it a file descriptor, and it writes out all buffered data for that filesystem. It's a middle ground between the very specific fsync and the very general sync.

