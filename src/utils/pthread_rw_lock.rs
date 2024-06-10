use libc::pthread_rwlock_t;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};

#[repr(C)]
pub(crate) struct PThreadRwLock<T: ?Sized> {
    lock: pthread_rwlock_t,
    data: UnsafeCell<T>,
}

pub(crate) unsafe fn pthread_rw_lock_initialize_at(ptr: *mut pthread_rwlock_t) -> std::io::Result<()> {
    let mut attr: MaybeUninit<libc::pthread_rwlockattr_t> = MaybeUninit::uninit();

    let attr_ptr = attr.as_mut_ptr();
    crate::utils::check_err_ne(libc::pthread_rwlockattr_init(attr_ptr))?;
    crate::utils::check_err_ne(libc::pthread_rwlockattr_setpshared(
        attr_ptr,
        libc::PTHREAD_PROCESS_SHARED,
    ))?;
    crate::utils::check_err_ne(libc::pthread_rwlockattr_setkind_np(attr_ptr, 2))?; //libc::PTHREAD_RWLOCK_PREFER_WRITER_NONRECURSIVE_NP)

    crate::utils::check_err_ne(libc::pthread_rwlock_init(ptr, attr_ptr))?;

    crate::utils::check_err_ne(libc::pthread_rwlockattr_destroy(attr_ptr))?;
    Ok(())
}

impl<T: ?Sized> PThreadRwLock<T> {
    
    pub(crate) fn read_lock(&self) -> std::io::Result<PThreadReadLockGuard<'_, T>> {
        let ptr = self.get_ptr() as *mut pthread_rwlock_t;

        unsafe {
            crate::utils::check_err_ne(libc::pthread_rwlock_rdlock(ptr))?;
        }

        Ok(PThreadReadLockGuard { lock: self })
    }

    pub(crate) fn write_lock(&self) -> std::io::Result<PThreadWriteLockGuard<'_, T>> {
        let ptr = self.get_ptr() as *mut pthread_rwlock_t;

        unsafe {
            crate::utils::check_err_ne(libc::pthread_rwlock_wrlock(ptr))?;
        }

        Ok(PThreadWriteLockGuard { lock: self })
    }

    fn get_ptr(&self) -> *const pthread_rwlock_t {
        (&self.lock) as *const pthread_rwlock_t
    }
}

unsafe impl<T: ?Sized> Send for PThreadRwLock<T> where T: Send {}
unsafe impl<T: ?Sized> Sync for PThreadRwLock<T> where T: Send {}

pub(crate) struct PThreadReadLockGuard<'a, T: ?Sized> {
    lock: &'a PThreadRwLock<T>,
}

impl<T: ?Sized> Deref for PThreadReadLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for PThreadReadLockGuard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_rwlock_unlock(self.lock.get_ptr() as *mut pthread_rwlock_t);
        }
    }
}

unsafe impl<T: ?Sized> Send for PThreadReadLockGuard<'_, T> where T: Send {}
unsafe impl<T: ?Sized> Sync for PThreadReadLockGuard<'_, T> where T: Send {}

pub(crate) struct PThreadWriteLockGuard<'a, T: ?Sized> {
    lock: &'a PThreadRwLock<T>,
}

impl<T: ?Sized> Deref for PThreadWriteLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T: ?Sized> DerefMut for PThreadWriteLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T: ?Sized> Drop for PThreadWriteLockGuard<'_, T> {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_rwlock_unlock(self.lock.get_ptr() as *mut pthread_rwlock_t);
        }
    }
}

unsafe impl<T: ?Sized> Send for PThreadWriteLockGuard<'_, T> where T: Send {}
unsafe impl<T: ?Sized> Sync for PThreadWriteLockGuard<'_, T> where T: Send {}
