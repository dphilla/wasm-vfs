use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::PathBuf;

#[derive(Debug)]
pub enum InodeKind {
    File,
    Directory,
    SymbolicLink(PathBuf),
}

#[derive(Debug)]
pub struct DirectoryEntry {
    name: String,
    inode: Inode,
}

#[derive(Eq, Hash, PartialEq, Debug, Clone, Default)]
pub struct Inode {
    pub number: u64,
    pub size: u64,
    pub permissions: Permissions,
    pub user_id: u32,
    pub group_id: u32,
    pub ctime: u64,
    pub mtime: u64,
    pub atime: u64,
    pub kind: InodeKind,
    pub children: Vec<DirectoryEntry>,
}

impl Inode {
    fn new(number: u64, size: u64, permissions: Permissions, user_id: u32, group_id: u32, ctime: u64, mtime: u64, atime: u64, kind: InodeKind) -> Self {
        let children = match &kind {
            InodeKind::Directory => vec![DirectoryEntry { name: ".".to_string(), inode: Inode { number, size, permissions, user_id, group_id, ctime, mtime, atime, kind: InodeKind::Directory(vec![]) } }],
            _ => vec![],
        };
        Inode {
            number,
            size,
            permissions,
            user_id,
            group_id,
            ctime,
            mtime,
            atime,
            kind,
            children,
        }
    }
}

#[derive(Debug, Default)]
pub struct Permissions {
    owner: Permission,
    group: Permission,
    other: Permission,
}

#[derive(Debug, Default)]
pub struct Permission {
    read: bool,
    write: bool,
    execute: bool,
}

impl From<u16> for Permissions {
    fn from(mode: u16) -> Self {
        Self {
            owner: Permission {
                read: (mode & 0o400) != 0,
                write: (mode & 0o200) != 0,
                execute: (mode & 0o100) != 0,
            },
            group: Permission {
                read: (mode & 0o40) != 0,
                write: (mode & 0o20) != 0,
                execute: (mode & 0o10) != 0,
            },
            other: Permission {
                read: (mode & 0o4) != 0,
                write: (mode & 0o2) != 0,
                execute: (mode & 0o1) != 0,
            },
        }
    }
}

pub struct Stat {
    pub st_dev: u64,        // ID of device containing file
    pub st_ino: u64,        // inode number
    pub st_mode: u16,       // protection
    pub st_nlink: u16,      // number of hard links
    pub st_uid: u32,        // user ID of owner
    pub st_gid: u32,        // group ID of owner
    pub st_rdev: u64,       // device ID (if special file)
    pub st_size: i64,       // total size, in bytes
    pub st_blksize: i32,    // blocksize for file system I/O
    pub st_blocks: i64,     // number of 512B blocks allocated
    pub st_atime: i64,      // time of last access
    pub st_mtime: i64,      // time of last modification
    pub st_ctime: i64,      // time of last status change
}

pub type FileDescriptor = i32;
pub type DirectoryDescriptor = i32;

#[derive(Debug)]
pub struct File {
    pub inode: Inode,
    pub data: Mutex<Vec<u8>>,
    pub position: u64,
    pub path: PathBuf
}

#[derive(Clone)]
pub struct OpenFile {
    pub file: Arc<File>,
    pub position: u64,
}

impl Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let file_data = &self.file.data.lock().unwrap();
        let bytes_to_read = std::cmp::min(buf.len(), (file_data.len() - self.position as usize));
        buf[..bytes_to_read].copy_from_slice(&file_data[self.position as usize..(self.position + bytes_to_read as u64) as usize]);
        self.position += bytes_to_read as u64;
        Ok(bytes_to_read)
    }
}

impl Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file_data = self.file.data.lock().unwrap();
        let new_size = std::cmp::max(file_data.len(), self.position as usize + buf.len());
        file_data.resize(new_size, 0);
        file_data[self.position as usize..self.position as usize + buf.len()]
            .copy_from_slice(buf);
        self.position += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl OpenFile {

    pub fn lseek(&mut self, pos: i64, whence: i32) -> Result<u64, &'static str> {
        let mut file_data = self.file.data.lock().unwrap();
        let file_len = file_data.len() as i64;

        //SEEK_SET (0) seeks from the beginning of the file.
        //SEEK_CUR (1) seeks from the current file position.
        //SEEK_END (2) seeks from the end of the file.
        let new_pos = match whence {
            0 => pos,
            1 => self.position as i64 + pos,
            2 => file_len + pos,
            _ => return Err("invalid whence"),
        };

        if new_pos < 0 || new_pos > file_len {
            return Err("invalid position");
        }

        self.position = new_pos as u64;

        Ok(self.position)
    }

}

// for now, there is a 1:1 with files:inodes, bc its a VFS
// in the future, a separation of files via OpenFile and File (possibly in a layer
// representing more persistent storage, could be possible)
pub struct FileSystem {
    pub files: HashMap<Inode, Arc<File>>,
    pub next_inode_number: u32
    pub open_files: HashMap<FileDescriptor, OpenFile>,
    pub next_fd: FileDescriptor,
    pub next_directory_descriptor: DirectoryDescriptor,
    pub open_directories: HashMap<DirectoryDescriptor, OpenDirectory>,
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            next_inode_number: 1,
            open_files:  HashMap::new(),
            next_fd: 1,
            next_directory_descriptor: 1,
            open_directories:   HashMap::new(),
        }
    }

    // --------------------
    // File Operations
    // --------------------

    pub fn lookup_inode(&mut self, path: &PathBuf) -> Option<Inode> {
        let inode = self.files
            .iter()
            .find_map(|(&inode, file)| if file.path == *path { Some(inode) } else { None });
        if inode == None {
            return Some(self.create_file(Vec::new(), path))
        } else {
            None
        }
    }

    fn get_inode(&self, fd: FileDescriptor) -> Result<&Inode, &'static str> {
        let open_file = self.open_files.get(&fd).ok_or("invalid file descriptor")?;
        Ok(&open_file.file.inode)
    }

    // change to look up inode from fd
    fn get_file(&self, fd: FileDescriptor) -> Result<Arc<File>, &'static str> {
        let inode = Inode { number: fd as u64, ..Default::default() };
        self.files.get(&inode).ok_or("invalid file descriptor")
            .map(|file| file.clone()) // need to clone?
    }

    pub fn create_file(&mut self, raw_data: Vec<u8>, path: &PathBuf) -> Inode {
        let inode = Inode::new(1, 1024, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::File);
        let data = Mutex::new(raw_data);
        let file = File { inode: inode.clone(), data, path: path.clone(), position: 0 };
        self.files.insert(inode.clone(), Arc::new(file));
        self.next_inode_number += 1;

        // Add new file to its parent's children
        if let Some(parent_path) = path.parent() {
            if let Some(parent_inode) = self.lookup_inode(&parent_path.into()) {
                if let Some(parent_file) = self.files.get_mut(&parent_inode) {
                    if let InodeKind::Directory(children) = &mut parent_file.inode.kind {
                        let dir_entry = DirectoryEntry { name: path.file_name().unwrap().to_str().unwrap().to_string(), inode: inode.clone() };
                        children.push(dir_entry);
                    }
                }
            }
        }

        inode
    }

    // *** "Syscalls" ***

    // --------------------
    // File Descriptor Management
    // --------------------

    pub fn open(&mut self, path: &PathBuf) -> Result<FileDescriptor, &'static str> {
        let inode = self.lookup_inode(path).ok_or("file not found")?;
        let file = self.files.get(&inode).ok_or("file not found")?;
        let open_file = OpenFile {
            file: file.clone(),
            position: 0,
        };
        let fd = self.next_fd;
        self.next_fd += 1;
        self.open_files.insert(fd, open_file);
        Ok(fd)
    }

    pub fn close(&mut self, fd: FileDescriptor) -> Result<(), &'static str> {
        self.open_files.remove(&fd).ok_or("invalid file descriptor")?;
        Ok(())
    }

    pub fn creat(&mut self, path: &PathBuf) -> Result<FileDescriptor, &'static str> {
        let inode = self.create_file(Vec::new(), path);
        let file = self.files.get(&inode).ok_or("file not found")?;
        let open_file = OpenFile {
            file: file.clone(),
            position: 0,
        };
        let fd = self.next_fd;
        self.next_fd += 1;
        self.open_files.insert(fd, open_file);
        Ok(fd)
    }

    pub fn openat(&mut self, dir_fd: FileDescriptor, path: &PathBuf, create: bool) -> Result<FileDescriptor, &'static str> {
        let dir_file = self.get_file(dir_fd)?;
        let full_path = dir_file.path.join(path);
        if create {
            self.creat(&full_path)
        } else {
            self.open(&full_path)
        }
    }


    pub fn dup(&mut self, old_fd: FileDescriptor) -> Result<FileDescriptor, &'static str> {
        let open_file = self.open_files.get(&old_fd).ok_or("invalid file descriptor")?;
        let new_fd = self.next_fd;
        self.next_fd += 1;
        self.open_files.insert(new_fd, open_file.clone());
        Ok(new_fd)
    }

    pub fn dup2(&mut self, old_fd: FileDescriptor, new_fd: FileDescriptor) -> Result<(), &'static str> {
        let open_file = self.open_files.get(&old_fd).ok_or("invalid file descriptor")?;
        self.open_files.insert(new_fd, open_file.clone());
        Ok(())
    }

    pub fn fstat(&self, fd: FileDescriptor) -> Result<Stat, &'static str> {
        let file = self.get_file(fd)?;
        let inode = &file.inode;
        Ok(Stat {
            st_dev: 0, // This is a virtual file system, so device ID doesn't really apply
            st_ino: inode.number,
            st_mode: 0, // You'll need to convert permissions to a mode
            st_nlink: 1, // Hard links aren't supported in this example
            st_uid: inode.user_id,
            st_gid: inode.group_id,
            st_rdev: 0, // Device ID doesn't apply for regular files
            st_size: inode.size as i64,
            st_blksize: 0, // This is a virtual file system, so block size doesn't really apply
            st_blocks: 0, // This is a virtual file system, so number of blocks doesn't really apply
            st_atime: inode.atime as i64,
            st_mtime: inode.mtime as i64,
            st_ctime: inode.ctime as i64,
        })
    }

    pub fn stat(&self, path: &PathBuf) -> Result<Stat, &'static str> {
        let inode = self.lookup_inode(path).ok_or("file not found")?;
        let file = self.files.get(&inode).ok_or("file not found")?;
        let inode = &file.inode;
        Ok(Stat {
            st_dev: 0, // This is a virtual file system, so device ID doesn't really apply
            st_ino: inode.number,
            st_mode: 0, // You'll need to convert permissions to a mode
            st_nlink: 1, // Hard links aren't supported in this example
            st_uid: inode.user_id,
            st_gid: inode.group_id,
            st_rdev: 0, // Device ID doesn't apply for regular files
            st_size: inode.size as i64,
            st_blksize: 0, // This is a virtual file system, so block size doesn't really apply
            st_blocks: 0, // This is a virtual file system, so number of blocks doesn't really apply
            st_atime: inode.atime as i64,
            st_mtime: inode.mtime as i64,
            st_ctime: inode.ctime as i64,
        })
    }


    pub fn unlink(&mut self, path: &PathBuf) -> Result<(), &'static str> {
        let inode = self.lookup_inode(path).ok_or("file not found")?;

        self.files.remove(&inode).ok_or("file not found")?;

        // Remove the file from its parent's children
        if let Some(parent_path) = path.parent() {
            if let Some(parent_inode) = self.lookup_inode(&parent_path.into()) {
                if let Some(parent_file) = self.files.get_mut(&parent_inode) {
                    if let InodeKind::Directory(children) = &mut parent_file.inode.kind {
                        children.retain(|dir_entry| dir_entry.inode != inode);
                    }
                }
            }
        }

        // sync?
        // same with close(), should write to somewhere? right? from the docs:

        //When a file is unlinked, the entry for the file is removed from the file system's directory.
        //The blocks or sectors on the disk that were used by the file's data are not immediately removed
        //or overwritten, but are marked as available for reuse. In other words, the data still exists
        //on the disk, but it is not accessible through the file system. It is possible for data recovery
        //software to recover deleted files from these blocks or sectors. However, eventually, when the
        //blocks or sectors are reused for other files, the old data will be overwritten and permanently lost.
        Ok(())
    }

    pub fn rename(&mut self, old_path: &PathBuf, new_path: &PathBuf) -> Result<(), &'static str> {
        let inode = self.lookup_inode(old_path).ok_or("file not found")?;

        let file = self.files.get(&inode).ok_or("file not found")?;
        file.path = new_path.clone();

        Ok(())
    }

    // --------------------
    // Directory Operations
    // --------------------


    pub struct OpenDirectory {
        pub directory: Arc<Inode>,
        pub position: usize,
    }

    pub fn mkdir(&mut self, path: &PathBuf) -> Result<(), &'static str> {
        if self.files.values().any(|file| file.path == *path) {
            return Err("File or directory already exists");
        }

        // Create new directory inode
        let inode = Inode::new(
            self.next_inode_number.into(),
            0,
            Permissions::from(0o755), // typical perms for a directory
            0,
            0,
            0,
            0,
            0,
            InodeKind::Directory
        );

        let file = File {
            inode: inode.clone(),
            data: Mutex::new(Vec::new()), // bc no actual data in a directory
            path: path.clone(),
            position: 0,
        };

        self.files.insert(inode.clone(), Arc::new(file));
        self.next_inode_number += 1;

        // Add new directory to its parent's children
        if let Some(parent_path) = path.parent() {
            if let Some(parent_inode) = self.lookup_inode(&parent_path.into()) {
                if let Some(parent_file) = self.files.get_mut(&parent_inode) {
                    if let InodeKind::Directory(children) = &mut parent_file.inode.kind {
                        let dir_entry = DirectoryEntry { name: path.file_name().unwrap().to_str().unwrap().to_string(), inode: inode.clone() };
                        children.push(dir_entry);
                    }
                }
            }
        }

        Ok(())
    }
    pub fn rmdir(&mut self, path: &PathBuf) -> Result<(), &'static str> {
        let inode = self.lookup_inode(path).ok_or("directory not found")?;

        let file = self.files.get(&inode).ok_or("directory not found")?;
        if let InodeKind::Directory(dir_entries) = &file.inode.kind {
            if !dir_entries.is_empty() {
                return Err("Directory is not empty");
            }
        } else {
            return Err("Not a directory");
        }

        self.files.remove(&inode);

        // Remove the directory from its parent's children
        if let Some(parent_path) = path.parent() {
            if let Some(parent_inode) = self.lookup_inode(&parent_path.into()) {
                if let Some(parent_file) = self.files.get_mut(&parent_inode) {
                    if let InodeKind::Directory(children) = &mut parent_file.inode.kind {
                        children.retain(|dir_entry| dir_entry.inode != inode);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn opendir(&mut self, path: &PathBuf) -> Result<DirectoryDescriptor, &'static str> {
        let inode = self.lookup_inode(path).ok_or("directory not found")?;

        // check if the inode is a directory
        match self.files.get(&inode).unwrap().inode.kind {
            InodeKind::Directory(_) => (),
            _ => return Err("Not a directory"),
        };

        let directory_descriptor = self.next_directory_descriptor;
        self.next_directory_descriptor += 1;

        let open_directory = OpenDirectory {
            directory: self.files.get(&inode).unwrap().clone(),
            position: 0,
        };

        self.open_directories.insert(directory_descriptor, open_directory);

        Ok(directory_descriptor)
    }

    // for future iterations, "disk" could by any other entity outside of module/component
    fn sync_to_disk(&self) -> std::io::Result<()> {
        //for file in self.files.values() {
            //let data = file.data.lock().unwrap();
            //let mut disk_file = std::fs::File::create(&file.path)?;
            //disk_file.write_all(&data)?;
        //}
        Ok(())
    }
}
