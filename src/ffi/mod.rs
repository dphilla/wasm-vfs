// my_ffi/cstr.rs
#![allow(dead_code)]

pub struct CStr {
    bytes: *const i8,
}

impl CStr {
    pub unsafe fn from_ptr<'a>(ptr: *const i8) -> &'a Self {
        &*(ptr as *const CStr)
    }

    pub unsafe fn to_string_lossy(&self) -> String {
        // read until \0
        let mut len = 0;
        while *self.bytes.add(len) != 0 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(self.bytes as *const u8, len);
        String::from_utf8_lossy(slice).into_owned()
    }
}

pub struct CString {
    pub bytes: *mut i8,
}

impl CString {
    pub fn new(s: &str) -> Self {
        let mut v = Vec::with_capacity(s.len() + 1);
        v.extend_from_slice(s.as_bytes());
        v.push(0);
        let ptr = v.as_mut_ptr() as *mut i8;
        core::mem::forget(v); // naive approach
        Self { bytes: ptr }
    }
}

