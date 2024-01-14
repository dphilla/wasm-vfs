use std::collections::HashMap; use std::io::{Read, Write};
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use bincode;

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum InodeKind {
    #[default]
    File,
    Directory,
    SymbolicLink(PathBuf),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum FileKind {
    File,
    DirectoryFile,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub inode: Inode, // refactor: just reference
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

    // In a unix filesystems, the field below would likely
    // be an i_block, with one or more pointers to the actual
    // blocks where the data is stored. This is easier/less
    // confusing for our purposes
    pub file: FileKind, // change to ref
}

impl Inode {
    pub fn new(number: u64, size: u64, permissions: Permissions, user_id: u32, group_id: u32, ctime: u64, mtime: u64, atime: u64, kind: InodeKind) -> Self {
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

#[derive(Debug, Clone)]
pub struct File {
    pub inode: u64,
    pub data: Vec<u8>,
    pub position: u64,
}

// In unix filesystems a Directory is actually (kinda )a file ("eveything is a file"...)
// and it is persisted as a Directory File (just 1 or more blocks of mem, like any other file's data),
// which contains a list of Directory Entries and corresponding inode_numbers
// that is read and used to determine name -> inode relationhips and thus hierarchy, etc.
// instead of that, we are just using the below dirents structure for expediency
#[derive(Debug, Clone)]
pub struct DirectoryFile {
    pub inode: u64,
    pub dirents: Vec<DirectoryEntry>,
}

#[derive(Debug)]
pub struct FileSystem {
    //  the inodes field here is a simplified form of a Unix Inode Table.
    //  See: https://exposnitc.github.io/os_design-files/disk_ds.html.
    //  This is crucial for constant time operations accessing file metadata,
    //  as **each index in this vec acts as each inode number**
    pub inodes: Vec<Inode>,
    pub next_inode_number: u64,
    pub current_directory: PathBuf,
    pub root_inode: Inode
}

impl FileSystem {
    pub fn new() -> Self {

         // Create root directory inode, and first file (directory), in future will be able to read
         // from external FS/block device, etc.
        let mut inode = Inode::new(0, 0, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::Directory);

        let root_dirents = vec![
            DirectoryEntry::new(name: ".", inode: inode.number),
            DirectoryEntry::new(name: "..", inode: inode.number)
        ];

        // START HERE, and build by hand: mkdir, and other simple, elemental things. Slowly move
        // things to process model that belong there -- the big Q: in unix, what is per process,
        // and what is always just in the FS, regardless of process

        // let directory_file = DirectoryFile { inode: inode.number, dirents: root_dirents };

        let mut fs = Self {
            // In many FSs, all possibly inodes are made on creation. Here, they
            // will be made with every new file, to conserve memory footprint
            inodes: Vec!(inode),
            next_inode_number: 1,
            current_directory: PathBuf::from("/"), // move to process
            root_inode: inode.clone(),
        };

        fs.next_inode_number += 1;
        //println!("{:#?}", fs);

        fs
    }

    pub fn construct_path(file: &File) -> PathBuf {
        let mut path = PathBuf::new();
        for dirent in &file.dirents {
            path.push(&dirent.name);
        }
        path
    }
}
