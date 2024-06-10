use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};

use libc::pthread_mutex_t;

use crate::utils::check_err_ne;

#[repr(C)]
pub struct PThreadLock<T: ?Sized> {
    lock: pthread_mutex_t,
    data: UnsafeCell<T>,
}

pub(crate) unsafe fn pthread_lock_initialize_at(ptr: *mut pthread_mutex_t) -> std::io::Result<()> {
    let mut attr: MaybeUninit<libc::pthread_mutexattr_t> = MaybeUninit::uninit();

    check_err_ne(libc::pthread_mutexattr_init(attr.as_mut_ptr()))?;
    check_err_ne(libc::pthread_mutexattr_setpshared(
        attr.as_mut_ptr(),
        libc::PTHREAD_PROCESS_SHARED,
    ))?;

    check_err_ne(libc::pthread_mutex_init(ptr, attr.as_mut_ptr()))?;

    check_err_ne(libc::pthread_mutexattr_destroy(attr.as_mut_ptr()))?;
    Ok(())
}

impl<T: ?Sized> PThreadLock<T> {
    pub(crate) fn lock(&self) -> std::io::Result<PThreadLockGuard<'_, T>> {
        let ptr = self.get_ptr() as *mut pthread_mutex_t;
        unsafe {
            check_err_ne(libc::pthread_mutex_lock(ptr))?;
        }

        Ok(PThreadLockGuard { lock: self })
    }

    fn release(&self) -> std::io::Result<()> {
        let ptr = self.get_ptr() as *mut pthread_mutex_t;
        unsafe {
            check_err_ne(libc::pthread_mutex_unlock(ptr))?;
        }
        Ok(())
    }

    fn get_ptr(&self) -> *const pthread_mutex_t {
        &self.lock as *const pthread_mutex_t
    }
}

pub(crate) struct PThreadLockGuard<'a, T: ?Sized> {
    lock: &'a PThreadLock<T>,
}

impl<T: ?Sized> Deref for PThreadLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for PThreadLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for PThreadLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release().unwrap()
    }
}

unsafe impl<T: ?Sized> Send for PThreadLockGuard<'_, T> where T: Send {}
unsafe impl<T: ?Sized> Sync for PThreadLockGuard<'_, T> where T: Send {}
