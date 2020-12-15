use core::{cell::Cell, sync::atomic};

const SPIN_LIMIT: u32 = 6;
const YIELD_LIMIT: u32 = 10;

pub struct Backoff {
    step: Cell<u32>,
}

impl Backoff {
    #[inline]
    pub fn new() -> Self { Backoff { step: Cell::new(0) } }

    #[inline]
    #[cfg(feature = "std")]
    pub fn reset(&self) { self.step.set(0); }

    #[inline]
    #[cfg(feature = "std")]
    pub fn spin(&self) {
        for _ in 0..1 << self.step.get().min(SPIN_LIMIT) {
            atomic::spin_loop_hint();
        }

        if self.step.get() <= SPIN_LIMIT {
            self.step.set(self.step.get() + 1);
        }
    }

    #[inline]
    pub fn snooze(&self) {
        if self.step.get() <= SPIN_LIMIT {
            for _ in 0..1 << self.step.get() {
                atomic::spin_loop_hint();
            }
        } else {
            #[cfg(not(feature = "std"))]
            for _ in 0..1 << self.step.get() {
                atomic::spin_loop_hint();
            }

            #[cfg(feature = "std")]
            ::std::thread::yield_now();
        }

        if self.step.get() <= YIELD_LIMIT {
            self.step.set(self.step.get() + 1);
        }
    }

    #[inline]
    #[cfg(feature = "std")]
    pub fn is_completed(&self) -> bool { self.step.get() > YIELD_LIMIT }
}

impl Default for Backoff {
    fn default() -> Backoff { Backoff::new() }
}
