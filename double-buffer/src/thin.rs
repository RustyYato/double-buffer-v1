use crate::TrustedRadium;
use core::{pin::Pin, sync::atomic::Ordering};
use std::boxed::Box;

pub type LocalThin<T> = Thin<T, core::cell::Cell<usize>>;
pub type SyncThin<T> = Thin<T, core::sync::atomic::AtomicUsize>;

pub type LocalThinInner<T> = ThinInner<T, core::cell::Cell<usize>>;
pub type SyncThinInner<T> = ThinInner<T, core::sync::atomic::AtomicUsize>;

pub struct Thin<T: ?Sized, R: TrustedRadium<Item = usize>> {
    ptr: *mut ThinInner<T, R>,
}

unsafe impl<T: ?Sized + Send + Sync, R: Send + Sync + TrustedRadium<Item = usize>> Send for Thin<T, R> {}
unsafe impl<T: ?Sized + Send + Sync, R: Send + Sync + TrustedRadium<Item = usize>> Sync for Thin<T, R> {}

pub struct ThinInner<T: ?Sized, R> {
    strong: R,
    value: T,
}

impl<T, R: TrustedRadium<Item = usize>> ThinInner<T, R> {
    pub fn new(value: T) -> Self {
        Self {
            strong: R::new(1),
            value,
        }
    }
}

impl<T, R: TrustedRadium<Item = usize>> From<ThinInner<T, R>> for Thin<T, R> {
    fn from(inner: ThinInner<T, R>) -> Self { Self::from(Box::new(inner)) }
}

impl<T: ?Sized, R: TrustedRadium<Item = usize>> From<Box<ThinInner<T, R>>> for Thin<T, R> {
    fn from(bx: Box<ThinInner<T, R>>) -> Self { Self { ptr: Box::into_raw(bx) } }
}

impl<T: ?Sized, R: TrustedRadium<Item = usize>> From<Pin<Box<ThinInner<T, R>>>> for Pin<Thin<T, R>> {
    fn from(bx: Pin<Box<ThinInner<T, R>>>) -> Self {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(bx).into()) }
    }
}

impl<T, R: TrustedRadium<Item = usize>> Thin<T, R> {
    pub fn new(value: T) -> Self { Box::new(ThinInner::new(value)).into() }
    pub fn pin(value: T) -> Pin<Self> { Box::pin(ThinInner::new(value)).into() }
}

impl<T: ?Sized, R: TrustedRadium<Item = usize>> Thin<T, R> {
    pub fn strong_count(this: &Self) -> usize { unsafe { (*this.ptr).strong.load(Ordering::Acquire) } }
}

impl<T: ?Sized, R: TrustedRadium<Item = usize>> core::ops::Deref for Thin<T, R> {
    type Target = T;

    fn deref(&self) -> &Self::Target { unsafe { &(*self.ptr).value } }
}

use std::fmt;
impl<T: ?Sized + fmt::Debug, R: TrustedRadium<Item = usize>> fmt::Debug for Thin<T, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { T::fmt(self, f) }
}

impl<T: ?Sized, R: TrustedRadium<Item = usize>> Clone for Thin<T, R> {
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
                    panic!("Tried to clone `Thin<T, R>` too many times")
                }

                backoff.snooze()
            }
        }

        Self { ptr: self.ptr }
    }
}

impl<T: ?Sized, R: TrustedRadium<Item = usize>> Drop for Thin<T, R> {
    fn drop(&mut self) {
        unsafe {
            let count = (*self.ptr).strong.fetch_sub(1, Ordering::Release);

            if count == 1 {
                Box::from_raw(self.ptr);
            }
        }
    }
}
