//! The mod's one lock poison policy: a lock a panicking thread held is taken
//! again with its state kept, its poison cleared and the place logged.

use std::panic::Location;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError, WaitTimeoutResult};
use std::time::Duration;

/// Takes a lock under the poison policy.
pub(crate) trait Relock<T: ?Sized> {
    /// The guard, also after a thread panicked holding the lock.
    fn relock(&self) -> MutexGuard<'_, T>;
}

/// Waits on a condition variable under the poison policy.
pub(crate) trait RelockWait {
    /// [`Condvar::wait_timeout_while`], with the guard also after a thread
    /// panicked holding the lock; the lock's poison is cleared by its next
    /// [`Relock::relock`].
    fn relock_wait_timeout_while<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
        condition: impl FnMut(&mut T) -> bool,
    ) -> (MutexGuard<'a, T>, WaitTimeoutResult);
}

impl<T: ?Sized> Relock<T> for Mutex<T> {
    #[track_caller]
    #[inline]
    fn relock(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            poisoned_at(Location::caller());
            self.clear_poison();
            poisoned.into_inner()
        })
    }
}

impl RelockWait for Condvar {
    #[track_caller]
    #[inline]
    fn relock_wait_timeout_while<'a, T>(
        &self,
        guard: MutexGuard<'a, T>,
        timeout: Duration,
        condition: impl FnMut(&mut T) -> bool,
    ) -> (MutexGuard<'a, T>, WaitTimeoutResult) {
        self.wait_timeout_while(guard, timeout, condition)
            .unwrap_or_else(|poisoned| {
                poisoned_at(Location::caller());
                PoisonError::into_inner(poisoned)
            })
    }
}

/// Logs that the lock taken at `at` was poisoned.
#[cold]
fn poisoned_at(at: &Location<'_>) {
    enshwick::warn!(
        "{}:{}: a thread panicked holding this lock; its state is kept",
        at.file(),
        at.line()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_poisoned_lock_keeps_its_state_and_heals() {
        let lock = Mutex::new(vec![1]);
        let _panicked = std::panic::catch_unwind(|| {
            let mut held = lock.lock().unwrap();
            held.push(2);
            panic!("while holding");
        });
        assert!(lock.is_poisoned());
        assert_eq!(*lock.relock(), [1, 2]);
        assert!(!lock.is_poisoned());
    }

    #[test]
    fn a_wait_on_a_poisoned_lock_keeps_its_state_until_the_next_relock_heals_it() {
        let lock = Mutex::new(false);
        let wake = Condvar::new();
        let _panicked = std::panic::catch_unwind(|| {
            let mut held = lock.lock().unwrap();
            *held = true;
            panic!("while holding");
        });
        let guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (stop, waited) =
            wake.relock_wait_timeout_while(guard, Duration::from_secs(5), |stop| !*stop);
        assert!(*stop);
        assert!(!waited.timed_out());
        drop(stop);
        assert!(lock.is_poisoned());
        assert!(*lock.relock());
        assert!(!lock.is_poisoned());
    }
}
