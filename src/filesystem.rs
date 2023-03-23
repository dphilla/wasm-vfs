use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::PathBuf;

pub type Inode = u64;

// future implementation
//struct Inode {
    //number: u64,
    //size: u64,
    //permissions: Permissions,
    //user_id: u32,
    //group_id: u32,
    //ctime: SystemTime,
    //mtime: SystemTime,
    //atime: SystemTime,
    // etc.
//}

pub type FileDescriptor = i32;

#[derive(Debug)]
struct File {
    inode: Inode,
    data: Mutex<Vec<u8>>,
    path: PathBuf

}

struct OpenFile {
    file: Arc<File>,
    position: u64,
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

struct FileSystem {
    files: HashMap<Inode, Arc<File>>,
    inodes: Inode,
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            inodes: 1,
        }
    }

    pub fn lookup_inode(&mut self, path: &PathBuf) -> Option<Inode> {
        let inode = self.files
            .iter()
            .find_map(|(&inode, file)| if file.path == *path { Some(inode) } else { None });
        if inode == None {
            return Some(self.create_file(Vec::new()))
        } else {
            None
        }
    }

    pub fn create_file(&mut self, raw_data: Vec<u8>) -> Inode {
        let inode = self.inodes;
        let data = Mutex::new(raw_data);
        let mut path = PathBuf::new();
        let file = File { inode, data, path};
        self.files.insert(inode, Arc::new(file));
        self.inodes += 1;
        inode
    }
}
