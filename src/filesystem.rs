// filesystem.rs
#![allow(dead_code)]

use serde::{Serialize, Deserialize};

// In project implementations - replaces rust's std crates:
use crate::path::PathBuf;
use crate::collections::HashMap;

// In a unix filesystems, the field below would likely
// be an i_block, with one or more pointers to the actual
// blocks where the data is stored. This is easier/less
// confusing for our purposes
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum InodeKind {
    #[default]
    File,
    Directory,
    SymbolicLink(PathBuf),
}

#[derive(Debug, Default, PartialEq, Clone, Hash, Eq, Serialize )]
pub struct Permission {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Default, PartialEq, Clone, Hash, Eq, Serialize)]
pub struct Permissions {
    pub owner: Permission,
    pub group: Permission,
    pub other: Permission,
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
                read: (mode & 0o040) != 0,
                write: (mode & 0o020) != 0,
                execute: (mode & 0o010) != 0,
            },
            other: Permission {
                read: (mode & 0o004) != 0,
                write: (mode & 0o002) != 0,
                execute: (mode & 0o001) != 0,
            },
        }
    }
}

#[derive(Eq, Hash, PartialEq, Debug, Clone, Default, Serialize)]
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
    pub fn new(number: u64, size: u64, permissions: Permissions,
               user_id: u32, group_id: u32, ctime: u64, mtime: u64, atime: u64,
               kind: InodeKind) -> Self {
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

// A minimal capacity for your path_map, say 256:
const PATH_MAP_CAP: usize = 256;
const FILES_CAP: usize = 256;

#[derive(Debug)]
pub struct FileSystem {
    // In a linux vfs, `inodes` would be indexed by inode number.
    // We'll ensure that the index in `inodes` matches the inode number:
    // i.e. inode.number == index.
    pub inodes: Vec<Inode>,
    pub next_inode_number: u64,
    pub current_directory: PathBuf,
    pub root_inode: Inode,
    // Instead of std::collections::HashMap, we do custom HashMap
    pub files: HashMap<u64, Vec<u8>, FILES_CAP>,
    pub path_map: HashMap<PathBuf, u64, PATH_MAP_CAP>,
}

impl Default for FileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem {
    pub fn new() -> Self {
        let root_inode = Inode::new(
            0,
            0,
            Permissions::default(),
            0,
            0,
            0,
            0,
            0,
            InodeKind::Directory
        );
        let mut fs = Self {
            inodes: vec![root_inode.clone()],
            next_inode_number: 1,
            current_directory: PathBuf::from("/"),
            root_inode: root_inode,
            files: HashMap::init(),
            path_map: HashMap::init(),
        };
        // Insert root dir
        fs.path_map.insert(PathBuf::from("/"), 0);
        fs
    }

    pub fn lookup_inode_by_path(&self, path: &PathBuf) -> Option<u64> {
        self.path_map.get(path).copied()
    }

    pub fn create_file(&mut self, path: &PathBuf, mode: u32) -> u64 {
        let inode_number = self.next_inode_number;
        self.next_inode_number += 1;

        let inode = Inode::new(
            inode_number,
            0,
            Permissions::from(mode as u16),
            0,
            0,
            0,
            0,
            0,
            InodeKind::File
        );
        self.inodes.push(inode.clone());
        self.path_map.insert(path.clone(), inode_number);
        self.files.insert(inode_number, Vec::new());
        inode_number
    }
}

// POSIX-like Stat structure
#[repr(C)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_mtime: i64,
    pub st_ctime: i64,
}

// Linux-like dirent structures for getdents calls
#[repr(C)]
pub struct Dirent {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

#[repr(C)]
pub struct Dirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

