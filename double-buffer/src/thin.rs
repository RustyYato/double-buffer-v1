use core::sync::atomic::{AtomicUsize, Ordering};
use std::boxed::Box;

pub struct Thin<T> {
    ptr: *mut Package<T>,
}

struct Package<T> {
    strong: AtomicUsize,
    value: T,
}

unsafe impl<T: Send + Sync> Send for Thin<T> {}
unsafe impl<T: Send + Sync> Sync for Thin<T> {}

impl<T> Thin<T> {
    pub fn new(value: T) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(Package {
                strong: AtomicUsize::new(1),
                value,
            })),
        }
    }
}

impl<T> Thin<T> {
    pub fn strong_count(this: &Self) -> usize { unsafe { (*this.ptr).strong.load(Ordering::Acquire) } }
}

impl<T> core::ops::Deref for Thin<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target { unsafe { &(*self.ptr).value } }
}

use std::fmt;
impl<T: fmt::Debug> fmt::Debug for Thin<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { T::fmt(self, f) }
}

impl<T> Clone for Thin<T> {
    fn clone(&self) -> Self {
        unsafe {
            (*self.ptr).strong.fetch_add(1, Ordering::Acquire);
        }

        Self { ptr: self.ptr }
    }
}

impl<T> Drop for Thin<T> {
    fn drop(&mut self) {
        unsafe {
            let count = (*self.ptr).strong.fetch_sub(1, Ordering::Release);

            if count == 1 {
                Box::from_raw(self.ptr);
            }
        }
    }
}
