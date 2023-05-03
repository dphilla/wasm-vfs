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


    //FS tests
    //--------

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
    fn test_filesystem_create_file() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test/file");
        let raw_data = vec![1, 2, 3];
        let inode = fs.create_file(raw_data.clone());

        let file = fs.files.get(&inode).unwrap();
        assert_eq!(*file.data.lock().unwrap(), raw_data);
    }

    #[test]
    fn test_filesystem_unlink() {
        let mut fs = FileSystem::new();
        let path = PathBuf::from("/test/file");
        let inode = fs.lookup_inode(&path).unwrap();

        fs.unlink(&path).unwrap();

        let file = fs.files.get(&inode);
        assert!(file.is_none());
    }

    #[test]
    fn test_filesystem_rename() {
        let mut fs = FileSystem::new();
        let old_path = PathBuf::from("/test/file");
        let new_path = PathBuf::from("/test/file2");
        let inode = fs.lookup_inode(&old_path).unwrap();

        fs.rename(&old_path, &new_path).unwrap();

        let file = fs.files.get(&inode).unwrap();
        assert_eq!(file.path, new_path);
    }
}
