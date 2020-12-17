use crate::TrustedRadium;
use core::{pin::Pin, sync::atomic::Ordering};
use std::boxed::Box;

pub type LocalThin<T> = Thin<T, core::cell::Cell<usize>>;
pub type SyncThin<T> = Thin<T, core::sync::atomic::AtomicUsize>;

pub type LocalThinInner<T> = ThinInner<T, core::cell::Cell<usize>>;
pub type SyncThinInner<T> = ThinInner<T, core::sync::atomic::AtomicUsize>;

pub struct Thin<T: ?Sized, W: TrustedRadium<Item = usize>> {
    ptr: *mut ThinInner<T, W>,
}

unsafe impl<T: ?Sized + Send + Sync, W: Send + Sync + TrustedRadium<Item = usize>> Send for Thin<T, W> {}
unsafe impl<T: ?Sized + Send + Sync, W: Send + Sync + TrustedRadium<Item = usize>> Sync for Thin<T, W> {}

pub struct ThinInner<T: ?Sized, W> {
    strong: W,
    value: T,
}

impl<T, W: TrustedRadium<Item = usize>> ThinInner<T, W> {
    pub fn new(value: T) -> Self {
        Self {
            strong: W::new(1),
            value,
        }
    }
}

impl<T, W: TrustedRadium<Item = usize>> From<ThinInner<T, W>> for Thin<T, W> {
    fn from(inner: ThinInner<T, W>) -> Self { Self::from(Box::new(inner)) }
}

impl<T: ?Sized, W: TrustedRadium<Item = usize>> From<Box<ThinInner<T, W>>> for Thin<T, W> {
    fn from(bx: Box<ThinInner<T, W>>) -> Self { Self { ptr: Box::into_raw(bx) } }
}

impl<T: ?Sized, W: TrustedRadium<Item = usize>> From<Pin<Box<ThinInner<T, W>>>> for Pin<Thin<T, W>> {
    fn from(bx: Pin<Box<ThinInner<T, W>>>) -> Self {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(bx).into()) }
    }
}

impl<T, W: TrustedRadium<Item = usize>> Thin<T, W> {
    pub fn new(value: T) -> Self { Box::new(ThinInner::new(value)).into() }
    pub fn pin(value: T) -> Pin<Self> { Box::pin(ThinInner::new(value)).into() }
}

impl<T: ?Sized, W: TrustedRadium<Item = usize>> Thin<T, W> {
    pub fn strong_count(this: &Self) -> usize { unsafe { (*this.ptr).strong.load(Ordering::Acquire) } }
}

impl<T: ?Sized, W: TrustedRadium<Item = usize>> core::ops::Deref for Thin<T, W> {
    type Target = T;

    fn deref(&self) -> &Self::Target { unsafe { &(*self.ptr).value } }
}

use std::fmt;
impl<T: ?Sized + fmt::Debug, W: TrustedRadium<Item = usize>> fmt::Debug for Thin<T, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { T::fmt(self, f) }
}

impl<T: ?Sized, W: TrustedRadium<Item = usize>> Clone for Thin<T, W> {
    fn clone(&self) -> Self {
        unsafe {
            use crossbeam_utils::Backoff;

            let strong = &(*self.ptr).strong;
            let mut value = strong.load(Ordering::Acquire);

            let backoff = Backoff::new();

            loop {
                if let Some(next_value) = value.checked_add(1) {
                    if let Err(current) =
                        strong.compare_exchange_weak(value, next_value, Ordering::Acquire, Ordering::Acquire)
                    {
                        value = current;
                    } else {
                        break
                    }
                } else {
                    panic!("Tried to clone `Thin<T, W>` too many times")
                }

                backoff.snooze()
            }
        }

        Self { ptr: self.ptr }
    }
}

impl<T: ?Sized, W: TrustedRadium<Item = usize>> Drop for Thin<T, W> {
    fn drop(&mut self) {
        unsafe {
            let count = (*self.ptr).strong.fetch_sub(1, Ordering::Release);

            if count == 1 {
                Box::from_raw(self.ptr);
            }
        }
    }
}
