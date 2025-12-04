use core::{
    cell::UnsafeCell,
    hint,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    arch::{IrqState, irq_disable},
    mp::{MP_STATE, MpState},
};

pub struct IntMutexGuard<'a, T> {
    mutex: &'a IntMutex<T>,
    irq_state: IrqState,
}

impl<'a, T> Drop for IntMutexGuard<'a, T> {
    fn drop(&mut self) {
        // TODO: wake things up from the queue
        self.mutex.lock.store(false, Ordering::Release);
        self.irq_state.restore();
    }
}

impl<'a, T> Deref for IntMutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<'a, T> DerefMut for IntMutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.mutex.data.get() }
    }
}

/// interrupt-disabled "smart" mutex
/// will spin for a fixed number of cycles before sleeping the current thread, but only if the
/// current context can be preempted.
pub struct IntMutex<T> {
    // underlying mutex
    lock: AtomicBool,
    data: UnsafeCell<T>,
    // TODO: we need a blocked queue here
}


impl<T> IntMutex<T> {
    pub const fn new(init: T) -> IntMutex<T> {
        IntMutex {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(init),
        }
    }

    #[inline(always)]
    pub fn lock(&self) -> IntMutexGuard<'_, T> {
        // TODO: for performance, the lock should be implemented as a optimistic xchg lock (then
        // check preemption state, then block/poll)

        if MP_STATE.load(Ordering::Relaxed) == MpState::MPPreempt {
            // TODO: this is a pre-emptable state, we need to be able to pre-empt.
            todo!()
        }

        let state = IrqState::save();

        loop {
            irq_disable();

            if !self.lock.swap(true, Ordering::Acquire) {
                break;
            }

            state.restore();

            while self.lock.load(Ordering::Relaxed) {
                hint::spin_loop();
            }
        }

        IntMutexGuard {
            mutex: self,
            irq_state: state,
        }
    }
}

unsafe impl<T: Send> Send for IntMutex<T> {}
unsafe impl<T: Send> Sync for IntMutex<T> {}
