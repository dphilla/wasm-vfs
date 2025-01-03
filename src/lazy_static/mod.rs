// my_lazy_static/mod.rs
#![allow(dead_code)]

use crate::sync::{Mutex, MutexGuard};

pub struct LazyStatic<T> {
    mutex: Mutex<Option<T>>,
}

impl<T> LazyStatic<T> {
    pub const fn new() -> Self {
        Self {
            mutex: Mutex::new(None),
        }
    }

    pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> MutexGuard<Option<T>> {
        let mut guard = self.mutex.lock();
        if guard.is_none() {
            *guard = Some(f());
        }
        guard
    }
}

