use crate::primitives::shared_futex::SharedFutex;
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};

#[repr(C)]
pub struct SharedMutex<T: ?Sized> {
    pub futex: SharedFutex,
    data: UnsafeCell<T>,
}

impl<T: ?Sized> SharedMutex<T> {
    pub fn lock(&self) -> std::io::Result<SharedMutexGuard<'_, T>> {
        self.futex.lock();
        Ok(SharedMutexGuard { lock: self })
    }

    #[allow(dead_code)]
    pub fn try_lock(&self) -> Option<SharedMutexGuard<'_, T>> {
        if self.futex.try_lock() {
            Some(SharedMutexGuard { lock: self })
        } else {
            None
        }
    }
}

unsafe impl<T: ?Sized> Send for SharedMutex<T> where T: Send {}
unsafe impl<T: ?Sized> Sync for SharedMutex<T> where T: Send {}

pub struct SharedMutexGuard<'a, T: ?Sized> {
    lock: &'a SharedMutex<T>,
}

impl<T: ?Sized> Deref for SharedMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for SharedMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for SharedMutexGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { self.lock.futex.unlock() }
    }
}

unsafe impl<T: ?Sized> Send for SharedMutexGuard<'_, T> where T: Send {}
unsafe impl<T: ?Sized> Sync for SharedMutexGuard<'_, T> where T: Send + Sync {}

pub(crate) fn guard_lock<'a, T: ?Sized>(guard: &SharedMutexGuard<'a, T>) -> &'a SharedFutex {
    &guard.lock.futex
}