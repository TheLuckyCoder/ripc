use crate::utils::check_err_ne;
use libc::{pthread_mutex_t, pthread_rwlock_t};
use std::mem::MaybeUninit;

#[repr(transparent)]
pub(crate) struct PThreadLock {
    lock: pthread_mutex_t,
}

impl PThreadLock {
    pub(crate) unsafe fn initialize_at(ptr: *mut PThreadLock) -> std::io::Result<()> {
        let mut attr: MaybeUninit<libc::pthread_mutexattr_t> = MaybeUninit::uninit();

        check_err_ne(libc::pthread_mutexattr_init(attr.as_mut_ptr()))?;
        check_err_ne(libc::pthread_mutexattr_setpshared(
            attr.as_mut_ptr(),
            libc::PTHREAD_PROCESS_SHARED,
        ))?;

        check_err_ne(libc::pthread_mutex_init(ptr.cast(), attr.as_mut_ptr()))?;

        check_err_ne(libc::pthread_mutexattr_destroy(attr.as_mut_ptr()))?;
        Ok(())
    }

    pub(crate) unsafe fn acquire_lock(&mut self) -> Result<PThreadLockGuard, String> {
        let ptr = &mut self.lock as *mut pthread_mutex_t;
        let result = libc::pthread_mutex_lock(ptr);

        if result != 0 {
            Err(format!("Failed to create lock: {}", result))
        } else {
            Ok(PThreadLockGuard::create(ptr))
        }
    }
}

pub(crate) struct PThreadLockGuard {
    lock: *mut pthread_mutex_t,
}

impl PThreadLockGuard {
    fn create(lock: *mut pthread_mutex_t) -> Self {
        Self { lock }
    }
}

impl Drop for PThreadLockGuard {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_mutex_unlock(self.lock);
        }
    }
}

#[repr(transparent)]
pub(crate) struct PThreadRwLock {
    lock: pthread_rwlock_t,
}

impl PThreadRwLock {
    pub(crate) unsafe fn initialize_at(ptr: *mut PThreadRwLock) -> std::io::Result<()> {
        let mut attr: MaybeUninit<libc::pthread_rwlockattr_t> = MaybeUninit::uninit();

        let attr_ptr = attr.as_mut_ptr();
        check_err_ne(libc::pthread_rwlockattr_init(attr_ptr))?;
        check_err_ne(libc::pthread_rwlockattr_setpshared(
            attr_ptr,
            libc::PTHREAD_PROCESS_SHARED,
        ))?;
        check_err_ne(libc::pthread_rwlockattr_setkind_np(attr_ptr, 2))?; //libc::PTHREAD_RWLOCK_PREFER_WRITER_NONRECURSIVE_NP)

        check_err_ne(libc::pthread_rwlock_init(ptr.cast(), attr_ptr))?;

        check_err_ne(libc::pthread_rwlockattr_destroy(attr_ptr))?;
        Ok(())
    }

    pub(crate) unsafe fn read_lock(&self) -> Result<PThreadRwLockGuard, String> {
        let ptr = (&self.lock as *const pthread_rwlock_t) as *mut pthread_rwlock_t;
        let result = libc::pthread_rwlock_rdlock(ptr);

        if result != 0 {
            Err(format!("Failed to create write lock: {}", result))
        } else {
            Ok(PThreadRwLockGuard::create(ptr))
        }
    }

    pub(crate) unsafe fn write_lock(&mut self) -> Result<PThreadRwLockGuard, String> {
        let ptr = &mut self.lock as *mut pthread_rwlock_t;
        let result = libc::pthread_rwlock_wrlock(ptr);

        if result != 0 {
            Err(format!("Failed to create write lock: {}", result))
        } else {
            Ok(PThreadRwLockGuard::create(ptr))
        }
    }
}

pub(crate) struct PThreadRwLockGuard {
    lock: *mut pthread_rwlock_t,
}

impl PThreadRwLockGuard {
    fn create(lock: *mut pthread_rwlock_t) -> Self {
        Self { lock }
    }
}

impl Drop for PThreadRwLockGuard {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_rwlock_unlock(self.lock);
        }
    }
}
