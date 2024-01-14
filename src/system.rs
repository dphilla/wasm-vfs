use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::MutexGuard;
use std::path::PathBuf;
use std::sync::Mutex;
use lazy_static::lazy_static;
use crate::filesystem::*;

pub type FileDescriptor = i32;

struct Proc {
    fs: FileSystem,
    //In a kernel, maybe this would fd -> vnode,
    //but since, for now, we are just dealing with
    //a VFS, were just going to use the inodes directly
    fd_table: HashMap<FileDescriptor, Inode>,
    next_fd: FileDescriptor,
}

impl Proc {
    fn new() -> Self {
        Self {
            fs: FileSystem::new(),
            fd_table: HashMap::new(),
            // start at three because ppl may be used to
            // the conventional stdin(0) stdout(1) and stderr(2).
            next_fd: 3,
        }
    }
}

lazy_static! {
    static ref GLOBAL_PROC: Mutex<Option<Proc>> = Mutex::new(None);
}

fn get_or_init_proc() -> MutexGuard<'static, Option<Proc>> {
    let mut proc = GLOBAL_PROC.lock().unwrap();
    if proc.is_none() {
        // Initialize if not already done
        *proc = Some(Proc::new());
    }
    proc
}

const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_RDWR: i32 = 2;
const O_CREAT: i32 = 64;
const O_TRUNC: i32 = 512;
const O_APPEND: i32 = 1024;

#[no_mangle]
pub extern "C" fn open(path: *const i8, flags: i32, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };
    let path_buf = PathBuf::from(path_str);

    let mut proc = get_or_init_proc(); // Assuming this function is defined

    let should_create = flags & O_CREAT == O_CREAT;
    let should_truncate = flags & O_TRUNC == O_TRUNC;
    let append_mode = flags & O_APPEND == O_APPEND;

    // File lookup or creation
    let mut inode = match proc.fs.inodes.iter().find(|inode| inode.path == path_buf) {
        Some(inode) => inode.clone(),
        None => {
            if should_create {
                // Create a new inode
                let new_inode = Inode::new(
                    proc.fs.next_inode_number,
                    0, // size
                    Permissions::from(mode as u16),
                    0, // user_id
                    0, // group_id
                    0, // ctime
                    0, // mtime
                    0, // atime
                    InodeKind::File,
                );
                proc.fs.inodes.push(new_inode.clone());
                proc.fs.next_inode_number += 1;
                new_inode
            } else {
                return -1; // File does not exist and O_CREAT is not set
            }
        }
    };

    if should_truncate && inode.kind == InodeKind::File {
        // Truncate the file (reset its size and data)
        inode.size = 0;
        // Assuming File data reset logic here
    }

    // Handle append mode
    if append_mode {
        // Logic to set the file position to the end for writing
        // Assuming File position handling logic here
    }

    // Assign a file descriptor
    let fd = proc.next_fd;
    proc.fd_table.insert(fd, inode);
    proc.next_fd += 1;

    fd
}

#[no_mangle]
pub extern "C" fn close(fd: i32) -> i32 {
    let proc = get_or_init_proc();
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn creat(path: *const i8, mode: u32) -> i32 {
    let proc = get_or_init_proc();
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn openat(dirfd: i32, pathname: *const i8, flags: i32, mode: u32) -> i32 {
    let proc = get_or_init_proc();
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn dup(oldfd: i32) -> i32 {
    let proc = get_or_init_proc();
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    let proc = get_or_init_proc();
    unimplemented!()
}

#[repr(C)]
struct Dirent {
    // ... dirent structure fields ...
}

#[repr(C)]
struct Dirent64 {
    // ... dirent64 structure fields ...
}

#[no_mangle]
pub extern "C" fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn write(fd: i32, buf: *const u8, count: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn pread64(fd: i32, buf: *mut u8, count: usize, offset: i64) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn pwrite64(fd: i32, buf: *const u8, count: usize, offset: i64) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn sendfile(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn sendfile64(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn splice(fd_in: i32, off_in: *mut i64, fd_out: i32, off_out: *mut i64, len: usize, flags: u32) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn getdents(fd: i32, dirp: *mut Dirent, count: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn getdents64(fd: i32, dirp: *mut Dirent64, count: usize) -> isize {
    unimplemented!()
}

#[repr(C)]
struct Stat {
    // ... stat structure fields ...
}

#[no_mangle]
pub extern "C" fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn stat(path: *const i8, statbuf: *mut Stat) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fstat(fd: i32, statbuf: *mut Stat) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn lstat(path: *const i8, statbuf: *mut Stat) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fstatat(dirfd: i32, pathname: *const i8, statbuf: *mut Stat, flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn getcwd(buf: *mut i8, size: usize) -> *mut i8 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn chdir(path: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fchdir(fd: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn chmod(path: *const i8, mode: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fchmod(fd: i32, mode: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fchmodat(dirfd: i32, pathname: *const i8, mode: u32, flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn chown(path: *const i8, owner: u32, group: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn lchown(path: *const i8, owner: u32, group: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fchown(fd: i32, owner: u32, group: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fchownat(dirfd: i32, pathname: *const i8, owner: u32, group: u32, flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn access(path: *const i8, mode: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn faccessat(dirfd: i32, pathname: *const i8, mode: i32, flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn umask(mask: u32) -> u32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn rename(oldpath: *const i8, newpath: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn renameat(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn renameat2(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8, flags: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn link(oldpath: *const i8, newpath: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn linkat(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8, flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn unlink(pathname: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn unlinkat(dirfd: i32, pathname: *const i8, flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn symlink(target: *const i8, linkpath: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn symlinkat(target: *const i8, newdirfd: i32, linkpath: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn readlink(path: *const i8, buf: *mut i8, bufsize: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn readlinkat(dirfd: i32, pathname: *const i8, buf: *mut i8, bufsize: usize) -> isize {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn mkdir(path: *const i8, mode: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn mkdirat(dirfd: i32, pathname: *const i8, mode: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn rmdir(path: *const i8) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn truncate(path: *const i8, length: i64) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn ftruncate(fd: i32, length: i64) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fallocate(fd: i32, mode: i32, offset: i64, len: i64) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn posix_fallocate(fd: i32, offset: i64, len: i64) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn flock(fd: i32, operation: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn mmap(addr: *mut u8, length: usize, prot: i32, flags: i32, fd: i32, offset: isize) -> *mut u8 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn munmap(addr: *mut u8, length: usize) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn mprotect(addr: *mut u8, length: usize, prot: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn mlock(addr: *const u8, len: usize) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn munlock(addr: *const u8, len: usize) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn msync(addr: *mut u8, length: usize, flags: i32) -> i32 {
    unimplemented!()
}

// ## Below: Future Implementations/Stubs

#[no_mangle]
pub extern "C" fn sync() {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fsync(fd: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn fdatasync(fd: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn syncfs(fd: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn inotify_init() -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn inotify_init1(flags: i32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn inotify_add_watch(fd: i32, pathname: *const i8, mask: u32) -> i32 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn inotify_rm_watch(fd: i32, wd: i32) -> i32 {
    unimplemented!()
}

