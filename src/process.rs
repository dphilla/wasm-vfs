use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::PathBuf;
use crate::filesystem::*;

pub struct Process {
    open_files: HashMap<FileDescriptor, OpenFile>,
    fds: FileDescriptor,
}

impl Process {
    pub fn new() -> Self {
        Self {
            open_files: HashMap::new(),
            fds: 0,
        }
    }

    pub fn open(&mut self, fs: &mut FileSystem, inode: Inode) -> Result<FileDescriptor, ()> {
        for (&fd, open_file) in self.open_files.iter() {
            if open_file.file.inode == inode {
                // File is already open, return the existing FileDescriptor
                return Ok(fd);
            }
        }

        // If the file is not open, check if it exists in the filesystem
        if let Some(file) = fs.files.get(&inode) {
            let fd = self.fds;
            self.open_files.insert(
                fd,
                OpenFile {
                    file: Arc::clone(file),
                    position: 0,
                },
            );
            self.fds += 1;
            return Ok(fd);
        }

        // If the inode is not provided or the file doesn't exist, create a new file
        let new_inode = fs.create_file(vec![]);
        let new_file = fs.files.get(&new_inode).ok_or(())?;
        let new_fd = self.fds;
        self.open_files.insert(
            new_fd,
            OpenFile {
                file: Arc::clone(new_file),
                position: 0,
            },
        );
        self.fds += 1;
        Ok(new_fd)
}


    pub fn read(&mut self, fd: FileDescriptor, buf: &mut [u8]) -> Result<usize, ()> {
        let open_file = self.open_files.get_mut(&fd).ok_or(())?;
        open_file.read(buf).map_err(|_| ())
    }

    pub fn write(&mut self, fd: FileDescriptor, buf: &[u8]) -> Result<usize, ()> {
       let open_file = self.open_files.get_mut(&fd).ok_or(())?;
       open_file.write(buf).map_err(|_| ())
    }

    pub fn close(&mut self, fd: FileDescriptor) -> Result<(), ()> {
        if self.open_files.remove(&fd).is_some() {
            Ok(())
        } else {
            Err(())
        }
    }
}

