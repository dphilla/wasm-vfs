use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::MutexGuard;
use std::path::PathBuf;
use std::sync::Mutex;
use lazy_static::lazy_static;
use crate::filesystem::*;

pub type FileDescriptor = i32;

// TODO - proc will be its own lib, used here and with /net stuff, for unix Process mgmt stuff, etc
//
// EAch named thing is a separate Wasm module:
//
//
//                     pkc
//                     /|\
//                    / | \

//
// VFS --- Proc --- Net        ...
//   \      |      /
//    \     |     /
//         libc
//          |     (else: Interfaces to
//          |     devices, user
//          |     input, etc.)
//          |
//      User program
//
// -------------------

pub struct Proc {
    fs: FileSystem,

    //In a kernel, maybe these might be vnodes in the fd_table,
    //but since, for now, we are just dealing with
    //a VFS, were just going to use the inodes directly


    //Array is for look up speed, etc. The index is the FD
    //Max 1024 is taken from linux convention

    // Use a fixed-size array for file descriptors. Each index represents an FD.
    fd_table: [Option<Inode>; 1024],
    next_fd: FileDescriptor,
}

impl Proc {
    pub fn new() -> Self {
        Self {
            fs: FileSystem::new(),
            fd_table: [None; 1024], // Initialize all slots to None
            next_fd: 3,            // Start after stdin(0), stdout(1), stderr(2)
        }
    }

    // Helper to find the next available FD
    fn allocate_fd(&mut self) -> Option<FileDescriptor> {
        for (index, slot) in self.fd_table.iter().enumerate() {
            if slot.is_none() {
                return Some(index as FileDescriptor);
            }
        }
        None
    }
}

#[cfg_attr(target_arch = "wasm32", export_name = "init_proc")]
#[no_mangle]
pub unsafe extern "C" fn init_proc(size: u32) -> *const u8 {
    unimplemented!()
}


lazy_static! {
    static ref GLOBAL_PROC: Mutex<Option<Proc>> = Mutex::new(None);
}

// Proc is intended to be instantiated from a separate Wasm module
// however it can be used local to this module if not sharing with
// other externel resource modules (wasm-net, etc.). This is an option.
//


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

// https://man7.org/linux/man-pages/man2/open.2.html
//const O_EXCL: i32 = 128;       // 0x80
//const O_NONBLOCK: i32 = 2048;  // 0x800
//const O_SYNC: i32 = 4096;      // 0x1000
//const O_NOCTTY: i32 = 131072;  // 0x20000
//const O_NOFOLLOW: i32 = 256;   // 0x100
//const O_DIRECTORY: i32 = 65536;// 0x10000

#[no_mangle]
pub extern "C" fn open(path: *const i8, flags: i32, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };
    let path_buf = PathBuf::from(path_str);

    let mut proc = get_or_init_proc();

    let should_create = flags & O_CREAT == O_CREAT;
    let should_truncate = flags & O_TRUNC == O_TRUNC;
    let append_mode = flags & O_APPEND == O_APPEND;

    let inode = if let Some(inode) = proc.as_mut().unwrap().fs.inodes.iter().find(|inode| {
        FileSystem::construct_path(&File {
            inode: inode.number,
            data: vec![],
            position: 0,
        }) == path_buf
    }) {
        inode.clone()
    } else if should_create {
        let new_inode = Inode::new(
            proc.as_mut().unwrap().fs.next_inode_number,
            0,
            Permissions::from(mode as u16),
            0, // user_id
            0, // group_id
            0, // ctime
            0, // mtime
            0, // atime
            InodeKind::File,
        );
        proc.as_mut().unwrap().fs.inodes.push(new_inode.clone());
        proc.as_mut().unwrap().fs.next_inode_number += 1;
        new_inode
    } else {
        return -1; // File not found, and O_CREAT not set
    };

    if should_truncate {
        inode.size = 0;
    }

    // Find the next available file descriptor
    let fd = proc.as_mut().unwrap().allocate_fd();
    if let Some(fd) = fd {
        proc.as_mut().unwrap().fd_table[fd as usize] = Some(inode);
        fd
    } else {
        -1 // No available file descriptor
    }
}

#[no_mangle]
pub extern "C" fn close(fd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    if (fd as usize) < proc.fd_table.len() && proc.fd_table[fd as usize].is_some() {
        proc.fd_table[fd as usize] = None; // Clear the file descriptor slot
        0 // Success
    } else {
        -1 // Invalid file descriptor
    }
}

#[no_mangle]
pub extern "C" fn creat(path: *const i8, mode: u32) -> i32 {
    open(path, O_WRONLY | O_CREAT | O_TRUNC, mode)
}

#[no_mangle]
pub extern "C" fn openat(dirfd: i32, pathname: *const i8, flags: i32, mode: u32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let base_path = if dirfd == libc::AT_FDCWD {
        proc.fs.current_directory.clone()
    } else if (dirfd as usize) < proc.fd_table.len() {
        if let Some(inode) = &proc.fd_table[dirfd as usize] {
            if let InodeKind::Directory = inode.kind {
                FileSystem::construct_path(&File {
                    inode: inode.number,
                    data: vec![],
                    position: 0,
                })
            } else {
                return -1; // Not a directory
            }
        } else {
            return -1; // Invalid file descriptor
        }
    } else {
        return -1; // Invalid file descriptor
    };

    let path_str = unsafe { CStr::from_ptr(pathname).to_string_lossy().into_owned() };
    let full_path = base_path.join(path_str);

    open(
        CString::new(full_path.to_string_lossy().into_owned())
            .unwrap()
            .as_ptr(),
        flags,
        mode,
    )
}

#[no_mangle]
pub extern "C" fn dup(oldfd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    if (oldfd as usize) < proc.fd_table.len() {
        if let Some(inode) = &proc.fd_table[oldfd as usize] {
            let new_fd = proc.allocate_fd();
            if let Some(new_fd) = new_fd {
                proc.fd_table[new_fd as usize] = Some(inode.clone());
                return new_fd;
            }
        }
    }
    -1 // Invalid oldfd or no available new file descriptor
}

#[no_mangle]
pub extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    if (oldfd as usize) < proc.fd_table.len() && (newfd as usize) < proc.fd_table.len() {
        if let Some(inode) = &proc.fd_table[oldfd as usize] {
            proc.fd_table[newfd as usize] = Some(inode.clone());
            return newfd;
        }
    }
    -1 // Invalid oldfd or newfd
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

