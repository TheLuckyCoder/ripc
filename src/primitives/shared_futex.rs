use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use linux_futex::{Futex, Shared};

const UNLOCKED: u32 = 0;
const LOCKED: u32 = 1; // locked, no other threads waiting
const CONTENDED: u32 = 2; // locked, and other threads waiting (contended)


#[repr(transparent)]
pub struct SharedFutex(Futex<Shared>);

/// This code is largely taken from std::sync::Mutex
impl SharedFutex {
    #[inline]
    pub fn try_lock(&self) -> bool {
        self.0.value.compare_exchange(UNLOCKED, LOCKED, Acquire, Relaxed).is_ok()
    }

    #[inline]
    pub fn lock(&self) {
        if self.0.value.compare_exchange(UNLOCKED, LOCKED, Acquire, Relaxed).is_err() {
            self.lock_contended();
        }
    }

    #[cold]
    fn lock_contended(&self) {
        // Spin first to speed things up if the lock is released quickly.
        let mut state =  self.0.value.load(Relaxed);
        
        // If it's unlocked now, attempt to take the lock
        // without marking it as contended.
        if state == UNLOCKED {
            match self.0.value.compare_exchange(UNLOCKED, LOCKED, Acquire, Relaxed) {
                Ok(_) => return, // Locked!
                Err(s) => state = s,
            }
        }

        loop {
            // Put the lock in contended state.
            // We avoid an unnecessary write if it as already set to CONTENDED,
            // to be friendlier for the caches.
            if state != CONTENDED && self.0.value.swap(CONTENDED, Acquire) == UNLOCKED {
                // We changed it from UNLOCKED to CONTENDED, so we just successfully locked it.
                return;
            }

            // Wait for the futex to change state, assuming it is still CONTENDED.
            let _ = self.0.wait(CONTENDED);

            // Get the new state
            state = self.0.value.load(Relaxed);
        }
    }

    #[inline]
    pub unsafe fn unlock(&self) {
        if self.0.value.swap(UNLOCKED, Release) == CONTENDED {
            // We only wake up one thread. When that thread locks the mutex, it
            // will mark the mutex as CONTENDED (see lock_contended above),
            // which makes sure that any other waiting threads will also be
            // woken up eventually.
            let _ = self.0.wake(1);
        }
    }
}
