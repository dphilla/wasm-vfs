use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum InodeKind {
    #[default]
    File,
    Directory,
    SymbolicLink(PathBuf),
}

// In a unix filesystems, the field below would likely
// be an i_block, with one or more pointers to the actual
// blocks where the data is stored. This is easier/less
// confusing for our purposes
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum FileKind {
    #[default]
    File,
    DirectoryFile,
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
    pub fn new(number: u64, size: u64, permissions: Permissions, user_id: u32, group_id: u32,
        ctime: u64, mtime: u64, atime: u64, kind: InodeKind) -> Self {
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

#[derive(Debug, Default, PartialEq, Clone, Hash, Eq, Serialize )]
pub struct Permissions {
    owner: Permission,
    group: Permission,
    other: Permission,
}

#[derive(Debug, Default, PartialEq, Clone, Hash, Eq, Serialize)]
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
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_mode: u16,
    pub st_nlink: u16,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i32,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_mtime: i64,
    pub st_ctime: i64,
}

pub struct FileSystem {
    // In a real FS, `inodes` would be indexed by inode number.
    // We'll ensure that the index in `inodes` matches the inode number:
    // i.e. inode.number == index.
    pub inodes: Vec<Inode>,
    pub next_inode_number: u64,
    pub current_directory: PathBuf,
    pub root_inode: Inode,

    // File data by inode number
    pub files: HashMap<u64, Vec<u8>>,

    // Mapping from full path to inode number
    pub path_map: HashMap<PathBuf, u64>,
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
            files: HashMap::new(),
            path_map: HashMap::new(),
        };

        // Insert root directory into path_map
        fs.path_map.insert(PathBuf::from("/"), 0);

        fs
    }

    pub fn lookup_inode_by_path(&self, path: &PathBuf) -> Option<u64> {
        self.path_map.get(path).cloned()
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
        self.files.insert(inode_number, vec![]);

        inode_number
    }
}
