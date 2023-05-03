use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::io::{Read, Write};
use std::path::PathBuf;
use crate::filesystem::*;

pub struct Process {
    open_files: HashMap<FileDescriptor, Arc<OpenFile>>,
    fds: FileDescriptor,
}

impl Process {
    pub fn new() -> Self {
        Self {
            open_files: HashMap::new(),
            fds: 0,
        }
    }

    pub fn open(&mut self, fs: &mut FileSystem, path: &PathBuf) -> Result<FileDescriptor, ()> {
        let inode = fs.lookup_inode(path).ok_or(())?;
        for (&fd, open_file) in self.open_files.iter() {
            if open_file.file.inode.number == inode.number {
                // File is already open, return the existing FileDescriptor
                return Ok(fd);
            }
        }

        let file = fs.files.get(&inode).ok_or(())?;
        let fd = self.fds;
        self.open_files.insert(
            fd,
            Arc::new(OpenFile {
                file: Arc::clone(file),
                position: 0,
            }),
        );
        self.fds += 1;
        Ok(fd)
    }

    pub fn read(&mut self, fd: FileDescriptor, buf: &mut [u8]) -> Result<usize, ()> {
        let open_file = self.open_files.get(&fd).ok_or(())?;
        open_file.read(buf).map_err(|_| ())
    }

    pub fn write(&mut self, fd: FileDescriptor, buf: &[u8]) -> Result<usize, ()> {
        let open_file = self.open_files.get(&fd).ok_or(())?;
        open_file.write(buf).map_err(|_| ())
    }

    pub fn close(&mut self, fd: FileDescriptor) -> Result<i32, i32> {
        if self.open_files.remove(&fd).is_some() {
            Ok(0)
        } else {
            Err(-1)
        }
    }
}

