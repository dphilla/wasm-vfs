#[cfg(test)]
mod tests {
    use super::*;

    //OpenFile tests
    //-------------

     fn read_returns_data_from_file() {
        let data = vec![0, 1, 2, 3, 4, 5];
        let file = File {
            inode: Inode::new(1, data.len() as u64, Permissions::default(), 0, 0, 0, 0, 0),
            data: Mutex::new(data.clone()),
            position: 0,
            path: PathBuf::new(),
        };
        let open_file = OpenFile {
            file: Arc::new(file),
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
            inode: Inode::new(1, data.len() as u64, Permissions::default(), 0, 0, 0, 0, 0),
            data: Mutex::new(data.clone()),
            position: 0,
            path: PathBuf::new(),
        };
        let open_file = OpenFile {
            file: Arc::new(file),
            position: 3,
        };
        let bytes_written = open_file.write(&[3, 4, 5]).unwrap();
        assert_eq!(bytes_written, 3);
        let file_data = open_file.file.data.lock().unwrap();
        assert_eq!(*file_data, vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn lseek_sets_file_position() {
        let data = vec![0, 1, 2, 3, 4, 5];
        let file = File {
            inode: Inode::new(1, data.len() as u64, Permissions::default(), 0, 0, 0, 0, 0),
            data: Mutex::new(data.clone()),
            position: 0,
            path: PathBuf::new(),
        };
        let open_file = OpenFile {
            file: Arc::new(file),
            position: 0,
        };
        let new_pos = open_file.lseek(3, 0).unwrap();
        assert_eq!(new_pos, 3);
        let mut buf = [0u8; 3];
        let bytes_read = open_file.read(&mut buf).unwrap();
        assert_eq!(bytes_read, 3);
        assert_eq!(buf, [3, 4, 5]);
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
        let inode = fs.lookup_inode(&path).unwrap();

        let fd = inode.number as i32;
        let stat = fs.fstat(fd).unwrap();

        assert_eq!(stat.st_ino, inode.number);
    }

    #[test]
    fn test_filesystem_stat() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test/file");
        let inode = fs.lookup_inode(&path).unwrap();

        let stat = fs.stat(&path).unwrap();

        assert_eq!(stat.st_ino, inode.number);
    }

    #[test]
    fn test_create_file() {
        let mut fs = FileSystem::new();
        let file_data = vec![1, 2, 3, 4, 5];
        let inode = fs.create_file(file_data.clone(), Path::new("/file1").to_path_buf()).unwrap();
        let file = fs.files.get(&inode).unwrap().clone();
        assert_eq!(*file.data.lock().unwrap(), file_data);
    }

    #[test]
    fn test_unlink() {
        let mut fs = FileSystem::new();
        let inode = fs.create_file(vec![1, 2, 3, 4, 5], Path::new("/file2").to_path_buf()).unwrap();
        fs.unlink(&Path::new("/file2").to_path_buf()).unwrap();
        assert!(fs.files.get(&inode).is_none());
    }

    #[test]
    fn test_rename() {
        let mut fs = FileSystem::new();
        fs.create_file(vec![1, 2, 3, 4, 5], Path::new("/file3").to_path_buf()).unwrap();
        fs.rename(&Path::new("/file3").to_path_buf(), &Path::new("/file4").to_path_buf()).unwrap();
        let inode = fs.lookup_inode(&Path::new("/file4").to_path_buf()).unwrap();
        let file = fs.files.get(&inode).unwrap().clone();
        assert_eq!(file.path, Path::new("/file4").to_path_buf());
    }

    //FS dir tests
    //------------

    #[test]
    fn test_mkdir_rmdir() {
        let mut fs = FileSystem::new();
        fs.mkdir(&Path::new("/dir1").to_path_buf()).unwrap();
        let inode = fs.lookup_inode(&Path::new("/dir1").to_path_buf()).unwrap();
        let file = fs.files.get(&inode).unwrap().clone();
        assert!(matches!(file.inode.kind, InodeKind::Directory(_)));
        fs.rmdir(&Path::new("/dir1").to_path_buf()).unwrap();
        assert!(fs.files.get(&inode).is_none());
    }

     #[test]
    fn test_opendir() {
        let mut fs = FileSystem::new();

        // Create a new directory
        let dir_path = PathBuf::from("/test_dir");
        fs.mkdir(&dir_path).expect("Failed to create directory");

        // Open the directory
        let dir_descriptor = fs.opendir(&dir_path).expect("Failed to open directory");

        // Verify the directory is opened
        assert_eq!(dir_descriptor, 1);
        assert!(fs.open_directories.contains_key(&dir_descriptor));

        let open_dir = fs.open_directories.get(&dir_descriptor).unwrap();
        assert_eq!(Arc::ptr_eq(&open_dir.directory, fs.files.get(&1).unwrap()), true);
        assert_eq!(open_dir.position, 0);
    }
}
