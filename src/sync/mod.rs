// my_sync/mutex.rs
#![allow(dead_code)]

pub struct Mutex<T> {
    locked: core::sync::atomic::AtomicBool,
    data: core::cell::UnsafeCell<T>,
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: core::sync::atomic::AtomicBool::new(false),
            data: core::cell::UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        // simple spinlock
        while self
            .locked
            .compare_exchange_weak(false, true, core::sync::atomic::Ordering::Acquire, core::sync::atomic::Ordering::Relaxed)
            != Ok(false)
        {
            // spin
        }
        MutexGuard { mutex: self }
    }

    fn unlock(&self) {
        self.locked.store(false, core::sync::atomic::Ordering::Release);
    }

    pub fn get_mut(&self) -> &mut T {
        // only safe if single-thread or no lock is needed
        unsafe { &mut *self.data.get() }
    }
}

impl<'a, T> MutexGuard<'a, T> {
    pub fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }

    pub fn as_ref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T> core::ops::Deref for MutexGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

