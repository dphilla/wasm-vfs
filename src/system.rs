//use std::ffi::{CStr, CString};
//use std::path::PathBuf;
//use std::sync::{Mutex, MutexGuard};
//use std::collections::HashMap;
//use lazy_static::lazy_static;
//use std::cmp;


// In project implementations - replaces rust's std crates:

use crate::ffi::CStr;
use crate::path::PathBuf;
use crate::sync::{Mutex, MutexGuard};
use crate::collections::HashMap;
use crate::cmp::{min, max};

use crate::filesystem::{
    FileSystem, Inode, InodeKind, Permissions, Permission, Stat, Dirent, Dirent64
};

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
// Proc is intended to be instantiated from a separate Wasm module
// however it can be used local to this module if not sharing with
// other externel resource modules (wasm-net, etc.). This is an option.
//

const OPEN_FILES_CAP: usize = 256;

pub struct Proc {
    pub fs: FileSystem,

    // Instead of storing Inode here, we just store the inode number.
    // In a real FS, the inode number acts like an index; we can always reference fs.inodes.
    // (change this?)
    fd_table: [Option<u64>; 1024],
    open_files: HashMap<FileDescriptor, OpenFileHandle, OPEN_FILES_CAP>,
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
        let new_len = {
            if let Some(data) = self.fs.files.get_mut(&inode_number) {
                if data.len() > new_size {
                    data.truncate(new_size);
                } else if data.len() < new_size {
                    data.resize(new_size, 0);
                }
                data.len()
            } else {
                0
            }
        };

        if let Some(inode) = self.get_inode_mut(inode_number) {
            inode.size = new_len as u64;
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

//lazy_static! {
    //static ref GLOBAL_PROC: Mutex<Option<Proc>> = Mutex::new(None);
//}

// Replace old static line:
static mut GLOBAL_PROC: Option<Mutex<Proc>> = None;

// Then add two helper functions:
fn init_global_proc() {
    unsafe {
        if GLOBAL_PROC.is_none() {
            GLOBAL_PROC = Some(Mutex::new(Proc::new()));
        }
    }
}

fn get_or_init_proc() -> MutexGuard<'static, Proc> {
    unsafe {
        if GLOBAL_PROC.is_none() {
            GLOBAL_PROC = Some(Mutex::new(Proc::new()));
        }
        GLOBAL_PROC.as_ref().unwrap().lock()
    }
}

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
pub unsafe extern "C" fn wasm_vfs_init_proc(_size: u32) -> *const u8 {
    unimplemented!()
}

#[no_mangle]
pub extern "C" fn wasm_vfs_open(path: *const i8, flags: i32, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };
    let path_buf = PathBuf::from(path_str);

    let mut proc = get_or_init_proc();

    let should_create = (flags & O_CREAT) == O_CREAT;
    let should_truncate = (flags & O_TRUNC) == O_TRUNC;
    let append_mode = (flags & O_APPEND) == O_APPEND;

    // 1) Determine the inode_number
    let inode_number = if let Some(inode_num) = proc.fs.lookup_inode_by_path(&path_buf) {
        inode_num
    } else if should_create {
        proc.fs.create_file(&path_buf, mode)
    } else {
        return -1;
    };

    // 2) Possibly truncate
    if should_truncate {
        if let Some(data) = proc.fs.files.get_mut(&inode_number) {
            data.clear();
        }
    } else {
        if !proc.fs.files.contains_key(&inode_number) {
            proc.fs.files.insert(inode_number, vec![]);
        }
    }

    // 3) Allocate FD
    let fd = match proc.allocate_fd() {
        Some(x) => x,
        None => return -1,
    };
    proc.fd_table[fd as usize] = Some(inode_number);

    // 4) If append_mode, figure out the file’s length in a short scope
    let initial_pos = if append_mode {
        let data_len = proc
            .fs
            .files
            .get(&inode_number)
            .map(|v| v.len())
            .unwrap_or(0);
        data_len as u64
    } else {
        0
    };

    // 5) Finally insert the handle
    proc.open_files.insert(
        fd,
        OpenFileHandle {
            inode_number,
            position: initial_pos,
            append_mode,
        },
    );

    fd
}


#[no_mangle]
pub extern "C" fn wasm_vfs_close(fd: i32) -> i32 {
    let mut proc = get_or_init_proc();

    if (fd as usize) < proc.fd_table.len() && proc.fd_table[fd as usize].is_some() {
        proc.fd_table[fd as usize] = None;
        proc.open_files.remove(&fd);
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn wasm_vfs_creat(path: *const i8, mode: u32) -> i32 {
    wasm_vfs_open(path, O_WRONLY | O_CREAT | O_TRUNC, mode)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_read(fd: i32, buf: *mut u8, count: usize) -> isize {
    let mut proc = get_or_init_proc();

    let (inode_num, position) = {
        let h = match proc.open_files.get_mut(&fd) {
            Some(x) => x,
            None => return -1,
        };
        (h.inode_number, h.position)
    };

    let data = match proc.fs.files.get(&inode_num) {
        Some(d) => d,
        None => return -1,
    };
    if position as usize >= data.len() {
        return 0; // EOF
    }

    let to_read = core::cmp::min(count, data.len() - position as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr().add(position as usize), buf, to_read);
    }


    if let Some(h2) = proc.open_files.get_mut(&fd) {
        h2.position += to_read as u64;
    }

    to_read as isize
}

// TODO - this is all just until we do more sophisticated, (more closely resembling native)
//        things with stdout (stderr, and stdin)
#[link(wasm_import_module = "env")]
extern "C" {
    /// Host function that receives a single line (including the trailing newline).
    /// The host environment can print it in xterm or a console, line by line.
    fn box_host_write_stdout_line(ptr: *const u8, len: usize);
}

static mut STDOUT_LINE_ACCUM: Vec<u8> = Vec::new();

#[no_mangle]
pub extern "C" fn wasm_vfs_write(fd: i32, buf: *const u8, count: usize) -> isize {
    // Special case: FD == 1 => "stdout"
    if fd == 1 {
        unsafe {
            // Convert the incoming pointer+length into a slice
            let incoming = core::slice::from_raw_parts(buf, count);

            // Go through each byte, appending to STDOUT_LINE_ACCUM.
            // Whenever we see '\n', we flush that line to the host function.
            for &byte in incoming.iter() {
                STDOUT_LINE_ACCUM.push(byte);
                if byte == b'\n' {
                    // We have a full line in STDOUT_LINE_ACCUM
                    box_host_write_stdout_line(
                        STDOUT_LINE_ACCUM.as_ptr(),
                        STDOUT_LINE_ACCUM.len(),
                    );
                    // Clear the accumulator
                    STDOUT_LINE_ACCUM.clear();
                }
            }
        }
        return count as isize;
    }

    let mut proc = get_or_init_proc();

    let (inode_num, old_pos, append_mode) = {
        let h = match proc.open_files.get_mut(&fd) {
            Some(x) => x,
            None => return -1,
        };
        (h.inode_number, h.position, h.append_mode)
    };


    let new_position = {
        let data = match proc.fs.files.get_mut(&inode_num) {
            Some(d) => d,
            None => return -1,
        };
        let mut actual_pos = if append_mode { data.len() as u64 } else { old_pos };
        if actual_pos as usize + count > data.len() {
            data.resize(actual_pos as usize + count, 0);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(buf, data.as_mut_ptr().add(actual_pos as usize), count);
        }
        actual_pos += count as u64;
        actual_pos
    };

    if let Some(handle2) = proc.open_files.get_mut(&fd) {
        handle2.position = new_position;
    }
    let final_size = proc.fs.files.get(&inode_num).map(|v| v.len()).unwrap_or(0);
    if let Some(inode) = proc.fs.inodes.get_mut(inode_num as usize) {
        inode.size = final_size as u64;
    }
    count as isize
}



#[no_mangle]
pub extern "C" fn wasm_vfs_openat(_dirfd: i32, pathname: *const i8, flags: i32, mode: u32) -> i32 {
    wasm_vfs_open(pathname, flags, mode)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_dup(oldfd: i32) -> i32 {
    let mut proc = get_or_init_proc();

    if (oldfd as usize) < proc.fd_table.len() {
        if let Some(inode_number) = proc.fd_table[oldfd as usize] {
            let (inode_number, position, append_mode) = {
                let old_handle = match proc.open_files.get(&oldfd) {
                    Some(h) => h,
                    None => return -1,
                };
                (old_handle.inode_number, old_handle.position, old_handle.append_mode)
            };

            if let Some(new_fd) = proc.allocate_fd() {
                proc.fd_table[new_fd as usize] = Some(inode_number);
                let new_handle = OpenFileHandle {
                    inode_number,
                    position,
                    append_mode,
                };
                proc.open_files.insert(new_fd, new_handle);
                return new_fd;
            }
        }
    }
    -1
}

#[no_mangle]
pub extern "C" fn wasm_vfs_dup2(oldfd: i32, newfd: i32) -> i32 {
    let mut proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_pread64(fd: i32, buf: *mut u8, count: usize, offset: i64) -> isize {
    let mut proc = get_or_init_proc();

    let inode_num = {
        let h = match proc.open_files.get(&fd) {
            Some(x) => x,
            None => return -1,
        };
        h.inode_number
    };

    let data = match proc.fs.files.get(&inode_num) {
        Some(d) => d,
        None => return -1,
    };

    let position = offset as usize;
    if position >= data.len() {
        return 0;
    }
    let to_read = core::cmp::min(count, data.len() - position);
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr().add(position), buf, to_read);
    }
    to_read as isize
}

#[no_mangle]
pub extern "C" fn wasm_vfs_pwrite64(fd: i32, buf: *const u8, count: usize, offset: i64) -> isize {
    let mut proc = get_or_init_proc();


    let inode_num = {
        let handle = match proc.open_files.get(&fd) {
            Some(h) => h,
            None => return -1,
        };
        handle.inode_number
    };

    {
        let data = match proc.fs.files.get_mut(&inode_num) {
            Some(d) => d,
            None => return -1,
        };
        let position = offset as usize;
        if position + count > data.len() {
            data.resize(position + count, 0);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buf, data.as_mut_ptr().add(position), count);
        }
    }


    {
        let final_len = proc.fs.files.get(&inode_num).map(|v| v.len()).unwrap_or(0);
        if let Some(inode) = proc.fs.inodes.get_mut(inode_num as usize) {
            inode.size = final_len as u64;
        }
    }

    count as isize
}


#[no_mangle]
pub extern "C" fn wasm_vfs_sendfile(
    out_fd: i32,
    in_fd: i32,
    offset: *mut i64,
    count: usize
) -> isize {
    let mut proc = get_or_init_proc();

    let (in_inode_number, in_pos, in_app) = {
        let h_in = match proc.open_files.get(&in_fd) {
            Some(h) => h,
            None => return -1,
        };
        (h_in.inode_number, h_in.position, h_in.append_mode)
    };

    let (out_inode_number, out_pos, out_app) = {
        let h_out = match proc.open_files.get(&out_fd) {
            Some(h) => h,
            None => return -1,
        };
        (h_out.inode_number, h_out.position, h_out.append_mode)
    };

    let in_data = match proc.fs.files.get(&in_inode_number) {
        Some(d) => d.clone(),
        None => return -1,
    };

    let mut read_pos = if !offset.is_null() {
        unsafe { *offset as usize }
    } else {
        in_pos as usize
    };
    if read_pos >= in_data.len() {
        return 0;
    }

    let to_copy = core::cmp::min(count, in_data.len() - read_pos);

    {
        let out_data = match proc.fs.files.get_mut(&out_inode_number) {
            Some(d) => d,
            None => return -1,
        };

        let real_out_pos = if out_app {
            out_data.len()
        } else {
            out_pos as usize
        };

        if real_out_pos + to_copy > out_data.len() {
            out_data.resize(real_out_pos + to_copy, 0);
        }
        out_data[real_out_pos..real_out_pos + to_copy]
            .copy_from_slice(&in_data[read_pos..read_pos + to_copy]);

        if !offset.is_null() {
            unsafe {
                *offset += to_copy as i64;
            }
        } else {
        }
    }


    if offset.is_null() {
        if let Some(h_in_mut) = proc.open_files.get_mut(&in_fd) {
            h_in_mut.position = in_pos + to_copy as u64;
        }
    }

    if let Some(h_out_mut) = proc.open_files.get_mut(&out_fd) {
        if out_app {
            let new_len = {
            };
        } else {
            h_out_mut.position = out_pos + to_copy as u64;
        }
    }

    let new_len = {
        if let Some(d) = proc.fs.files.get(&out_inode_number) {
            d.len()
        } else {
            0
        }
    };

    if let Some(h_out_mut) = proc.open_files.get_mut(&out_fd) {
        if out_app {
            h_out_mut.position = new_len as u64;
        } else {
            h_out_mut.position = out_pos + to_copy as u64;
        }
    }

    to_copy as isize
}


#[no_mangle]
pub extern "C" fn wasm_vfs_sendfile64(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> isize {
    wasm_vfs_sendfile(out_fd, in_fd, offset, count)
}

//#[no_mangle]
//pub extern "C" fn wasm_vfs_splice(fd_in: i32, off_in: *mut i64, fd_out: i32, off_out: *mut i64, len: usize, _flags: u32) -> isize {
    //let mut proc = get_or_init_proc();

    //let (in_inode_number, in_position, in_append_mode) = {
        //let in_handle = proc.open_files.get(&fd_in).ok_or(-1).unwrap();
        //(in_handle.inode_number, in_handle.position, in_handle.append_mode)
    //};

    //let (out_inode_number, out_position, out_append_mode) = {
        //let out_handle = proc.open_files.get(&fd_out).ok_or(-1).unwrap();
        //(out_handle.inode_number, out_handle.position, out_handle.append_mode)
    //};

    //let in_data = proc.fs.files.get(&in_inode_number).unwrap().clone();

    //let mut in_pos = if !off_in.is_null() {
        //unsafe { *off_in as usize }
    //} else {
        //in_position as usize
    //};

    //if in_pos >= in_data.len() {
        //return 0;
    //}

    //let to_copy = min(len, in_data.len() - in_pos);

    //{
        //let out_data = proc.fs.files.get_mut(&out_inode_number).unwrap();
        //let mut out_pos = if !off_out.is_null() {
            //unsafe { *off_out as usize }
        //} else if out_append_mode {
            //out_data.len()
        //} else {
            //out_position as usize
        //};

        //if out_pos + to_copy > out_data.len() {
            //out_data.resize(out_pos + to_copy, 0);
        //}

        //out_data[out_pos..out_pos+to_copy].copy_from_slice(&in_data[in_pos..in_pos+to_copy]);

        //if !off_in.is_null() {
            //unsafe {
                //*off_in += to_copy as i64;
            //}
        //} else {
            //if let Some(in_handle_mut) = proc.open_files.get_mut(&fd_in) {
                //in_handle_mut.position = in_position + to_copy as u64;
            //}
        //}

        //if !off_out.is_null() {
            //unsafe {
                //*off_out += to_copy as i64;
            //}
        //} else {
            //if let Some(out_handle_mut) = proc.open_files.get_mut(&fd_out) {
                //if out_append_mode {
                    //out_handle_mut.position = out_data.len() as u64;
                //} else {
                    //out_handle_mut.position = out_position + to_copy as u64;
                //}
            //}
        //}

        //let new_size = out_data.len();
        //drop(out_data);

        //if let Some(out_inode) = proc.get_inode_mut(out_inode_number) {
            //out_inode.size = new_size as u64;
        //}
    //}

    //to_copy as isize
//}
//

#[no_mangle]
pub extern "C" fn wasm_vfs_splice(
    fd_in: i32,
    off_in: *mut i64,
    fd_out: i32,
    off_out: *mut i64,
    len: usize,
    _flags: u32
) -> isize {
    let mut proc = get_or_init_proc();

    // 1) Input FD info
    let (in_ino, in_pos, in_app) = {
        let h = match proc.open_files.get(&fd_in) {
            Some(x) => x,
            None => return -1,
        };
        (h.inode_number, h.position, h.append_mode)
    };

    // 2) Output FD info
    let (out_ino, out_pos, out_app) = {
        let h = match proc.open_files.get(&fd_out) {
            Some(x) => x,
            None => return -1,
        };
        (h.inode_number, h.position, h.append_mode)
    };

    // 3) Clone input
    let in_data = match proc.fs.files.get(&in_ino) {
        Some(d) => d.clone(),
        None => return -1,
    };

    let mut read_pos = if !off_in.is_null() {
        unsafe { *off_in as usize }
    } else {
        in_pos as usize
    };
    if read_pos >= in_data.len() {
        return 0;
    }

    let to_copy = core::cmp::min(len, in_data.len() - read_pos);

    {
        // 4) Mutably borrow out_data in a short block
        let out_data = match proc.fs.files.get_mut(&out_ino) {
            Some(d) => d,
            None => return -1,
        };

        let mut write_pos = if !off_out.is_null() {
            unsafe { *off_out as usize }
        } else if out_app {
            out_data.len()
        } else {
            out_pos as usize
        };

        if write_pos + to_copy > out_data.len() {
            out_data.resize(write_pos + to_copy, 0);
        }
        out_data[write_pos..write_pos + to_copy]
            .copy_from_slice(&in_data[read_pos..read_pos + to_copy]);

        if !off_in.is_null() {
            unsafe { *off_in += to_copy as i64; }
        }
        if !off_out.is_null() {
            unsafe { *off_out += to_copy as i64; }
        }
    }

    if off_in.is_null() {
        if let Some(in_handle) = proc.open_files.get_mut(&fd_in) {
            in_handle.position = in_pos + to_copy as u64;
        }
    }

    let (out_app_local, old_out_pos) = {
        if let Some(out_handle) = proc.open_files.get_mut(&fd_out) {
            (out_handle.append_mode, out_handle.position)
        } else {
            // If no handle, bail out
            return -1;
        }
    };

    let length_copy = if out_app_local && off_out.is_null() {
        if let Some(od) = proc.fs.files.get(&out_ino) {
            od.len()
        } else {
            0
        }
    } else {
        old_out_pos as usize
    };

    if let Some(out_handle) = proc.open_files.get_mut(&fd_out) {
        if out_app_local && off_out.is_null() {
            out_handle.position = length_copy as u64;
        } else if off_out.is_null() {
            out_handle.position = old_out_pos + to_copy as u64;
        }
    }

    to_copy as isize
}

#[no_mangle]
pub extern "C" fn wasm_vfs_getdents(fd: i32, dirp: *mut Dirent, count: usize) -> isize {
    let mut proc = get_or_init_proc();


    let inode_number = {
        let handle = match proc.open_files.get_mut(&fd) {
            Some(h) => h,
            None => return -1,
        };
        let ino = handle.inode_number;
        ino
    };

    let inode = match proc.get_inode(inode_number) {
        Some(i) => i,
        None => return -1,
    };

    if let InodeKind::Directory = inode.kind {

    } else {
        return -1;
    }

    let dir_prefix = if inode.number == 0 {
        PathBuf::from("/")
    } else {
        proc.fs.path_map.iter()
            .find_map(|(pp, ino)| if *ino == inode.number { Some(pp.clone()) } else { None })
            .unwrap_or(PathBuf::from("/"))
    };

    let dir_path = proc.fs.path_map.iter().filter(|(p, _)| {
        if p.as_path() == dir_prefix.as_path() {
            false
        } else {
            p.parent().map(|pp| pp == dir_prefix).unwrap_or(false)
        }
    });

    let entries: Vec<(PathBuf, u64)> = dir_path.map(|(p,i)| (p.clone(), *i)).collect();


    let position = {
        let handle = proc.open_files.get_mut(&fd).unwrap();
        handle.position
    };

    let start = position as usize;
    if start >= entries.len() {
        return 0;
    }

    let mut written = 0;
    let mut dirp_ptr = dirp as *mut u8;

    for (path, ino) in entries.iter().skip(start) {
        let name = path.file_name().unwrap_or_default().to_string();
        let d_type = inode_kind_to_dtype(&proc.get_inode(*ino).unwrap().kind);

        let mut d: Dirent = Dirent {
            d_ino: *ino,
            d_off: (position + 1) as i64,
            d_reclen: 0,
            d_type,
            d_name: [0;256],
        };

        let bytes = name.as_bytes();
        if bytes.len() >= 256 {
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


        {
            let handle = proc.open_files.get_mut(&fd).unwrap();
            handle.position += 1;
        }
    }

    written as isize
}

#[no_mangle]
pub extern "C" fn wasm_vfs_getdents64(fd: i32, dirp: *mut Dirent64, count: usize) -> isize {

    let mut buf = vec![0u8; count];
    let ret = wasm_vfs_getdents(fd, buf.as_mut_ptr() as *mut Dirent, count);
    if ret <= 0 {
        return ret;
    }


    let used = ret as usize;
    unsafe {
        std::ptr::copy_nonoverlapping(buf.as_ptr(), dirp as *mut u8, used);
    }
    ret
}

#[no_mangle]
pub extern "C" fn wasm_vfs_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    let mut proc = get_or_init_proc();

    let (inode_num, old_pos) = {
        let h = match proc.open_files.get_mut(&fd) {
            Some(x) => x,
            None => return -1,
        };
        (h.inode_number, h.position)
    };


    let size = match proc.fs.files.get(&inode_num) {
        Some(d) => d.len() as i64,
        None => return -1,
    };

    let new_pos = match whence {
        0 => offset,              // SEEK_SET
        1 => old_pos as i64 + offset, // SEEK_CUR
        2 => size + offset,       // SEEK_END
        _ => return -1,
    };
    if new_pos < 0 {
        return -1;
    }


    if let Some(h2) = proc.open_files.get_mut(&fd) {
        h2.position = new_pos as u64;
    }

    new_pos
}

#[no_mangle]
pub extern "C" fn wasm_vfs_stat(path: *const i8, statbuf: *mut Stat) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };

    let proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_fstat(fd: i32, statbuf: *mut Stat) -> i32 {
    let proc = get_or_init_proc();

    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };

    let inode = proc.get_inode(inode_num).unwrap();
    fill_stat_from_inode(inode, statbuf);
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_lstat(path: *const i8, statbuf: *mut Stat) -> i32 {
    wasm_vfs_stat(path, statbuf)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_fstatat(dirfd: i32, pathname: *const i8, statbuf: *mut Stat, _flags: i32) -> i32 {
    wasm_vfs_stat(pathname, statbuf)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_getcwd(buf: *mut i8, size: usize) -> *mut i8 {
    let proc = get_or_init_proc();
    let cwd_str = proc.fs.current_directory.to_string_lossy();

    let bytes = cwd_str.as_bytes();
    if bytes.len() + 1 > size {
        return core::ptr::null_mut();
    }

    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
        *buf.add(bytes.len()) = 0;
    }
    buf
}

#[no_mangle]
pub extern "C" fn wasm_vfs_chdir(path: *const i8) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };

    let mut proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_fchdir(fd: i32) -> i32 {
    let mut proc = get_or_init_proc();

    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };
    let inode = proc.get_inode(inode_num).unwrap();
    if let InodeKind::Directory = inode.kind {
        let path = proc.fs.path_map.iter().find_map(|(p, i)| if *i == inode_num { Some(p.clone()) } else { None }).unwrap();
        proc.fs.current_directory = path;
        0
    } else {
        -1
    }
}

#[no_mangle]
pub extern "C" fn wasm_vfs_chmod(path: *const i8, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };

    let mut proc = get_or_init_proc();
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
pub extern "C" fn wasm_vfs_fchmod(fd: i32, mode: u32) -> i32 {
    let mut proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_fchmodat(_dirfd: i32, pathname: *const i8, mode: u32, _flags: i32) -> i32 {
    wasm_vfs_chmod(pathname, mode)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_chown(path: *const i8, owner: u32, group: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };

    let mut proc = get_or_init_proc();
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
pub extern "C" fn wasm_vfs_lchown(path: *const i8, owner: u32, group: u32) -> i32 {
    wasm_vfs_chown(path, owner, group)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_fchown(fd: i32, owner: u32, group: u32) -> i32 {
    let mut proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_fchownat(_dirfd: i32, pathname: *const i8, owner: u32, group: u32, _flags: i32) -> i32 {
    wasm_vfs_chown(pathname, owner, group)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_access(path: *const i8, mode: i32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };
    let proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_faccessat(_dirfd: i32, pathname: *const i8, mode: i32, _flags: i32) -> i32 {
    wasm_vfs_access(pathname, mode)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_umask(mask: u32) -> u32 {
    let mut proc = get_or_init_proc();
    let old = proc.umask_value;
    proc.umask_value = mask & 0o777;
    old
}

#[no_mangle]
pub extern "C" fn wasm_vfs_rename(oldpath: *const i8, newpath: *const i8) -> i32 {
    let old_str = unsafe { CStr::from_ptr(oldpath).to_string_lossy() };
    let new_str = unsafe { CStr::from_ptr(newpath).to_string_lossy() };

    let mut proc = get_or_init_proc();

    let old_abs = proc.get_absolute_path(&PathBuf::from(old_str));
    let new_abs = proc.get_absolute_path(&PathBuf::from(new_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&old_abs) {
        Some(i) => i,
        None => return -1,
    };


    proc.fs.path_map.remove(&old_abs);

    proc.fs.path_map.insert(new_abs, inode_num);
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_renameat(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8) -> i32 {
    wasm_vfs_rename(oldpath, newpath)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_renameat2(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8, _flags: u32) -> i32 {
    wasm_vfs_renameat(olddirfd, oldpath, newdirfd, newpath)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_link(oldpath: *const i8, newpath: *const i8) -> i32 {
    let old_str = unsafe { CStr::from_ptr(oldpath).to_string_lossy() };
    let new_str = unsafe { CStr::from_ptr(newpath).to_string_lossy() };

    let mut proc = get_or_init_proc();

    let old_abs = proc.get_absolute_path(&PathBuf::from(old_str));
    let new_abs = proc.get_absolute_path(&PathBuf::from(new_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&old_abs) {
        Some(i) => i,
        None => return -1,
    };
    proc.fs.path_map.insert(new_abs, inode_num);
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_linkat(olddirfd: i32, oldpath: *const i8, newdirfd: i32, newpath: *const i8, _flags: i32) -> i32 {
    wasm_vfs_link(oldpath, newpath)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_unlink(pathname: *const i8) -> i32 {
    let path_str = unsafe { CStr::from_ptr(pathname).to_string_lossy() };
    let mut proc = get_or_init_proc();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };

    let inode = proc.get_inode(inode_num).unwrap();
    if let InodeKind::Directory = inode.kind {
        return -1;
    }

    proc.fs.path_map.remove(&abs_path);
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_unlinkat(dirfd: i32, pathname: *const i8, flags: i32) -> i32 {
    // ignoring flags and dirfd
    wasm_vfs_unlink(pathname)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_symlink(target: *const i8, linkpath: *const i8) -> i32 {
    let target_str = unsafe { CStr::from_ptr(target).to_string_lossy() };
    let link_str = unsafe { CStr::from_ptr(linkpath).to_string_lossy() };

    let mut proc = get_or_init_proc();

    let link_abs = proc.get_absolute_path(&PathBuf::from(link_str));
    let inode_number = proc.fs.next_inode_number;
    proc.fs.next_inode_number += 1;

    let kind = InodeKind::SymbolicLink(PathBuf::from(target_str));
    proc.insert_directory_entry(&link_abs, inode_number, kind);
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_symlinkat(target: *const i8, newdirfd: i32, linkpath: *const i8) -> i32 {
    wasm_vfs_symlink(target, linkpath)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_readlink(path: *const i8, buf: *mut i8, bufsize: usize) -> isize {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };
    let proc = get_or_init_proc();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));
    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };
    let inode = proc.get_inode(inode_num).unwrap();
    match &inode.kind {
        InodeKind::SymbolicLink(t) => {
            let loss = t.to_string_lossy();
            let bytes = loss.as_bytes();
            let to_copy = min(bytes.len(), bufsize);
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, to_copy);
            }
            to_copy as isize
        }
        _ => -1,
    }
}

#[no_mangle]
pub extern "C" fn wasm_vfs_readlinkat(dirfd: i32, pathname: *const i8, buf: *mut i8, bufsize: usize) -> isize {
    wasm_vfs_readlink(pathname, buf, bufsize)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_mkdir(path: *const i8, mode: u32) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };

    let mut proc = get_or_init_proc();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));

    if proc.fs.lookup_inode_by_path(&abs_path).is_some() {
        return -1;
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
pub extern "C" fn wasm_vfs_mkdirat(dirfd: i32, pathname: *const i8, mode: u32) -> i32 {
    wasm_vfs_mkdir(pathname, mode)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_rmdir(path: *const i8) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };

    let mut proc = get_or_init_proc();

    let abs_path = proc.get_absolute_path(&PathBuf::from(path_str));

    let inode_num = match proc.fs.lookup_inode_by_path(&abs_path) {
        Some(i) => i,
        None => return -1,
    };
    let inode = proc.get_inode(inode_num).unwrap();
    match inode.kind {
        InodeKind::Directory => {

            let entries = proc.fs.path_map.iter().filter(|(p,_)| {
                p.parent().map(|pp| pp == abs_path).unwrap_or(false)
            }).count();
            if entries > 0 {
                return -1;
            }
            proc.fs.path_map.remove(&abs_path);
            0
        },
        _ => -1,
    }
}

#[no_mangle]
pub extern "C" fn wasm_vfs_truncate(path: *const i8, length: i64) -> i32 {
    let path_str = unsafe { CStr::from_ptr(path).to_string_lossy() };
    let mut proc = get_or_init_proc();

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
pub extern "C" fn wasm_vfs_ftruncate(fd: i32, length: i64) -> i32 {
    if length < 0 {
        return -1;
    }

    let mut proc = get_or_init_proc();
    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };
    proc.set_file_size(inode_num, length as usize);
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_fallocate(fd: i32, _mode: i32, offset: i64, len: i64) -> i32 {
    if offset < 0 || len < 0 {
        return -1;
    }

    let mut proc = get_or_init_proc();
    let inode_num = match proc.fd_table.get(fd as usize) {
        Some(Some(i)) => *i,
        _ => return -1,
    };

    let end = (offset + len) as usize;

    {
        let data = proc.fs.files.get_mut(&inode_num).unwrap();
        if data.len() < end {
            data.resize(end, 0);
        }
    }

    let new_size = proc.fs.files.get(&inode_num).unwrap().len();
    if let Some(inode) = proc.get_inode_mut(inode_num) {
        inode.size = new_size as u64;
    }

    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_posix_fallocate(fd: i32, offset: i64, len: i64) -> i32 {
    wasm_vfs_fallocate(fd, 0, offset, len)
}

#[no_mangle]
pub extern "C" fn wasm_vfs_flock(fd: i32, _operation: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_mmap(_addr: *mut u8, _length: usize, _prot: i32, _flags: i32, _fd: i32, _offset: isize) -> *mut u8 {
    // not supported in this in-memory fs
    core::ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn wasm_vfs_munmap(_addr: *mut u8, _length: usize) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_mprotect(_addr: *mut u8, _length: usize, _prot: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_mlock(_addr: *const u8, _len: usize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_munlock(_addr: *const u8, _len: usize) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_msync(_addr: *mut u8, _length: usize, _flags: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_sync() {
    // no-op
}

#[no_mangle]
pub extern "C" fn wasm_vfs_fsync(_fd: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_fdatasync(_fd: i32) -> i32 {
    // no-op
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_syncfs(_fd: i32) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn wasm_vfs_inotify_init() -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn wasm_vfs_inotify_init1(_flags: i32) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn wasm_vfs_inotify_add_watch(_fd: i32, _pathname: *const i8, _mask: u32) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn wasm_vfs_inotify_rm_watch(_fd: i32, _wd: i32) -> i32 {
    -1
}

// -----------------------------------------------------------
// helper function to populate/mount a host directory into wasm-vfs

//#[cfg(feature = "std")]
//pub fn mount_host_directory(host_path: &path, mount_path: &path) -> result<(), std::io::error> {
    //use std::fs;
    //let mut proc = global_proc.lock().unwrap();
    //if proc.is_none() {
        //*proc = Some(proc::new());
    //}


    //fn recurse(proc: &mut proc, host: &path, vfs_path: &path) -> std::io::result<()> {
        //let meta = host.wasm_vfs_symlink_metadata()?;
        //if meta.is_dir() {
            //if vfs_path != path::new("/") {
                //let inode_number = proc.fs.next_inode_number;
                //proc.fs.next_inode_number += 1;
                //proc.fs.path_map.insert(vfs_path.to_path_buf(), inode_number);
                //let inode = Inode::new(
                    //inode_number,
                    //0,
                    //Permissions::from((0o755) as u16),
                    //0,
                    //0,
                    //0,
                    //0,
                    //0,
                    //InodeKind::Directory
                //);
                //while proc.fs.inodes.len() <= inode_number as usize {
                    //proc.fs.inodes.push(Inode::default());
                //}
                //proc.fs.inodes[inode_number as usize] = inode;
                //proc.fs.files.insert(inode_number, vec![]);
            //}
            //for entry in fs::read_dir(host)? {
                //let entry = entry?;
                //let name = entry.file_name();
                //let new_vfs_path = vfs_path.join(&name);
                //recurse(proc, &entry.path(), &new_vfs_path)?;
            //}
        //} else if meta.is_file() {
            //let inode_number = proc.fs.next_inode_number;
            //proc.fs.next_inode_number += 1;
            //proc.fs.path_map.insert(vfs_path.to_path_buf(), inode_number);
            //let data = fs::read(host)?;
            //let inode = Inode::new(
                //inode_number,
                //data.len() as u64,
                //Permissions::from((0o644) as u16),
                //0,
                //0,
                //0,
                //0,
                //0,
                //InodeKind::file
            //);
            //while proc.fs.inodes.len() <= inode_number as usize {
                //proc.fs.inodes.push(Inode::default());
            //}
            //proc.fs.inodes[inode_number as usize] = inode;
            //proc.fs.files.insert(inode_number, data);
        //} else if meta.file_type().is_symlink() {
            //let target = fs::read_link(host)?;
            //let inode_number = proc.fs.next_inode_number;
            //proc.fs.next_inode_number += 1;
            //proc.fs.path_map.insert(vfs_path.to_path_buf(), inode_number);
            //let inode = Inode::new(
                //inode_number,
                //0,
                //Permissions::from((0o777) as u16),
                //0,
                //0,
                //0,
                //0,
                //0,
                //InodeKind::SymbolicLink(target),
            //);
            //while proc.fs.inodes.len() <= inode_number as usize {
                //proc.fs.inodes.push(Inode::default());
            //}
            //proc.fs.inodes[inode_number as usize] = inode;
        //}

        //Ok(())
    //}

    //recurse(proc, host_path, mount_path)?;

    //Ok(())
//}
//

/// A descriptor for a single file to copy into the Wasm VFS.
///
/// - `dest_path`: a C-style null-terminated string (pointer to i8),
/// - `data_ptr`: raw pointer to file content in guest memory,
/// - `data_len`: number of bytes at `data_ptr`.
#[repr(C)]
pub struct FileDef {
    pub dest_path: *const i8,
    pub data_ptr: *const u8,
    pub data_len: u32,
}


#[no_mangle]
pub extern "C" fn wasm_vfs_mount_in_memory(count: u32, files: *const FileDef) -> i32 {
    let _proc_guard = get_or_init_proc();
    for i in 0..count {
        let filedef_ptr = unsafe { files.add(i as usize) };
        if filedef_ptr.is_null() {
            // If any pointer is null, bail
            return -1;
        }

        let filedef = unsafe { &*filedef_ptr };

        // (b) Convert the destination path from CStr to Rust string
        let path_str = unsafe {
            crate::ffi::CStr::from_ptr(filedef.dest_path).to_string_lossy();
        };

        let fd = wasm_vfs_open(
            filedef.dest_path,                  // the same pointer
            0x001 /*O_WRONLY*/ | 0x040 /*O_CREAT*/ | 0x200 /*O_TRUNC*/,
            0o644
        );
        if fd < 0 {
            continue;
        }

        let data_len = filedef.data_len as usize;
        let data_slice = unsafe {
            core::slice::from_raw_parts(filedef.data_ptr, data_len)
        };

        let written = wasm_vfs_write(fd, data_slice.as_ptr(), data_slice.len());
        if written < 0 {
            // optional: handle error
        }

        let rc = wasm_vfs_close(fd);
        if rc < 0 {
            // optional: handle error
        }
    }

    0
}

