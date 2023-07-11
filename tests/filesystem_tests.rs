#[cfg(test)]
mod tests {
    use wasm_vfs::{FileSystem, File, InodeKind, Inode, OpenFile, Permissions, DirectoryEntry};
    use std::path::PathBuf;
    use std::path::Path;


    //OpenFile tests
    //-------------

    fn read_returns_data_from_file() {
        let data = vec![0, 1, 2, 3, 4, 5];
        let file = File {
            inode: Inode::new(1, data.len() as u64, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::File),
            data: data.clone(),
            position: 0,
            dirents: vec![],
        };
        let mut open_file = OpenFile {
            file: file,
            position: 0,
        };
        let mut buf = [0u8; 3];
        let bytes_read = open_file.read(&mut buf).unwrap();
        assert_eq!(bytes_read, 3);
        assert_eq!(buf, [0, 1, 2]);
    }

    #[test]
    fn write_appends_data_to_file() {
        let data = vec![0, 1, 2];
        let file = File {
            inode: Inode::new(1, data.len() as u64, Permissions::default(), 0, 0, 0, 0, 0, InodeKind::File),
            data: data.clone(),
            position: 0,
            dirents: vec![],
        };
        let mut open_file = OpenFile {
            file: file,
            position: 3,
        };
        let bytes_written = open_file.write(&[3, 4, 5]).unwrap();
        assert_eq!(bytes_written, 3);
        let file_data = open_file.file.data;
        assert_eq!(*file_data, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_lseek() {
        let inode = Inode::new(1, 10, Permissions::from(0o755), 0, 0, 0, 0, 0, InodeKind::File);
        let mut file = File {
            inode,
            data: vec![0; 10],
            position: 0,
            dirents: vec![],
        };
        let mut open_file = OpenFile {
            file,
            position: 0,
        };

        // Test SEEK_SET
        assert_eq!(open_file.lseek(5, 0).unwrap(), 5);
        assert_eq!(open_file.position, 5);

        // Test SEEK_CUR
        assert_eq!(open_file.lseek(2, 1).unwrap(), 7);
        assert_eq!(open_file.position, 7);

        // Test SEEK_END
        assert_eq!(open_file.lseek(-2, 2).unwrap(), 8);
        assert_eq!(open_file.position, 8);

        // Test invalid whence
        assert!(open_file.lseek(0, 3).is_err());

        // Test invalid position
        assert!(open_file.lseek(20, 0).is_err());
        assert!(open_file.lseek(-20, 0).is_err());
    }


    //FS file tests
    //-------------

    #[test]
    fn test_filesystem_lookup_inode() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test/file");
        let inode = fs.lookup_inode(&path);
        assert!(inode.is_some());

        let path2 = PathBuf::from("/test/file2");
        let inode2 = fs.lookup_inode(&path2);
        assert!(inode2.is_some());
        assert_ne!(inode, inode2);
    }

    #[test]
    fn test_filesystem_fstat() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test/file");
        let mut inode = fs.lookup_inode(&path);

        let fd = inode.clone().unwrap().number as i32;
        let stat = fs.fstat(fd.clone()).unwrap();

        assert_eq!(stat.st_ino, inode.unwrap().number);
    }

    #[test]
    fn test_filesystem_stat() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test/file");
        let inode = fs.lookup_inode(&path);

        let stat = fs.stat(&path).unwrap();

        assert_eq!(stat.st_ino, inode.unwrap().number);
    }

     #[test]
    fn test_lstat() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/testfile");
        let permissions = Permissions::from(0o644);
        let inode = fs.create_file(vec![1, 2, 3, 4, 5], &path);

        let stat = fs.lstat(&path).unwrap();

        assert_eq!(stat.st_ino, inode.number);
        assert_eq!(stat.st_mode, 0o100000); // regular file
    }

    #[test]
    fn test_fstatat() {
        let mut fs = FileSystem::new();

        // Create a file
        let file_path = PathBuf::from("/test_dir/test_file");
        let file_inode = fs.create_file(vec![1, 2, 3, 4, 5], &file_path);

        // Open the parent directory
        let dir_path = PathBuf::from("/test_dir");
        let dir_fd = fs.open(&dir_path).unwrap();

        // Call fstatat on the file
        let file_stat = fs.fstatat(dir_fd, &PathBuf::from("test_file")).unwrap();

        // Check the results
        assert_eq!(file_stat.st_ino, file_inode.number);
        assert_eq!(file_stat.st_size, 5);
    }

    #[test]
    fn test_getcwd() {
        let mut fs = FileSystem::new();

        // The initial working directory should be root.
        assert_eq!(fs.getcwd().into_os_string().into_string().unwrap(), "/");

        // Change the working directory and check again.
        fs.current_directory = PathBuf::from("/home/user");
        assert_eq!(fs.getcwd().into_os_string().into_string().unwrap(), "/home/user");
    }

     #[test]
    fn test_chdir() {
        let mut fs = FileSystem::new();

        // Create a directory
        let mut dir_path = PathBuf::from("/test_dir");
        fs.mkdir(&mut dir_path).unwrap();

        // Change to the new directory
        fs.chdir(&dir_path).unwrap();

        // Check that the current directory has been updated
        assert_eq!(fs.getcwd(), dir_path);
    }

    #[test]
    fn test_chdir_nonexistent() {
        let mut fs = FileSystem::new();

        // Try to change to a nonexistent directory
        let dir_path = PathBuf::from("/nonexistent_dir");
        assert!(fs.chdir(&dir_path).is_err());

        // Check that the current directory has not been updated
        assert_eq!(fs.getcwd(), PathBuf::from("/"));
    }

    #[test]
    fn test_chdir_not_a_directory() {
        let mut fs = FileSystem::new();

        // Create a file
        let file_path = PathBuf::from("/test_file");
        fs.create_file(vec![], &file_path);

        // Try to change to the file
        assert!(fs.chdir(&file_path).is_err());

        // Check that the current directory has not been updated
        assert_eq!(fs.getcwd(), PathBuf::from("/"));
    }

    #[test]
    fn test_fchdir() {
        let mut fs = FileSystem::new();

        // Create a directory
        let mut dir_path = PathBuf::from("/dir");
        let dir_inode = fs.mkdir(&mut dir_path);

        // Open the directory
        let dir_fd = fs.open(&dir_path).unwrap();

        // Change the current directory to the directory
        fs.fchdir(dir_fd).unwrap();

        // Check that the current directory has been changed
        assert_eq!(fs.getcwd().into_os_string().into_string().unwrap(), "/dir");

        // Try to change the current directory to a file
        let file_path = PathBuf::from("/dir/file");
        fs.create_file(Vec::new(), &file_path);
        let file_fd = fs.open(&file_path).unwrap();
        assert_eq!(fs.fchdir(file_fd), Err("not a directory"));
    }

    #[test]
    fn test_getdents() {
        let mut fs = FileSystem::new();

        // Create a directory
        let mut dir_path = PathBuf::from("/test_dir");
        fs.mkdir(&mut dir_path).unwrap();

        // Open the directory
        let dir_fd = fs.opendir(&dir_path).unwrap();

        // Create a file in the directory
        let file_path = dir_path.join("test_file");
        fs.creat(&file_path).unwrap();

        // Get directory entries
        let mut buf = vec![0; 1024];
        let bytes_read = fs.getdents(dir_fd, &mut buf).unwrap();

        // Check that bytes_read is not 0 (since we have at least one entry in the directory)
        assert!(bytes_read > 0);

        // Check that the first entry is our file
        let entry: DirectoryEntry = unsafe { std::ptr::read(buf.as_ptr() as *const _) };
        assert_eq!(entry.name, "test_file");
    }

    #[test]
    fn test_create_file() {
        let mut fs = FileSystem::new();
        let file_data = vec![1, 2, 3, 4, 5];
        let inode = fs.create_file(file_data.clone(), &Path::new("/file1").to_path_buf());
        let file = fs.files.get(&inode).unwrap().clone();
        assert_eq!(file.data, file_data);
    }

    #[test]
    fn test_unlink() {
        let mut fs = FileSystem::new();
        let inode = fs.create_file(vec![1, 2, 3, 4, 5], &Path::new("/file2").to_path_buf());
        fs.unlink(&mut Path::new("/file2").to_path_buf()).unwrap();
        assert!(fs.files.get(&inode).is_none());
    }

    #[test]
    fn test_rename() {
        let mut fs = FileSystem::new();
        fs.create_file(vec![1, 2, 3, 4, 5], &Path::new("/file3").to_path_buf());
        fs.rename(&mut Path::new("/file3").to_path_buf(), &Path::new("/file4").to_path_buf()).unwrap();
        let inode = fs.lookup_inode(&Path::new("/file4").to_path_buf());
        let file = fs.files.get(&inode.unwrap()).unwrap();
    }

    // FS descriptor management tests

    #[test]
    fn test_open_close() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test");

        // Open a non-existent file should fail
        assert!(fs.open(&path).is_err());

        // Create a new file
        fs.creat(&path).unwrap();

        // Open an existing file should succeed
        let fd = fs.open(&path).unwrap();

        // Close the file should succeed
        assert!(fs.close(fd).is_ok());

        // Close the file again should fail
        assert!(fs.close(fd).is_err());
    }

    #[test]
    fn test_creat() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test");

        // Create a new file should succeed
        let fd = fs.creat(&path).unwrap();

        // Close the file should succeed
        assert!(fs.close(fd).is_ok());

        // Create the same file again should fail
        assert!(fs.creat(&path).is_err());
    }

    #[test]
    fn test_openat() {
        let mut fs = FileSystem::new();
        let mut dir_path = PathBuf::from("/dir");
        let file_path = PathBuf::from("file");

        // Create a new directory
        fs.mkdir(&mut dir_path);

        // Open a directory should fail
        assert!(fs.open(&dir_path).is_err());

        // Open a file in the directory should fail
        let mut first_path = fs.open(&dir_path);
        assert!(fs.openat(first_path.unwrap(), &file_path).is_err());

        // Create a new file in the directory
        fs.creat(&dir_path.join(&file_path)).unwrap();

        // Open a file in the directory should succeed
        let mut second_path = fs.open(&dir_path).unwrap();
        let fd = fs.openat(second_path, &file_path).unwrap();

        // Close the file should succeed
        assert!(fs.close(fd).is_ok());
    }

    #[test]
    fn test_dup_dup2() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test");

        // Create a new file
        let fd = fs.creat(&path).unwrap();

        // Duplicate the file descriptor should succeed
        let new_fd = fs.dup(fd).unwrap();

        // Close the original file descriptor should succeed
        assert!(fs.close(fd).is_ok());

        // Close the duplicated file descriptor should succeed
        assert!(fs.close(new_fd).is_ok());

        // Duplicate a closed file descriptor should fail
        assert!(fs.dup(fd).is_err());

        // Duplicate to a specific file descriptor should succeed
        assert!(fs.dup2(fd, new_fd).is_ok());

        // Close the specific file descriptor should succeed
        assert!(fs.close(new_fd).is_ok());
    }


    // --------------------
    // File Reading/Writing
    // --------------------

    #[test]
    fn test_read_write() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/testfile");
        let fd = fs.creat(&path).unwrap();
        let data = b"Hello, world!";
        fs.write(fd, data).unwrap();
        let mut buf = vec![0; 13];
        fs.read(fd, &mut buf).unwrap();
        assert_eq!(&buf, data);
    }

    #[test]
    fn test_pread_pwrite() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/testfile");
        let fd = fs.creat(&path).unwrap();
        let data = b"Hello, world!";
        fs.pwrite64(fd, data, 0).unwrap();
        let mut buf = vec![0; 13];
        fs.pread64(fd, &mut buf, 0).unwrap();
        assert_eq!(&buf, data);
    }

    fn test_sendfile() {
        let mut fs = FileSystem::new();

        let path1 = PathBuf::from("/file1");
        let path2 = PathBuf::from("/file2");

        let fd1 = fs.creat(&path1).unwrap();
        let fd2 = fs.creat(&path2).unwrap();

        let data = b"Hello, world!";
        fs.write(fd1, data).unwrap();

        let count = fs.sendfile(fd2, fd1, None, data.len()).unwrap();
        assert_eq!(count, data.len());

        let mut buf = vec![0; data.len()];
        fs.read(fd2, &mut buf).unwrap();

        assert_eq!(&buf, data);
    }

    #[test]
    fn test_splice() {
        let mut fs = FileSystem::new();

        let path1 = PathBuf::from("/file1");
        let path2 = PathBuf::from("/file2");

        let fd1 = fs.creat(&path1).unwrap();
        let fd2 = fs.creat(&path2).unwrap();

        let data = b"Hello, world!";
        fs.write(fd1, data).unwrap();

        let count = fs.splice(fd1, None, fd2, None, data.len(), 0).unwrap();
        assert_eq!(count, data.len());

        let mut buf = vec![0; data.len()];
        fs.read(fd2, &mut buf).unwrap();

        assert_eq!(&buf, data);
    }

    //FS dir tests
    //------------

    #[test]
     fn test_mkdir_rmdir() {
        let mut fs = FileSystem::new();
        fs.mkdir(&mut Path::new("/dir1").to_path_buf()).unwrap();
        let inode = fs.lookup_inode(&Path::new("/dir1").to_path_buf());
        let file = fs.files.get(&inode.clone().unwrap()).unwrap().clone();
        assert!(matches!(file.inode.kind, InodeKind::Directory));
        fs.rmdir(&mut Path::new("/dir1").to_path_buf()).unwrap();
        assert!(fs.files.get(&inode.unwrap()).is_none());
    }

    #[test]
    fn test_opendir() {
        let mut fs = FileSystem::new();

        // Create a new directory
        let mut dir_path = PathBuf::from("/test_dir");
        fs.mkdir(&mut dir_path).expect("Failed to create directory");

        // Open the directory
        let dir_descriptor = fs.opendir(&dir_path).expect("Failed to open directory");

        // Verify the directory is opened
        assert_eq!(dir_descriptor, 1);
        assert!(fs.open_directories.contains_key(&dir_descriptor));

        let open_dir = fs.open_directories.get(&dir_descriptor).unwrap();
        assert_eq!(open_dir.position, 0);
    }
}
