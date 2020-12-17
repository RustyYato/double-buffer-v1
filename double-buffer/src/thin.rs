use crate::TrustedRadium;
use core::{pin::Pin, sync::atomic::Ordering};
use std::boxed::Box;

pub type Rc<T> = Thin<T, core::cell::Cell<usize>>;
pub type Arc<T> = Thin<T, core::sync::atomic::AtomicUsize>;

pub type RcInner<T> = ThinInner<T, core::cell::Cell<usize>>;
pub type ArcInner<T> = ThinInner<T, core::sync::atomic::AtomicUsize>;

pub struct Thin<T: ?Sized, S: TrustedRadium<Item = usize>> {
    ptr: *mut ThinInner<T, S>,
}

unsafe impl<T: ?Sized + Send + Sync, S: Send + Sync + TrustedRadium<Item = usize>> Send for Thin<T, S> {}
unsafe impl<T: ?Sized + Send + Sync, S: Send + Sync + TrustedRadium<Item = usize>> Sync for Thin<T, S> {}

pub struct ThinInner<T: ?Sized, S> {
    strong: S,
    value: T,
}

impl<T, S: TrustedRadium<Item = usize>> ThinInner<T, S> {
    pub fn new(value: T) -> Self {
        Self {
            strong: S::new(1),
            value,
        }
    }
}

impl<T, S: TrustedRadium<Item = usize>> From<ThinInner<T, S>> for Thin<T, S> {
    fn from(inner: ThinInner<T, S>) -> Self { Self::from(Box::new(inner)) }
}

impl<T: ?Sized, S: TrustedRadium<Item = usize>> From<Box<ThinInner<T, S>>> for Thin<T, S> {
    fn from(bx: Box<ThinInner<T, S>>) -> Self { Self { ptr: Box::into_raw(bx) } }
}

impl<T: ?Sized, S: TrustedRadium<Item = usize>> From<Pin<Box<ThinInner<T, S>>>> for Pin<Thin<T, S>> {
    fn from(bx: Pin<Box<ThinInner<T, S>>>) -> Self {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(bx).into()) }
    }
}

impl<T, S: TrustedRadium<Item = usize>> Thin<T, S> {
    pub fn new(value: T) -> Self { Box::new(ThinInner::new(value)).into() }
    pub fn pin(value: T) -> Pin<Self> { Box::pin(ThinInner::new(value)).into() }
}

impl<T: ?Sized, S: TrustedRadium<Item = usize>> Thin<T, S> {
    pub fn strong_count(this: &Self) -> usize { unsafe { (*this.ptr).strong.load(Ordering::Acquire) } }

    fn strong(&self) -> &S { unsafe { &(*self.ptr).strong } }
}

impl<T: ?Sized, S: TrustedRadium<Item = usize>> core::ops::Deref for Thin<T, S> {
    type Target = T;

    fn deref(&self) -> &Self::Target { unsafe { &(*self.ptr).value } }
}

use std::fmt;
impl<T: ?Sized + fmt::Debug, S: TrustedRadium<Item = usize>> fmt::Debug for Thin<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { T::fmt(self, f) }
}

impl<T: ?Sized, S: TrustedRadium<Item = usize>> Clone for Thin<T, S> {
    fn clone(&self) -> Self {
        #[cold]
        #[inline(never)]
        fn clone_fail() -> ! {
            struct Abort;

            impl Drop for Abort {
                fn drop(&mut self) { panic!() }
            }

            // double panic = abort
            let _abort = Abort;

            panic!("Tried to increment ref-count too many times!")
        }

        let old_size = self.strong().fetch_add(1, Ordering::Relaxed);

        if old_size > isize::MAX as usize {
            clone_fail()
        }

        Self { ptr: self.ptr }
    }
}

impl<T: ?Sized, S: TrustedRadium<Item = usize>> Drop for Thin<T, S> {
    fn drop(&mut self) {
        unsafe {
            if 1 == self.strong().fetch_sub(1, Ordering::Release) {
                if !S::IS_LOCAL {
                    core::sync::atomic::fence(Ordering::Acquire);
                }
                drop_slow(Box::from_raw(self.ptr))
            }
        }
    }
}

#[cold]
#[inline(never)]
fn drop_slow<T: ?Sized, S: TrustedRadium<Item = usize>>(inner: Box<ThinInner<T, S>>) { drop(inner); }
