use std::collections::HashMap; use std::io::{Read, Write};
use std::path::PathBuf;


#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub enum InodeKind {
    #[default]
    File,
    Directory,
    SymbolicLink(PathBuf),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct DirectoryEntry {
    name: String,
    inode: Inode,

    // info below is more typical to flat-file
    // impl of dirents may be used in future
    offset: i64,
    record_length: usize,
    file_type: String,
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
}

impl Inode {
    fn new(number: u64, size: u64, permissions: Permissions, user_id: u32, group_id: u32, ctime: u64, mtime: u64, atime: u64, kind: InodeKind) -> Self {
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
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone, Hash, Eq)]
pub struct Permissions {
    owner: Permission,
    group: Permission,
    other: Permission,
}

#[derive(Debug, Default, PartialEq, Clone, Hash, Eq)]
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
    pub st_dev: u64,        // ID of device containing file, dummy field for now
    pub st_ino: u64,        // inode number
    pub st_mode: u16,       // protection
    pub st_nlink: u16,      // number of hard links
    pub st_uid: u32,        // user ID of owner
    pub st_gid: u32,        // group ID of owner
    pub st_rdev: u64,       // device ID (if special file), dummy field for now
    pub st_size: i64,       // total size, in bytes
    pub st_blksize: i32,    // blocksize for file system I/O
    pub st_blocks: i64,     // number of 512B blocks allocated, dummy field for now
    pub st_atime: i64,      // time of last access, dummy field for now
    pub st_mtime: i64,      // time of last modification, dummy field for now
    pub st_ctime: i64,      // time of last status change, dummy field for now
}

pub type FileDescriptor = i32;
pub type DirectoryDescriptor = i32;

#[derive(Debug, Clone)]
pub struct File {
    pub inode: Inode,
    pub data: Vec<u8>,
    pub position: u64,
    //pub path: PathBuf
    pub dirents: Vec<DirectoryEntry>,
}

#[derive(Clone)]
pub struct OpenFile {
    pub file: File,
    pub position: u64,
}

pub struct OpenDirectory {
    pub directory: Inode,
    pub position: usize,
}

impl Read for OpenFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let file_data = &self.file.data;
        let bytes_to_read = std::cmp::min(buf.len(), file_data.len() - self.position as usize);
        buf[..bytes_to_read].copy_from_slice(&file_data[self.position as usize..(self.position + bytes_to_read as u64) as usize]);
        self.position += bytes_to_read as u64;
        Ok(bytes_to_read)
    }
}

impl Write for OpenFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let file_data = &mut self.file.data;
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
        let file_data = &mut self.file.data;
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
    pub files: HashMap<Inode, File>,
    pub next_inode_number: u64,
    pub open_files: HashMap<FileDescriptor, OpenFile>,
    pub next_fd: FileDescriptor,
    pub next_directory_descriptor: DirectoryDescriptor,
    pub open_directories: HashMap<DirectoryDescriptor, OpenDirectory>,
    pub current_directory: PathBuf,
    pub root_inode: Inode
}

impl FileSystem {
    pub fn new() -> Self {

         // Create root directory inode, and first file (directory)
        let inode = Inode::new(1, 0, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::Directory);
        let file = File { inode: inode.clone(), data: Vec::new(), position: 0, dirents: vec![] };

        let mut fs = Self {
            files: HashMap::new(),
            next_inode_number: 1,
            open_files:  HashMap::new(),
            next_fd: 1,
            next_directory_descriptor: 1,
            open_directories:   HashMap::new(),
            current_directory: PathBuf::from("/"),
            root_inode: inode.clone(),
        };

        fs.next_inode_number += 1;
        fs.files.insert(inode.clone(), file);

        fs
    }

    fn create_directory(&mut self) -> Inode {
        let inode = Inode::new(self.next_inode_number, 0, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::Directory);
        let file = File { inode: inode.clone(), data: Vec::new(), position: 0, dirents: vec![] };
        self.files.insert(inode.clone(), file);
        self.next_inode_number += 1;
        inode
    }

    // --------------------
    // File Operations
    // --------------------


    fn get_inode(&self, fd: FileDescriptor) -> Result<&Inode, &'static str> {
        let open_file = self.open_files.get(&fd).ok_or("invalid file descriptor")?;
        Ok(&open_file.file.inode)
    }

    fn get_file(&self, fd: FileDescriptor) -> Result<File, &'static str> {
        let inode = Inode { number: fd as u64, ..Default::default() };
        self.files.get(&inode).ok_or("invalid file descriptor")
            .map(|file| file.clone()) // need to clone?
    }

    fn construct_full_path(&self, dir_inode: &Inode, path: &PathBuf) -> Result<PathBuf, &'static str> {
        let dir_entry = self.get_directory_entry(dir_inode.number)?;
        let mut full_path = dir_entry.name.clone();
        let combined_full_path = full_path + &path.clone().into_os_string().into_string().unwrap();
        Ok(PathBuf::from(combined_full_path))
    }

    fn get_directory_entry(&self, inode_number: u64) -> Result<DirectoryEntry, &'static str> {
        for file in self.files.values() {
            for dirent in &file.dirents {
                if dirent.inode.number == inode_number {
                    return Ok(dirent.clone());
                }
            }
        }
        Err("inode not found")
    }

    pub fn create_file(&mut self, raw_data: Vec<u8>, path: &PathBuf) -> Inode {
        let inode = Inode::new(1, 1024, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::File);
        let data = raw_data;
        let data_len = data.clone().len();
        let file = File { inode: inode.clone(), data, position: 0, dirents: vec![] };
        self.files.insert(inode.clone(), file);
        self.next_inode_number += 1;

        // Add new file and update the with the relevant directory entries
        if let Some(parent_path) = path.parent() {
            if let Some(parent_inode) = self.lookup_inode(&parent_path.into()) {
                if let Some(parent_file) = self.files.get_mut(&parent_inode) {
                    if let InodeKind::Directory = &mut parent_file.inode.kind {
                        let dirent = DirectoryEntry { inode: inode.clone(), offset: parent_file.dirents.len() as i64, record_length: data_len , file_type: "file".to_string(), name: path.file_name().unwrap().to_str().unwrap().to_string() };
                        parent_file.dirents.push(dirent);
                    }
                }
            }
        }

        inode
    }

    pub fn lookup_inode(&mut self, path: &PathBuf) -> Option<Inode> {
        let components = path.components().collect::<Vec<_>>();
        let mut current_inode = self.root_inode.clone();

        for component in components {
            let filename = component.as_os_str().to_str().unwrap();
            let file = self.files.get(&current_inode).unwrap();

            if let Some(dirent) = file.dirents.iter().find(|dirent| dirent.name == filename) {
                current_inode = Inode { number: dirent.inode.number, ..Default::default() };
            } else {
                return None;
            }
        }

        Some(current_inode)
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
        let full_path = path;
        if create {
            self.creat(&full_path)
        } else {
            let entry = dir_file.dirents.iter().find(|entry| entry.name == path.to_str().unwrap());
            match entry {
                Some(entry) => {
                    let file = self.files.get(&entry.inode).ok_or("file not found")?;
                    let open_file = OpenFile {
                        file: file.clone(),
                        position: 0,
                    };
                    let fd = self.next_fd;
                    self.next_fd += 1;
                    self.open_files.insert(fd, open_file);
                    Ok(fd)
                },
                None => Err("file not found"),
            }
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


    // --------------------
    // File Reading/Writing
    // --------------------


    pub fn read(&mut self, fd: FileDescriptor, buf: &mut [u8]) -> Result<usize, &'static str> {
        let open_file = self.open_files.get_mut(&fd).ok_or("invalid file descriptor")?;
        open_file.read(buf).map_err(|_| "read error")
    }

    pub fn write(&mut self, fd: FileDescriptor, buf: &[u8]) -> Result<usize, &'static str> {
        let open_file = self.open_files.get_mut(&fd).ok_or("invalid file descriptor")?;
        open_file.write(buf).map_err(|_| "write error")
    }

    pub fn pread64(&mut self, fd: FileDescriptor, buf: &mut [u8], offset: u64) -> Result<usize, &'static str> {
        let open_file = self.open_files.get_mut(&fd).ok_or("invalid file descriptor")?;
        let current_position = open_file.position;
        open_file.position = offset;
        let result = open_file.read(buf).map_err(|_| "read error");
        open_file.position = current_position;
        result
    }

    pub fn pwrite64(&mut self, fd: FileDescriptor, buf: &[u8], offset: u64) -> Result<usize, &'static str> {
        let open_file = self.open_files.get_mut(&fd).ok_or("invalid file descriptor")?;
        let current_position = open_file.position;
        open_file.position = offset;
        let result = open_file.write(buf).map_err(|_| "write error");
        open_file.position = current_position;
        result
    }

    pub fn sendfile(&mut self, out_fd: FileDescriptor, in_fd: FileDescriptor, offset: Option<&mut i64>, count: usize) -> Result<usize, &'static str> {
        self.splice(in_fd, None, out_fd, None, count, 0)
    }

    pub fn splice(&mut self, fd_in: FileDescriptor, off_in: Option<&mut u64>, fd_out: FileDescriptor, off_out: Option<&mut u64>, len: usize, flags: u32) -> Result<usize, &'static str> {
        let mut buf = vec![0; len];
        let mut total_bytes = 0;

        if let Some(offset) = off_in {
            let mut file_in = self.open_files.get_mut(&fd_in).ok_or("invalid input file descriptor")?;
            file_in.lseek(*offset as i64, 0)?;
            total_bytes += file_in.read(&mut buf).unwrap();
            *offset += total_bytes as u64;
        } else {
            let mut file_in = self.open_files.get_mut(&fd_in).ok_or("invalid input file descriptor")?;
            total_bytes += file_in.read(&mut buf).unwrap();
        }

        if let Some(offset) = off_out {
            let mut file_out = self.open_files.get_mut(&fd_out).ok_or("invalid output file descriptor")?;
            file_out.lseek(*offset as i64, 0)?;
            total_bytes += file_out.write(&buf).unwrap();
            *offset += total_bytes as u64;
        } else {
            let mut file_out = self.open_files.get_mut(&fd_out).ok_or("invalid output file descriptor")?;
            total_bytes += file_out.write(&buf).unwrap();
        }

        Ok(total_bytes)
    }

    pub fn fstat(&self, fd: FileDescriptor) -> Result<Stat, &'static str> {
        let file = self.get_file(fd)?;
        let inode = &file.inode;
        Ok(Stat {
            st_dev: 0,
            st_ino: inode.number,
            st_mode: 0,
            st_nlink: 1,
            st_uid: inode.user_id,
            st_gid: inode.group_id,
            st_rdev: 0,
            st_size: inode.size as i64,
            st_blksize: 0,
            st_blocks: 0,
            st_atime: inode.atime as i64,
            st_mtime: inode.mtime as i64,
            st_ctime: inode.ctime as i64,
        })
    }

    pub fn stat(&mut self, path: &PathBuf) -> Result<Stat, &'static str> {
        let inode = self.lookup_inode(path).ok_or("file not found")?;
        let file = self.files.get(&inode).ok_or("file not found")?;
        let inode = &file.inode;
        Ok(Stat {
            st_dev: 0,
            st_ino: inode.number,
            st_mode: 0,
            st_nlink: 1,
            st_uid: inode.user_id,
            st_gid: inode.group_id,
            st_rdev: 0,
            st_size: inode.size as i64,
            st_blksize: 0,
            st_blocks: 0,
            st_atime: inode.atime as i64,
            st_mtime: inode.mtime as i64,
            st_ctime: inode.ctime as i64,
        })
    }

    pub fn lstat(&mut self, path: &PathBuf) -> Result<Stat, &'static str> {
        let inode = self.lookup_inode(path).ok_or("file not found")?;
        let file = self.files.get(&inode).ok_or("file not found")?;

        let st_mode = match file.inode.kind {
            InodeKind::File => 0o100000, // regular file
            InodeKind::Directory => 0o040000, // directory
            InodeKind::SymbolicLink(_) => 0o120000, // symbolic link
        };

        Ok(Stat {
            st_dev: 0,
            st_ino: file.inode.number,
            st_mode: st_mode,
            st_nlink: 1,
            st_uid: file.inode.user_id,
            st_gid: file.inode.group_id,
            st_rdev: 0,
            st_size: file.inode.size as i64,
            st_blksize: 512,
            st_blocks: 0,
            st_atime: file.inode.atime as i64,
            st_mtime: file.inode.mtime as i64,
            st_ctime: file.inode.ctime as i64,
        })
    }

     pub fn fstatat(&mut self, dir_fd: FileDescriptor, path: &PathBuf) -> Result<Stat, &'static str> {
        let dir_inode = self.get_inode(dir_fd)?;
        if let InodeKind::Directory = dir_inode.kind {
            let full_path = self.construct_full_path(dir_inode, path)?;
            self.stat(&full_path)
        } else {
            Err("not a directory")
        }
    }

    pub fn getcwd(&self) -> PathBuf {
        self.current_directory.clone()

        // note: on libc level, will need to figure out how to handle non-unnnicode chars if returning a string
        // a possible way: self.current_directory.to_str().ok_or("Non-Unicode path").map(|s| s.to_string())
    }

      pub fn chdir(&mut self, path: &PathBuf) -> Result<(), &'static str> {
        // Check if the path exists and is a directory
        let inode = self.lookup_inode(path).ok_or("directory not found")?;
        let file = self.files.get(&inode).ok_or("directory not found")?;
        match file.inode.kind {
            InodeKind::Directory => {
                self.current_directory = path.clone();
                Ok(())
            },
            _ => Err("not a directory"),
        }
    }

    pub fn fchdir(&mut self, fd: FileDescriptor) -> Result<(), &'static str> {
        let open_file = self.open_files.get(&fd).ok_or("invalid file descriptor")?;
        let inode = &open_file.file.inode;

        match inode.kind {
            InodeKind::Directory => {
                self.current_directory = inode.number;
                Ok(())
            }
            _ => Err("not a directory"),
        }
    }


    pub fn getdents(&self, fd: DirectoryDescriptor, dirp: &mut [u8]) -> Result<usize, &'static str> {
         unimplemented!();
    }

    pub fn getdents64(&self, fd: DirectoryDescriptor, dirp: &mut [u8]) -> Result<usize, &'static str> {
         unimplemented!();
    }

    pub fn mkdir(&mut self, parent_fd: FileDescriptor, name: &str) -> Result<(), &'static str> {
        let parent_file = self.get_file(parent_fd)?;
        if let InodeKind::Directory = parent_file.inode.kind {
            let new_dir_inode = self.create_directory();
            let new_dir_entry = DirectoryEntry { inode: new_dir_inode.clone(), offset: 0 as i64, record_length: 0, file_type: "dir".to_string(), name: name.to_string() };
            let parent_file = self.files.get_mut(&parent_file.inode).ok_or("invalid file descriptor")?;
            parent_file.dirents.push(new_dir_entry);
            Ok(())
        } else {
            Err("not a directory")
        }
    }

    pub fn rmdir(&mut self, parent_fd: FileDescriptor, name: &str) -> Result<(), &'static str> {
        let parent_file = self.get_file(parent_fd)?;
        if let InodeKind::Directory = parent_file.inode.kind {
            let parent_file = self.files.get_mut(&parent_file.inode).ok_or("invalid file descriptor")?;
            let index = parent_file.dirents.iter().position(|x| x.name == name).ok_or("directory not found")?;
            let dir_entry = parent_file.dirents.remove(index);
            self.files.remove(&dir_entry.inode);
            Ok(())
        } else {
            Err("not a directory")
        }
    }

    pub fn unlink(&mut self, parent_fd: FileDescriptor, name: &str) -> Result<(), &'static str> {
        let parent_file = self.get_file(parent_fd)?;
        if let InodeKind::Directory = parent_file.inode.kind {
            let parent_file = self.files.get_mut(&parent_file.inode).ok_or("invalid file descriptor")?;
            let index = parent_file.dirents.iter().position(|x| x.name == name).ok_or("file not found")?;
            let file_entry = parent_file.dirents.remove(index);
            self.files.remove(&file_entry.inode);
            Ok(())
        } else {
            Err("not a directory")
        }
    }

    pub fn rename(&mut self, old_path: &PathBuf, new_path: &PathBuf) -> Result<(), &'static str> {
        let inode = self.lookup_inode(old_path).ok_or("file not found")?;

        let file = self.files.get_mut(&inode).ok_or("file not found")?;

        // Update the DirectoryEntry in the parent directory
        if let Some(parent_path) = old_path.parent() {
            if let Some(parent_inode) = self.lookup_inode(&parent_path.into()) {
                if let Some(parent_file) = self.files.get_mut(&parent_inode) {
                    if let InodeKind::Directory = parent_file.inode.kind {
                        if let Some(entry) = parent_file.dirents.iter_mut().find(|entry| entry.inode == inode) {
                            entry.name = new_path.file_name().unwrap().to_str().unwrap().to_string();
                        }
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
            InodeKind::Directory => (),
            _ => return Err("Not a directory"),
        };

        let directory_descriptor = self.next_directory_descriptor;
        self.next_directory_descriptor += 1;

        let open_directory = OpenDirectory {
            directory: inode.clone().into(),
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
