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

    // For the uninitiated:
    // When a process creates a new file or directory (using calls like open() with O_CREAT, mkdir(), etc.), it supplies a mode argument specifying the intended permissions (for example, 0o666 for files, 0o777 for directories).
    // The actual permissions that the file or directory ends up with are the intended permissions minus the bits "masked out" by the process's umask.
    // The umask is a set of bits where each bit turned on (1) in the umask clears (removes) the corresponding permission bit from the file's final mode.
    // see: https://man7.org/linux/man-pages/man2/umask.2.html
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
#[no_mangle]
pub extern "C" fn pread64(fd: i32, buf: *mut u8, count: usize, offset: i64) -> isize {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let handle = match proc.open_files.get(&fd) {
        Some(h) => h,
        None => return -1,
    };
    let data = match proc.fs.files.get(&handle.inode_number) {
        Some(d) => d,
        None => return -1,
    };

    let position = offset as usize;
    if position >= data.len() {
        return 0;
    }
    let to_read = cmp::min(count, data.len() - position);

    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr().add(position), buf, to_read);
    }

    to_read as isize
}

#[no_mangle]
pub extern "C" fn pwrite64(fd: i32, buf: *const u8, count: usize, offset: i64) -> isize {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let handle = match proc.open_files.get(&fd) {
        Some(h) => h,
        None => return -1,
    };
    let data = proc.fs.files.get_mut(&handle.inode_number).unwrap();

    let position = offset as usize;
    if position + count > data.len() {
        data.resize(position + count, 0);
    }

    unsafe {
        std::ptr::copy_nonoverlapping(buf, data.as_mut_ptr().add(position), count);
    }

    if let Some(inode) = proc.fs.inodes.get_mut(handle.inode_number as usize) {
        inode.size = data.len() as u64;
    }

    count as isize
}

#[no_mangle]
pub extern "C" fn sendfile(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> isize {
    // sendfile copies data from in_fd to out_fd
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let in_handle = match proc.open_files.get_mut(&in_fd) {
        Some(h) => h,
        None => return -1,
    };

    let out_handle = match proc.open_files.get_mut(&out_fd) {
        Some(h) => h,
        None => return -1,
    };

    let in_data = proc.fs.files.get(&in_handle.inode_number).unwrap().clone();

    let mut read_pos = if !offset.is_null() {
        unsafe { *offset as usize }
    } else {
        in_handle.position as usize
    };

    if read_pos >= in_data.len() {
        return 0;
    }

    let to_copy = cmp::min(count, in_data.len() - read_pos);

    let out_data = proc.fs.files.get_mut(&out_handle.inode_number).unwrap();
    let out_pos = if out_handle.append_mode {
        out_data.len()
    } else {
        out_handle.position as usize
    };

    if out_pos + to_copy > out_data.len() {
        out_data.resize(out_pos + to_copy, 0);
    }

    out_data[out_pos..out_pos+to_copy].copy_from_slice(&in_data[read_pos..read_pos+to_copy]);

    // Update positions
    if !offset.is_null() {
        unsafe {
            *offset += to_copy as i64;
        }
    } else {
        in_handle.position += to_copy as u64;
    }

    if out_handle.append_mode {
        out_handle.position = out_data.len() as u64;
    } else {
        out_handle.position += to_copy as u64;
    }

    if let Some(out_inode) = proc.get_inode_mut(out_handle.inode_number) {
        out_inode.size = out_data.len() as u64;
    }

    to_copy as isize
}

#[no_mangle]
pub extern "C" fn sendfile64(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> isize {
    sendfile(out_fd, in_fd, offset, count)
}

#[no_mangle]
pub extern "C" fn splice(fd_in: i32, off_in: *mut i64, fd_out: i32, off_out: *mut i64, len: usize, _flags: u32) -> isize {
    // emulate splice as a copy similar to sendfile
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let in_handle = match proc.open_files.get(&fd_in) {
        Some(h) => h,
        None => return -1,
    };

    let out_handle = match proc.open_files.get_mut(&fd_out) {
        Some(h) => h,
        None => return -1,
    };

    let in_data = proc.fs.files.get(&in_handle.inode_number).unwrap().clone();

    let mut in_pos = if !off_in.is_null() {
        unsafe { *off_in as usize }
    } else {
        in_handle.position as usize
    };

    if in_pos >= in_data.len() {
        return 0;
    }

    let to_copy = cmp::min(len, in_data.len() - in_pos);

    let out_data = proc.fs.files.get_mut(&out_handle.inode_number).unwrap();
    let mut out_pos = if !off_out.is_null() {
        unsafe { *off_out as usize }
    } else if out_handle.append_mode {
        out_data.len()
    } else {
        out_handle.position as usize
    };

    if out_pos + to_copy > out_data.len() {
        out_data.resize(out_pos + to_copy, 0);
    }

    out_data[out_pos..out_pos+to_copy].copy_from_slice(&in_data[in_pos..in_pos+to_copy]);

    if !off_in.is_null() {
        unsafe {
            *off_in += to_copy as i64;
        }
    } else {
        // update in pos if not direct offset
        let mut_ref = &mut (proc.open_files.get_mut(&fd_in).unwrap().position);
        *mut_ref += to_copy as u64;
    }

    if !off_out.is_null() {
        unsafe {
            *off_out += to_copy as i64;
        }
    } else {
        // update out pos
        let mut_ref = &mut (proc.open_files.get_mut(&fd_out).unwrap().position);
        if out_handle.append_mode {
            *mut_ref = out_data.len() as u64;
        } else {
            *mut_ref += to_copy as u64;
        }
    }

    if let Some(out_inode) = proc.get_inode_mut(out_handle.inode_number) {
        out_inode.size = out_data.len() as u64;
    }

    to_copy as isize
}

#[no_mangle]
pub extern "C" fn getdents(fd: i32, dirp: *mut Dirent, count: usize) -> isize {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    // getdents reads directory entries from a directory fd
    let handle = match proc.open_files.get_mut(&fd) {
        Some(h) => h,
        None => return -1,
    };

    let inode = match proc.get_inode(handle.inode_number) {
        Some(i) => i,
        None => return -1,
    };

    // Ensure it's a directory
    if let InodeKind::Directory = inode.kind {
    } else {
        return -1;
    }

    // We'll simulate directory entries by enumerating all paths that start with this directory
    let dir_path = proc.fs.path_map.iter().filter(|(p, _)| {
        // directory entries are those that are direct children of the dir
        let dir_prefix = if inode.number == 0 {
            PathBuf::from("/")
        } else {
            // find path by inode
            proc.fs.path_map.iter().find_map(|(pp, ino)| if *ino == inode.number {Some(pp.clone())} else {None}).unwrap_or(PathBuf::from("/"))
        };
        if *p == dir_prefix {
            // this is the directory itself
            false
        } else {
            if let Some(parent) = p.parent() {
                parent == dir_prefix
            } else {
                false
            }
        }
    });

    // handle.position as a directory offset
    let entries: Vec<(PathBuf, u64)> = dir_path.map(|(p,i)| (p.clone(), *i)).collect();
    let start = handle.position as usize;
    if start >= entries.len() {
        return 0; // no more entries
    }

    let mut written = 0;
    let mut dirp_ptr = dirp as *mut u8;

    for (path, ino) in entries.iter().skip(start) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let d_type = inode_kind_to_dtype(&proc.get_inode(*ino).unwrap().kind);
        let mut d: Dirent = Dirent {
            d_ino: *ino,
            d_off: (handle.position + 1) as i64,
            d_reclen: 0, // will set after
            d_type,
            d_name: [0;256],
        };

        let bytes = name.as_bytes();
        if bytes.len() >= 256 {
            // name too long
            continue;
        }
        d.d_name[..bytes.len()].copy_from_slice(bytes);
        d.d_name[bytes.len()] = 0;

        let record_len = std::mem::size_of::<Dirent>();
        if written + record_len > count {
            break;
        }

        unsafe {
            std::ptr::copy_nonoverlapping(&d as *const Dirent as *const u8, dirp_ptr, record_len);
        }
        written += record_len;
        dirp_ptr = unsafe { dirp_ptr.add(record_len) };
        handle.position += 1;
    }

    written as isize
}

#[no_mangle]
pub extern "C" fn getdents64(fd: i32, dirp: *mut Dirent64, count: usize) -> isize {
    // We'll just reuse getdents and cast
    let mut buf = vec![0u8; count];
    let ret = getdents(fd, buf.as_mut_ptr() as *mut Dirent, count);
    if ret <= 0 {
        return ret;
    }

    // Convert Dirent to Dirent64 (they are the same in this simplified scenario)
    let used = ret as usize;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), dirp as *mut u8, used);
    }
    ret
}

#[no_mangle]
pub extern "C" fn lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let handle = match proc.open_files.get_mut(&fd) {
        Some(h) => h,
        None => return -1,
    };

    let size = {
        let data = proc.fs.files.get(&handle.inode_number).unwrap();
        data.len() as i64
    };

    let new_pos = match whence {
        SEEK_SET => offset,
        SEEK_CUR => handle.position as i64 + offset,
        SEEK_END => size + offset,
        _ => return -1,
    };

    if new_pos < 0 {
        return -1;
    }

    handle.position = new_pos as u64;
    new_pos
}

#[no_mangle]
pub extern "C" fn stat(path: *const i8, statbuf: *mut Stat) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    let inode = proc.get_inode(inode_num).unwrap();
    fill_stat_from_inode(inode, statbuf);
    0
}

#[no_mangle]
pub extern "C" fn fstat(fd: i32, statbuf: *mut Stat) -> i32 {
    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();

    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };

    let inode = proc.get_inode(inode_num).unwrap();
    fill_stat_from_inode(inode, statbuf);
    0
}

#[no_mangle]
pub extern "C" fn lstat(path: *const i8, statbuf: *mut Stat) -> i32 {
    // For simplicity same as stat since we don't have special link-handling difference
    stat(path, statbuf)
}

#[no_mangle]
pub extern "C" fn fstatat(dirfd: i32, pathname: *const i8, statbuf: *mut Stat, _flags: i32) -> i32 {
    // ignoring dirfd for simplicity
    stat(pathname, statbuf)
}

#[no_mangle]
pub extern "C" fn getcwd(buf: *mut i8, size: usize) -> *mut i8 {
    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();
    let cwd_str = proc.fs.current_directory.to_string_lossy();

    let bytes = cwd_str.as_bytes();
    if bytes.len() + 1 > size {
        return std::ptr::null_mut();
    }

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
        *buf.add(bytes.len()) = 0;
    }
    buf
}

#[no_mangle]
pub extern "C" fn chdir(path: *const i8) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    let inode = proc.get_inode(inode_num).unwrap();
    if let InodeKind::Directory = inode.kind {
        proc.fs.current_directory = abs_path;
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn fchdir(fd: i32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };
    let inode = proc.get_inode(inode_num).unwrap();
    if let InodeKind::Directory = inode.kind {
        // find path from inode
        let path = proc.fs.path_map.iter().find_map(|(p, i)| if *i == inode_num { Some(p.clone()) } else { None }).unwrap();
        proc.fs.current_directory = path;
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn chmod(path: *const i8, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    if let Some(inode) = proc.get_inode_mut(inode_num) {
        inode.permissions = Permissions::from((mode & 0o777) as u16);
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn fchmod(fd: i32, mode: u32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };

    if let Some(inode) = proc.get_inode_mut(inode_num) {
        inode.permissions = Permissions::from((mode & 0o777) as u16);
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn fchmodat(_dirfd: i32, pathname: *const i8, mode: u32, _flags: i32) -> i32 {
    chmod(pathname, mode)
}

#[no_mangle]
pub extern "C" fn chown(path: *const i8, owner: u32, group: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    if let Some(inode) = proc.get_inode_mut(inode_num) {
        inode.user_id = owner;
        inode.group_id = group;
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn lchown(path: *const i8, owner: u32, group: u32) -> i32 {
    chown(path, owner, group)
}

#[no_mangle]
pub extern "C" fn fchown(fd: i32, owner: u32, group: u32) -> i32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };

    if let Some(inode) = proc.get_inode_mut(inode_num) {
        inode.user_id = owner;
        inode.group_id = group;
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn fchownat(_dirfd: i32, pathname: *const i8, owner: u32, group: u32, _flags: i32) -> i32 {
    chown(pathname, owner, group)
}

#[no_mangle]
pub extern "C" fn access(path: *const i8, mode: i32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };
    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    let inode = proc.get_inode(inode_num).unwrap();
    if proc.check_access(inode, mode) {
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn faccessat(_dirfd: i32, pathname: *const i8, mode: i32, _flags: i32) -> i32 {
    access(pathname, mode)
}

#[no_mangle]
pub extern "C" fn umask(mask: u32) -> u32 {
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let old = proc.umask_value;
    proc.umask_value = mask & 0o777;
    old
}

#[no_mangle]
pub extern "C" fn rename(oldpath: *const i8, newpath: *const i8) -> i32 {
    let old_str = unsafe { CStr::from_ptr(oldpath).to_string_lossy().into_owned() };
    let new_str = unsafe { CStr::from_ptr(newpath).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let old_abs = proc.get_absolute_path(&PathBuf::from(old_str));
    let new_abs = proc.get_absolute_path(&PathBuf::from(new_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&old_abs) {
        Some(i) => i,
        None => return -1,
    };

    // remove old
    proc.fs.path_map.remove(&old_abs);
    // insert new
    proc.fs.path_map.insert(new_abs, inode_num);
    0
}

#[no_mangle]
pub extern "C" fn renameat(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8) -> i32 {
    // ignoring dirfd for simplicity
    rename(oldpath, newpath)
}

#[no_mangle]
pub extern "C" fn renameat2(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8, _flags: u32) -> i32 {
    renameat(olddirfd, oldpath, newdirfd, newpath)
}

#[no_mangle]
pub extern "C" fn link(oldpath: *const i8, newpath: *const i8) -> i32 {
    let old_str = unsafe { CStr::from_ptr(oldpath).to_string_lossy().into_owned() };
    let new_str = unsafe { CStr::from_ptr(newpath).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let old_abs = proc.get_absolute_path(&PathBuf::from(old_str));
    let new_abs = proc.get_absolute_path(&PathBuf::from(new_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&old_abs) {
        Some(i) => i,
        None => return -1,
    };
    // hard link: just add another path_map entry to the same inode.
    proc.fs.path_map.insert(new_abs, inode_num);
    0
}

#[no_mangle]
pub extern "C" fn linkat(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8, _flags: i32) -> i32 {
    link(oldpath, newpath)
}

#[no_mangle]
pub extern "C" fn unlink(pathname: *const i8) -> i32 {
    let path_str = unsafe { CStr::from_ptr(pathname).to_string_lossy().into_owned() };
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    // If directory, fail
    let inode = proc.get_inode(inode_num).unwrap();
    if let InodeKind::Directory = inode.kind {
        return -1;
    }

    proc.fs.path_map.remove(&abs_path);
    0
}

#[no_mangle]
pub extern "C" fn unlinkat(dirfd: i32, pathname: *const i8, flags: i32) -> i32 {
    // ignoring flags and dirfd
    unlink(pathname)
}

#[no_mangle]
pub extern "C" fn symlink(target: *const i8, linkpath: *const i8) -> i32 {
    let target_str = unsafe { CStr::from_ptr(target).to_string_lossy().into_owned() };
    let link_str = unsafe { CStr::from_ptr(linkpath).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let link_abs = proc.get_absolute_path(&PathBuf::from(link_str));
    let inode_number = proc.fs.next_inode_number;
    proc.fs.next_inode_number += 1;

    let kind = InodeKind::SymbolicLink(PathBuf::from(target_str));
    proc.insert_directory_entry(&link_abs, inode_number, kind);
    0
}

#[no_mangle]
pub extern "C" fn symlinkat(target: *const i8, newdirfd: i32, linkpath: *const i8) -> i32 {
    symlink(target, linkpath)
}

#[no_mangle]
pub extern "C" fn readlink(path: *const i8, buf: *mut i8, bufsize: usize) -> isize {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };
    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };
    let inode = proc.get_inode(inode_num).unwrap();
    match &inode.kind {
        InodeKind::SymbolicLink(t) => {
            let bytes = t.to_string_lossy().as_bytes();
            let to_copy = cmp::min(bytes.len(), bufsize);
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, to_copy);
            }
            to_copy as isize
        },
        _ => -1,
    }
}

#[no_mangle]
pub extern "C" fn readlinkat(dirfd: i32, pathname: *const i8, buf: *mut i8, bufsize: usize) -> isize {
    readlink(pathname, buf, bufsize)
}

#[no_mangle]
pub extern "C" fn mkdir(path: *const i8, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));

    if proc.fs.lookup_inode_by_path(&abs_path).is_some() {
        return -1; // already exists
    }

    let inode_number = proc.fs.next_inode_number;
    proc.fs.next_inode_number += 1;

    let adjusted_mode = (mode as u16) & !(proc.umask_value as u16);
    let kind = InodeKind::Directory;
    proc.insert_directory_entry(&abs_path, inode_number, kind);
    let inode = proc.get_inode_mut(inode_number).unwrap();
    inode.permissions = Permissions::from(adjusted_mode);
    0
}

#[no_mangle]
pub extern "C" fn mkdirat(dirfd: i32, pathname: *const i8, mode: u32) -> i32 {
    mkdir(pathname, mode)
}

#[no_mangle]
pub extern "C" fn rmdir(path: *const i8) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };
    let inode = proc.get_inode(inode_num).unwrap();
    match inode.kind {
        InodeKind::Directory => {
            // Check if directory is empty
            let entries = proc.fs.path_map.iter().filter(|(p,_)| {
                p.parent().map(|pp| pp == abs_path).unwrap_or(false)
            }).count();
            if entries > 0 {
                return -1; // not empty
            }
            proc.fs.path_map.remove(&abs_path);
            0
        },
        _ => -1,
    }
}

#[no_mangle]
pub extern "C" fn truncate(path: *const i8, length: i64) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy().into_owned() };
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };
    if length < 0 {
        return -1;
    }
    proc.set_file_size(inode_num, length as usize);
    0
}

#[no_mangle]
pub extern "C" fn ftruncate(fd: i32, length: i64) -> i32 {
    if length < 0 {
        return -1;
    }

    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };
    proc.set_file_size(inode_num, length as usize);
    0
}

#[no_mangle]
pub extern "C" fn fallocate(fd: i32, _mode: i32, offset: i64, len: i64) -> i32 {
    // Just ensure space
    if offset < 0 || len < 0 {
        return -1;
    }
    let mut proc = get_or_init_proc();
    let proc = proc.as_mut().unwrap();
    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };

    let end = (offset + len) as usize;
    let data = proc.fs.files.get_mut(&inode_num).unwrap();
    if data.len() < end {
        data.resize(end, 0);
    }

    if let Some(inode) = proc.get_inode_mut(inode_num) {
        inode.size = data.len() as u64;
    }

    0
}

#[no_mangle]
pub extern "C" fn posix_fallocate(fd: i32, offset: i64, len: i64) -> i32 {
    fallocate(fd, 0, offset, len)
}

#[no_mangle]
pub extern "C" fn flock(fd: i32, _operation: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn mmap(_addr: *mut u8, _length: usize, _prot: i32, _flags: i32, _fd: i32, _offset: isize) -> *mut u8 {
    // Not supported in this in-memory FS
    std::ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn munmap(_addr: *mut u8, _length: usize) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn mprotect(_addr: *mut u8, _length: usize, _prot: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn mlock(_addr: *const u8, _len: usize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn munlock(_addr: *const u8, _len: usize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn msync(_addr: *mut u8, _length: usize, _flags: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn sync() {
    // no-op
}

#[no_mangle]
pub extern "C" fn fsync(_fd: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn fdatasync(_fd: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn syncfs(_fd: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn inotify_init() -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn inotify_init1(_flags: i32) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn inotify_add_watch(_fd: i32, _pathname: *const i8, _mask: u32) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn inotify_rm_watch(_fd: i32, _wd: i32) -> i32 {
    -1
}

// -----------------------------------------------------------
// Helper function to populate/mount a host directory into wasm-vfs

#[cfg(feature = "std")]
pub fn mount_host_directory(host_path: &Path, mount_path: &Path) -> Result<(), std::io::Error> {
    use std::fs;
    let mut proc = GLOBAL_PROC.lock().unwrap();
    if proc.is_none() {
        *proc = Some(Proc::new());
    }
    let proc = proc.as_mut().unwrap();

    // Recursively copy host_path into in-memory FS at mount_path
    fn recurse(proc: &mut Proc, host: &Path, vfs_path: &Path) -> std::io::Result<()> {
        let meta = host.symlink_metadata()?;
        if meta.is_dir() {
            // mkdir
            if vfs_path != Path::new("/") {
                let inode_number = proc.fs.next_inode_number;
                proc.fs.next_inode_number += 1;
                proc.fs.path_map.insert(vfs_path.to_path_buf(), inode_number);
                let inode = Inode::new(
                    inode_number,
                    0,
                    Permissions::from((0o755) as u16),
                    0,
                    0,
                    0,
                    0,
                    0,
                    InodeKind::Directory
                );
                while proc.fs.inodes.len() <= inode_number as usize {
                    proc.fs.inodes.push(Inode::default());
                }
                proc.fs.inodes[inode_number as usize] = inode;
                proc.fs.files.insert(inode_number, vec![]);
            }
            for entry in fs::read_dir(host)? {
                let entry = entry?;
                let name = entry.file_name();
                let new_vfs_path = vfs_path.join(&name);
                recurse(proc, &entry.path(), &new_vfs_path)?;
            }
        } else if meta.is_file() {
            let inode_number = proc.fs.next_inode_number;
            proc.fs.next_inode_number += 1;
            proc.fs.path_map.insert(vfs_path.to_path_buf(), inode_number);
            let data = fs::read(host)?;
            let inode = Inode::new(
                inode_number,
                data.len() as u64,
                Permissions::from((0o644) as u16),
                0,
                0,
                0,
                0,
                0,
                InodeKind::File
            );
            while proc.fs.inodes.len() <= inode_number as usize {
                proc.fs.inodes.push(Inode::default());
            }
            proc.fs.inodes[inode_number as usize] = inode;
            proc.fs.files.insert(inode_number, data);
        } else if meta.file_type().is_symlink() {
            let target = fs::read_link(host)?;
            let inode_number = proc.fs.next_inode_number;
            proc.fs.next_inode_number += 1;
            proc.fs.path_map.insert(vfs_path.to_path_buf(), inode_number);
            let inode = Inode::new(
                inode_number,
                0,
                Permissions::from((0o777) as u16),
                0,
                0,
                0,
                0,
                0,
                InodeKind::SymbolicLink(target),
            );
            while proc.fs.inodes.len() <= inode_number as usize {
                proc.fs.inodes.push(Inode::default());
            }
            proc.fs.inodes[inode_number as usize] = inode;
        }

        Ok(())
    }

    recurse(proc, host_path, mount_path)?;

    Ok(())
}
