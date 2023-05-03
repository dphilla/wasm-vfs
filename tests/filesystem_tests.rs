#[cfg(test)]
mod tests {
    use super::*;

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
