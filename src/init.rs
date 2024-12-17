//use std::sync::Once;
//use std::cell::UnsafeCell;
//use crate::{Process, FileSystem};

//static mut FILESYSTEM: Option<UnsafeCell<FileSystem>> = None;
//static mut PROCESS: Option<UnsafeCell<Process>> = None;
//static INIT: Once = Once::new();

//pub fn initialize_globals() {
    //INIT.call_once(|| {
        //unsafe {
            //FILESYSTEM = Some(UnsafeCell::new(FileSystem::new()));
            //PROCESS = Some(UnsafeCell::new(Process::new()));
        //}
    //});
//}
