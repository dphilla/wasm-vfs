use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::collections::HashMap;
use lazy_static::lazy_static;
use crate::filesystem::{
    FileSystem, Inode, InodeKind, Permissions, Permission, Stat, Dirent, Dirent64
};
use std::cmp;

pub type FileDescriptor = i32;

struct OpenFileHandle {
    inode_number: u64,
    position: u64,
    append_mode: bool,
}

// Flags
const O_RDONLY: i32 = 0;
const O_WRONLY: i32 = 1;
const O_RDWR: i32 = 2;
const O_CREAT: i32 = 64;
const O_TRUNC: i32 = 512;
const O_APPEND: i32 = 1024;

const SEEK_SET: i32 = 0;
const SEEK_CUR: i32 = 1;
const SEEK_END: i32 = 2;

// Mode checks for access()
const R_OK: i32 = 4;
const W_OK: i32 = 2;
const X_OK: i32 = 1;

// TODO - proc will be its own lib, used here and with /net stuff, for unix Process mgmt stuff, etc
-// Proc is intended to be instantiated from a separate Wasm module
-// however it can be used local to this module if not sharing with
-// other externel resource modules (wasm-net, etc.). This is an option.
pub struct Proc {
    pub fs: FileSystem,

    // Instead of storing Inode here, we just store the inode number.
    // In a real FS, the inode number acts like an index; we can always reference fs.inodes.
    // (change this?)
    fd_table: [Option<u64>; 1024],
    open_files: std::collections::HashMap<FileDescriptor, OpenFileHandle>,
    next_fd: FileDescriptor,
    umask_value: u32,
}

impl Proc {
    pub fn new() -> Self {
        Self {
            fs: FileSystem::new(),
            fd_table: [None; 1024],
            open_files: HashMap::new(),
            next_fd: 3,
            umask_value: 0o022,
        }
    }

    // Find the next available FD
    fn allocate_fd(&mut self) -> Option<FileDescriptor> {
        for (index, slot) in self.fd_table.iter().enumerate() {
            if slot.is_none() && index >= 3 {
                return Some(index as FileDescriptor);
            }
        }
        None
    }

    fn get_absolute_path(&self, path: &PathBuf) -> PathBuf {
        if path.is_absolute() {
            path.clone()
        } else {
            self.fs.current_directory.join(path)
        }
    }

    fn get_inode_mut(&mut self, inode_num: u64) -> Option<&mut Inode> {
        self.fs.inodes.get_mut(inode_num as usize)
    }

    fn get_inode(&self, inode_num: u64) -> Option<&Inode> {
        self.fs.inodes.get(inode_num as usize)
    }

    fn set_file_size(&mut self, inode_number: u64, new_size: usize) {
        if let Some(data) = self.fs.files.get_mut(&inode_number) {
            if data.len() > new_size {
                data.truncate(new_size);
            } else if data.len() < new_size {
                data.resize(new_size, 0);
            }
            if let Some(inode) = self.get_inode_mut(inode_number) {
                inode.size = data.len() as u64;
            }
        }
    }

    fn check_access(&self, inode: &Inode, mode: i32) -> bool {
        // For simplicity, we assume root.
        let perm = &inode.permissions;
        let p = &perm.owner;
        let can_read = p.read;
        let can_write = p.write;
        let can_exec = p.execute;

        if (mode & R_OK) != 0 && !can_read {
            return false;
        }
        if (mode & W_OK) != 0 && !can_write {
            return false;
        }
        if (mode & X_OK) != 0 && !can_exec {
            return false;
        }

        true
    }

    fn insert_directory_entry(&mut self, path: &PathBuf, inode_number: u64, kind: InodeKind) {
        let inode = Inode::new(
            inode_number,
            0,
            Permissions::from((0o777 & !self.umask_value) as u16),
            0,
            0,
            0,
            0,
            0,
            kind,
        );
        while self.fs.inodes.len() <= inode_number as usize {
            self.fs.inodes.push(Inode::default());
        }
        self.fs.inodes[inode_number as usize] = inode.clone();
        self.fs.path_map.insert(path.clone(), inode_number);
        if let InodeKind::Directory = inode.kind {
            self.fs.files.insert(inode_number, vec![]);
        } else if let InodeKind::File = inode.kind {
            self.fs.files.insert(inode_number, vec![]);
        }
    }
}

lazy_static! {
    static ref GLOBAL_PROC: Mutex<Option<Proc>> = Mutex::new(None);
}

fn get_or_init_proc() -> MutexGuard<'static, Option<Proc>> {
    let mut proc = GLOBAL_PROC.lock().unwrap();
    if proc.is_none() {
        *proc = Some(Proc::new());
    }
    proc
}

// convert InodeKind to a dirent d_type
fn inode_kind_to_dtype(kind: &InodeKind) -> u8 {
    match kind {
        InodeKind::File => 8,         // DT_REG
        InodeKind::Directory => 4,    // DT_DIR
        InodeKind::SymbolicLink(_) => 10, // DT_LNK
    }
}

fn fill_stat_from_inode(inode: &Inode, statbuf: *mut Stat) {
    let mode_type = match inode.kind {
        InodeKind::File => 0o100000,       // regular file
        InodeKind::Directory => 0o040000,  // directory
        InodeKind::SymbolicLink(_) => 0o120000, // symlink
    };
    let perms = &inode.permissions;
    let mut mode_perms = 0;
    // owner
    if perms.owner.read { mode_perms |= 0o400; }
    if perms.owner.write { mode_perms |= 0o200; }
    if perms.owner.execute { mode_perms |= 0o100; }
    // group
    if perms.group.read { mode_perms |= 0o040; }
    if perms.group.write { mode_perms |= 0o020; }
    if perms.group.execute { mode_perms |= 0o010; }
    // other
    if perms.other.read { mode_perms |= 0o004; }
    if perms.other.write { mode_perms |= 0o002; }
    if perms.other.execute { mode_perms |= 0o001; }

    let st_mode = mode_type | mode_perms;

    unsafe {
        (*statbuf).st_dev = 0;
        (*statbuf).st_ino = inode.number;
        (*statbuf).st_mode = st_mode;
        (*statbuf).st_nlink = 1;
        (*statbuf).st_uid = inode.user_id;
        (*statbuf).st_gid = inode.group_id;
        (*statbuf).st_rdev = 0;
        (*statbuf).st_size = inode.size as i64;
        (*statbuf).st_blksize = 4096;
        (*statbuf).st_blocks = (inode.size as i64 + 511)/512;
        (*statbuf).st_atime = inode.atime as i64;
        (*statbuf).st_mtime = inode.mtime as i64;
        (*statbuf).st_ctime = inode.ctime as i64;
    }
}

#[no_mangle]
pub unsafe extern "C" fn init_proc(_size: u32) -> *const u8 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn open(path: *const i8, flags: i32, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };
    let path_buf = PathBuf::from(path_str);

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let should_create = (flags & O_CREAT) == O_CREAT;
    let should_truncate = (flags & O_TRUNC) == O_TRUNC;
    let append_mode = (flags & O_APPEND) == O_APPEND;

    let inode_number = if let Some(inode_num) = proc.fs.lookup_inode_by_path(&path_buf) {
        // File exists
        inode_num
    } else if should_create {
        // Create a new file
        proc.fs.create_file(&path_buf, mode)
    } else {
        return -1; // File not found and O_CREAT not set
    };

    // Truncate if requested
    if should_truncate {
        if let Some(data) = proc.fs.files.get_mut(&inode_number) {
            data.clear();
        }
    } else {
        // Ensure file data exists
        if !proc.fs.files.contains_key(&inode_number) {
            proc.fs.files.insert(inode_number, vec![]);
        }
    }

    // Allocate FD
    let fd = proc.allocate_fd();
    let fd = match fd {
        Some(fd) => fd,
        None => return -1,
    };

    proc.fd_table[fd as usize] = Some(inode_number);
    proc.open_files.insert(
        fd,
        OpenFileHandle {
            inode_number,
            position: if append_mode {
                proc.fs.files.get(&inode_number).unwrap().len() as u64
            } else {
                0
            },
            append_mode,
        },
    );

    fd
}

#[no_mangle]
pub extern "C" fn close(fd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    if (fd as usize) < proc.fd_table.len() && proc.fd_table[fd as usize].is_some() {
        proc.fd_table[fd as usize] = None;
        proc.open_files.remove(&fd);
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn creat(path: *const i8, mode: u32) -> i32 {
    open(path, O_WRONLY | O_CREAT | O_TRUNC, mode)
}

#[no_mangle]
pub extern "C" fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let handle = match proc.open_files.get_mut(&fd) {
        Some(h) => h,
        None => return -1,
    };

    let data = match proc.fs.files.get(&handle.inode_number) {
        Some(d) => d,
        None => return -1,
    };

    let position = handle.position as usize;
    if position >= data.len() {
        return 0; // EOF
    }

    let to_read = cmp::min(count, data.len() - position);
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr().add(position), buf, to_read);
    }

    handle.position += to_read as u64;
    to_read as isize
}

#[no_mangle]
pub extern "C" fn write(fd: i32, buf: *const u8, count: usize) -> isize {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let handle = match proc.open_files.get_mut(&fd) {
        Some(h) => h,
        None => return -1,
    };

    let data = proc.fs.files.get_mut(&handle.inode_number).unwrap();

    if handle.append_mode {
        handle.position = data.len() as u64;
    }

    let position = handle.position as usize;

    if position + count > data.len() {
        data.resize(position + count, 0);
    }

    unsafe {
        std::ptr::copy_nonoverlapping(buf, data.as_mut_ptr().add(position), count);
    }

    handle.position += count as u64;

    // Update inode size
    if let Some(inode) = proc.fs.inodes.get_mut(handle.inode_number as usize) {
        inode.size = data.len() as u64;
    }

    count as isize
}

#[no_mangle]
pub extern "C" fn openat(_dirfd: i32, pathname: *const i8, flags: i32, mode: u32) -> i32 {
    open(pathname, flags, mode)
}

#[no_mangle]
pub extern "C" fn dup(oldfd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    if (oldfd as usize) < proc.fd_table.len() {
        if let Some(inode_number) = proc.fd_table[oldfd as usize] {
            if let Some(old_handle) = proc.open_files.get(&oldfd) {
                if let Some(new_fd) = proc.allocate_fd() {
                    proc.fd_table[new_fd as usize] = Some(inode_number);
                    let new_handle = OpenFileHandle {
                        inode_number: old_handle.inode_number,
                        position: old_handle.position,
                        append_mode: old_handle.append_mode,
                    };
                    proc.open_files.insert(new_fd, new_handle);
                    return new_fd;
                }
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn dup2(oldfd: i32, newfd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    if (oldfd as usize) < proc.fd_table.len() && (newfd as usize) < proc.fd_table.len() {
        if let Some(inode_number) = proc.fd_table[oldfd as usize] {
            if oldfd == newfd {
                return newfd;
            }

            if proc.fd_table[newfd as usize].is_some() {
                proc.fd_table[newfd as usize] = None;
                proc.open_files.remove(&newfd);
            }

            proc.fd_table[newfd as usize] = Some(inode_number);

            if let Some(old_handle) = proc.open_files.get(&oldfd) {
                let new_handle = OpenFileHandle {
                    inode_number: old_handle.inode_number,
                    position: old_handle.position,
                    append_mode: old_handle.append_mode,
                };
                proc.open_files.insert(newfd, new_handle);
            }

            return newfd;
        }
    }
    -1
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

