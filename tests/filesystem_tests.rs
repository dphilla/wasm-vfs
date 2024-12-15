#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::ffi::CString;

    #[test]
    fn test_open_happy_path() {
        let path = CString::new("/testfile").unwrap();
        let flags = O_CREAT | O_RDWR;
        let mode = 0o644;

        // Create a file using open
        let fd = unsafe { open(path.as_ptr(), flags, mode) };
        assert!(fd >= 0, "Failed to create and open file");

        // Ensure the file descriptor table contains the entry
        let proc = get_or_init_proc();
        assert!(proc.as_ref().unwrap().fd_table[fd as usize].is_some());
    }

    #[test]
    fn test_open_sad_path() {
        let path = CString::new("/nonexistentfile").unwrap();
        let flags = O_RDWR;
        let mode = 0o644;

        // Try to open a non-existent file without O_CREAT
        let fd = unsafe { open(path.as_ptr(), flags, mode) };
        assert_eq!(fd, -1, "Opened a non-existent file without O_CREAT");
    }

    #[test]
    fn test_close_happy_path() {
        let path = CString::new("/testfile").unwrap();
        let flags = O_CREAT | O_RDWR;
        let mode = 0o644;

        // Create and open a file
        let fd = unsafe { open(path.as_ptr(), flags, mode) };
        assert!(fd >= 0, "Failed to create and open file");

        // Close the file
        let result = unsafe { close(fd) };
        assert_eq!(result, 0, "Failed to close the file");

        // Ensure the file descriptor table no longer contains the entry
        let proc = get_or_init_proc();
        assert!(proc.as_ref().unwrap().fd_table[fd as usize].is_none());
    }

    #[test]
    fn test_close_sad_path() {
        let invalid_fd = 999;

        // Try to close an invalid file descriptor
        let result = unsafe { close(invalid_fd) };
        assert_eq!(result, -1, "Closed an invalid file descriptor");
    }

    #[test]
    fn test_creat_happy_path() {
        let path = CString::new("/testfile").unwrap();
        let mode = 0o644;

        // Create a file using creat
        let fd = unsafe { creat(path.as_ptr(), mode) };
        assert!(fd >= 0, "Failed to create file");

        // Ensure the file descriptor table contains the entry
        let proc = get_or_init_proc();
        assert!(proc.as_ref().unwrap().fd_table[fd as usize].is_some());
    }

    #[test]
    fn test_creat_sad_path() {
        let path = CString::new("").unwrap();
        let mode = 0o644;

        // Try to create a file with an empty path
        let fd = unsafe { creat(path.as_ptr(), mode) };
        assert_eq!(fd, -1, "Created a file with an invalid path");
    }

    #[test]
    fn test_openat_happy_path() {
        let dir_path = CString::new("/testdir").unwrap();
        let file_path = CString::new("testfile").unwrap();
        let flags = O_CREAT | O_RDWR;
        let mode = 0o644;

        // Create and open a directory
        let dir_fd = unsafe { open(dir_path.as_ptr(), O_CREAT | O_DIRECTORY, 0o755) };
        assert!(dir_fd >= 0, "Failed to create and open directory");

        // Create and open a file within the directory using openat
        let file_fd = unsafe { openat(dir_fd, file_path.as_ptr(), flags, mode) };
        assert!(file_fd >= 0, "Failed to create and open file in directory");

        // Ensure the file descriptor table contains the entry
        let proc = get_or_init_proc();
        assert!(proc.as_ref().unwrap().fd_table[file_fd as usize].is_some());
    }

    #[test]
    fn test_openat_sad_path() {
        let dir_fd = 999; // Invalid directory file descriptor
        let file_path = CString::new("testfile").unwrap();
        let flags = O_RDWR;
        let mode = 0o644;

        // Try to open a file within an invalid directory
        let file_fd = unsafe { openat(dir_fd, file_path.as_ptr(), flags, mode) };
        assert_eq!(file_fd, -1, "Opened a file in an invalid directory");
    }
}

#[test]
fn test_dup_happy_path() {
    let path = CString::new("/testfile").unwrap();
    let flags = O_CREAT | O_RDWR;
    let mode = 0o644;

    // Open a file
    let fd = unsafe { open(path.as_ptr(), flags, mode) };
    assert!(fd >= 0, "Failed to create and open file");

    // Duplicate the file descriptor
    let new_fd = unsafe { dup(fd) };
    assert!(new_fd >= 0, "Failed to duplicate file descriptor");

    // Ensure both file descriptors point to the same inode
    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();
    assert_eq!(
        proc.fd_table[fd as usize],
        proc.fd_table[new_fd as usize],
        "Duplicated file descriptor does not point to the same inode"
    );
}

#[test]
fn test_dup_sad_path() {
    let invalid_fd = 999; // Invalid file descriptor

    // Try to duplicate an invalid file descriptor
    let new_fd = unsafe { dup(invalid_fd) };
    assert_eq!(new_fd, -1, "Duplicated an invalid file descriptor");
}

#[test]
fn test_dup2_happy_path() {
    let path = CString::new("/testfile").unwrap();
    let flags = O_CREAT | O_RDWR;
    let mode = 0o644;

    // Open a file
    let fd = unsafe { open(path.as_ptr(), flags, mode) };
    assert!(fd >= 0, "Failed to create and open file");

    // Allocate a new file descriptor
    let new_fd = 10;

    // Duplicate fd to a specific new_fd
    let result_fd = unsafe { dup2(fd, new_fd) };
    assert_eq!(result_fd, new_fd, "Failed to duplicate to a specific file descriptor");

    // Ensure both file descriptors point to the same inode
    let proc = get_or_init_proc();
    let proc = proc.as_ref().unwrap();
    assert_eq!(
        proc.fd_table[fd as usize],
        proc.fd_table[new_fd as usize],
        "Duplicated file descriptor does not point to the same inode"
    );
}

#[test]
fn test_dup2_sad_path() {
    let invalid_fd = 999; // Invalid file descriptor
    let new_fd = 10; // Target file descriptor

    // Try to duplicate an invalid file descriptor
    let result_fd = unsafe { dup2(invalid_fd, new_fd) };
    assert_eq!(result_fd, -1, "Duplicated an invalid file descriptor");

    // Try to duplicate to an out-of-range new_fd
    let out_of_range_fd = 1024; // Beyond the limit of fd_table
    let path = CString::new("/testfile").unwrap();
    let flags = O_CREAT | O_RDWR;
    let mode = 0o644;

    let fd = unsafe { open(path.as_ptr(), flags, mode) };
    assert!(fd >= 0, "Failed to create and open file");

    let result_fd = unsafe { dup2(fd, out_of_range_fd) };
    assert_eq!(result_fd, -1, "Duplicated to an out-of-range file descriptor");
}

