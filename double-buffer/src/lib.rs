#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(feature = "alloc", not(feature = "std")))]
extern crate alloc as std;

#[cfg(feature = "alloc")]
pub mod op;

#[cfg(feature = "alloc")]
pub mod thin;

pub mod atomic;
pub mod local;
#[cfg(feature = "alloc")]
pub mod sync;

#[cfg(test)]
mod tests;

mod buffer_ref;

use core::{
    cell::UnsafeCell,
    marker::PhantomPinned,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::atomic::Ordering,
};
use radium::Radium;

pub unsafe trait TrustedRadium: Radium {
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item;
}

unsafe impl TrustedRadium for core::cell::Cell<bool> {
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item { self.get() }
}

unsafe impl TrustedRadium for core::sync::atomic::AtomicBool {
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item {
        core::ptr::read(self as *const core::sync::atomic::AtomicBool as *const bool)
    }
}

unsafe impl TrustedRadium for core::cell::Cell<usize> {
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item { self.get() }
}

unsafe impl TrustedRadium for core::sync::atomic::AtomicUsize {
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item {
        core::ptr::read(self as *const core::sync::atomic::AtomicUsize as *const usize)
    }
}

pub type BufferRefData<BR> = BufferData<
    <BR as BufferRef>::RawPtr,
    <BR as BufferRef>::Buffer,
    <BR as BufferRef>::Strategy,
    <BR as BufferRef>::Extra,
>;

type ReaderTag<BR> = <<BR as BufferRef>::Strategy as Strategy>::ReaderTag;
type Capture<BR> = <<BR as BufferRef>::Strategy as Strategy>::Capture;

pub unsafe trait BufferRef: Sized {
    type RawPtr: TrustedRadium<Item = bool>;
    type Buffer;
    type Strategy: Strategy;
    type Extra: ?Sized;
    type UpgradeError: core::fmt::Debug;

    type Strong: Clone + Deref<Target = BufferRefData<Self>>;
    type Weak: Clone;

    fn split(self) -> (Pin<Self::Strong>, Self::Weak);

    fn is_dangling(weak: &Self::Weak) -> bool;

    fn upgrade(weak: &Self::Weak) -> Result<Pin<Self::Strong>, Self::UpgradeError>;

    fn downgrade(strong: &Pin<Self::Strong>) -> Self::Weak;
}

pub unsafe trait Strategy: Sized {
    type ReaderTag;
    type Capture;
    type RawGuard;

    fn create_tag(&self) -> Self::ReaderTag;

    fn fence(&self);

    fn capture_readers(&self) -> Self::Capture;

    fn is_capture_complete(&self, capture: &mut Self::Capture) -> bool;

    fn begin_guard(&self, tag: &Self::ReaderTag) -> Self::RawGuard;

    fn end_guard(&self, guard: Self::RawGuard);
}

// pub unsafe trait Capture: Strategy {}

pub struct Writer<B: BufferRef> {
    inner: Pin<B::Strong>,
}

pub struct Reader<B: BufferRef> {
    inner: B::Weak,
    tag: ReaderTag<B>,
}

pub struct ReaderGuard<'reader, B: BufferRef, T: ?Sized = <B as BufferRef>::Buffer> {
    value: &'reader T,
    raw: RawGuard<B>,
}

pub struct RawGuard<B: BufferRef> {
    raw: ManuallyDrop<<B::Strategy as Strategy>::RawGuard>,
    keep_alive: Pin<B::Strong>,
}

impl<B: BufferRef> Drop for RawGuard<B> {
    fn drop(&mut self) { unsafe { self.keep_alive.strategy.end_guard(ManuallyDrop::take(&mut self.raw)) } }
}

struct Buffers<B>(UnsafeCell<[B; 2]>);

unsafe impl<B: Send> Send for Buffers<B> {}
unsafe impl<B: Sync> Sync for Buffers<B> {}

impl<B> Buffers<B> {
    fn get_raw(&self, item: bool) -> *mut B { unsafe { self.0.get().cast::<B>().offset(isize::from(item)) } }
    fn read_buffer(&self, item: bool) -> *mut B { self.get_raw(item) }
    fn write_buffer(&self, item: bool) -> *mut B { self.get_raw(!item) }
}

pub struct BufferData<R, B, S, E: ?Sized> {
    _pin: PhantomPinned,
    which: R,
    buffers: Buffers<B>,
    strategy: S,
    extra: E,
}

pub struct Swap<'a, B: BufferRef> {
    strategy: &'a B::Strategy,
    capture: Capture<B>,
}

#[non_exhaustive]
pub struct SplitMut<'a, B: BufferRef> {
    pub read: &'a B::Buffer,
    pub write: &'a mut B::Buffer,
    pub extra: &'a B::Extra,
}

#[non_exhaustive]
pub struct Split<'a, B: BufferRef> {
    pub read: &'a B::Buffer,
    pub write: &'a B::Buffer,
    pub extra: &'a B::Extra,
}

impl<'a, B: BufferRef> Copy for Split<'a, B> {}
impl<'a, B: BufferRef> Clone for Split<'a, B> {
    fn clone(&self) -> Self { *self }
}

pub fn new<B: BufferRef>(buffer_ref: B) -> (Reader<B>, Writer<B>) {
    let (writer, reader) = buffer_ref.split();
    writer.which.store(false, Ordering::Release);
    let tag = writer.strategy.create_tag();
    (Reader { inner: reader, tag }, Writer { inner: writer })
}

impl<R, B: Default, S, E: Default> Default for BufferData<R, B, S, E>
where
    R: TrustedRadium<Item = bool>,
    B: Default,
    S: Default + Strategy,
    E: Default,
{
    #[inline]
    fn default() -> Self { Self::with_extra(Default::default(), Default::default(), Default::default()) }
}

impl<R, B, S> BufferData<R, B, S, ()>
where
    R: TrustedRadium<Item = bool>,
    S: Default + Strategy,
{
    #[inline]
    pub fn new(front: B, back: B) -> Self { Self::with_extra(front, back, ()) }
}

impl<R, B, S, E> BufferData<R, B, S, E>
where
    R: TrustedRadium<Item = bool>,
    S: Default + Strategy,
{
    #[inline]
    pub fn with_extra(front: B, back: B, extra: E) -> Self {
        Self {
            _pin: PhantomPinned,
            buffers: Buffers(UnsafeCell::new([front, back])),
            which: R::new(false),
            strategy: S::default(),
            extra,
        }
    }
}

impl<R, B, S, E: ?Sized> BufferData<R, B, S, E>
where
    R: TrustedRadium<Item = bool>,
    S: Default + Strategy,
{
    pub fn extra(&self) -> &E { &self.extra }

    pub fn strategy(&self) -> &S { &self.strategy }

    pub fn split_mut(self: Pin<&mut Self>) -> (Reader<Pin<&mut Self>>, Writer<Pin<&mut Self>>) { new(self) }
}

struct FinishSwapOnDrop<'a, B: BufferRef> {
    swap: Swap<'a, B>,
    backoff: crossbeam_utils::Backoff,
}

impl<B: BufferRef> Drop for FinishSwapOnDrop<'_, B> {
    #[inline]
    fn drop(&mut self) {
        while !self.swap.is_swap_completed() {
            self.backoff.snooze()
        }
    }
}

impl<B: BufferRef> Swap<'_, B> {
    #[inline]
    pub fn is_swap_completed(&mut self) -> bool {
        if self.strategy.is_capture_complete(&mut self.capture) {
            self.strategy.fence();
            true
        } else {
            false
        }
    }

    pub fn finish_swap(self) {
        let mut on_drop = FinishSwapOnDrop {
            swap: self,
            backoff: crossbeam_utils::Backoff::new(),
        };
        let FinishSwapOnDrop { swap, backoff } = &mut on_drop;

        while !swap.is_swap_completed() {
            backoff.snooze()
        }

        core::mem::forget(on_drop);
    }

    pub fn finish_swap_with<F: FnMut()>(self, ref mut f: F) {
        #[cold]
        #[inline(never)]
        fn cold(f: &mut dyn FnMut()) { f() }

        fn finish_swap_with<B: BufferRef>(swap: Swap<B>, f: &mut dyn FnMut()) {
            let mut on_drop = FinishSwapOnDrop {
                swap,
                backoff: crossbeam_utils::Backoff::new(),
            };
            let swap = &mut on_drop.swap;

            while !swap.is_swap_completed() {
                cold(f)
            }

            core::mem::forget(on_drop)
        }

        finish_swap_with(self, f)
    }
}

impl<B: BufferRef> Writer<B> {
    #[inline]
    pub fn reader(this: &Self) -> Reader<B> {
        let tag = this.inner.strategy.create_tag();
        Reader {
            tag,
            inner: B::downgrade(&this.inner),
        }
    }

    #[inline]
    pub fn read(this: &Self) -> &B::Buffer {
        unsafe {
            let which = this.inner.which.load_unchecked();
            let read_buffer = this.inner.buffers.read_buffer(which);
            &*read_buffer
        }
    }

    #[inline]
    pub fn strategy(this: &Self) -> &B::Strategy { &this.inner.strategy }

    #[inline]
    pub fn extra(this: &Self) -> &B::Extra { &this.inner.extra }

    #[inline]
    pub fn split(this: &Self) -> Split<'_, B> {
        unsafe {
            let inner = &*this.inner;
            let which = inner.which.load_unchecked();
            let reader = inner.buffers.read_buffer(which);
            let writer = inner.buffers.write_buffer(which);

            Split {
                read: &*reader,
                write: &*writer,
                extra: &inner.extra,
            }
        }
    }

    #[inline]
    pub fn split_mut(this: &mut Self) -> SplitMut<'_, B> {
        unsafe {
            let inner = &*this.inner;
            let which = inner.which.load_unchecked();
            let reader = inner.buffers.read_buffer(which);
            let writer = inner.buffers.write_buffer(which);

            SplitMut {
                read: &*reader,
                write: &mut *writer,
                extra: &inner.extra,
            }
        }
    }

    pub fn swap_buffers(this: &mut Self) { unsafe { Self::start_buffer_swap(this).finish_swap() } }

    pub fn swap_buffers_with<F: FnMut(Split<'_, B>)>(this: &mut Self, f: F) {
        fn bake<'a, B: 'a + BufferRef, F: 'a + FnMut(Split<'a, B>)>(
            split: Split<'a, B>,
            mut f: F,
        ) -> impl '_ + FnMut() {
            move || f(split)
        }

        let (swap, split) = unsafe { Self::split_start_buffer_swap(this) };

        swap.finish_swap_with(bake(split, f))
    }

    #[inline]
    pub unsafe fn start_buffer_swap(this: &mut Self) -> Swap<'_, B> { Self::split_start_buffer_swap(this).0 }

    #[inline]
    pub unsafe fn split_start_buffer_swap(this: &mut Self) -> (Swap<'_, B>, Split<'_, B>) {
        let inner = &*this.inner;
        inner.strategy.fence();

        // `fetch_not` == `fetch_xor(true)`
        let which = inner.which.fetch_xor(true, Ordering::Release);
        let which = !which;

        let capture = inner.strategy.capture_readers();
        let read = inner.buffers.read_buffer(which);
        let write = inner.buffers.write_buffer(which);
        let extra = &inner.extra;

        (
            Swap {
                strategy: &inner.strategy,
                capture,
            },
            Split {
                read: &*read,
                write: &*write,
                extra,
            },
        )
    }

    #[inline]
    pub fn get_pinned_write_buffer(this: Pin<&mut Self>) -> Pin<&mut B::Buffer> {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(this) as &mut B::Buffer) }
    }
}

impl<B: BufferRef> Reader<B> {
    #[inline]
    pub fn try_clone(&self) -> Result<Self, B::UpgradeError> {
        let inner = B::upgrade(&self.inner)?;
        let tag = inner.strategy.create_tag();
        Ok(Reader {
            inner: self.inner.clone(),
            tag,
        })
    }

    #[inline]
    pub fn is_dangling(&self) -> bool { B::is_dangling(&self.inner) }

    #[inline]
    pub fn get(&mut self) -> ReaderGuard<'_, B> { self.try_get().expect("Tried to reader from a dangling `Reader<B>`") }

    #[inline]
    pub fn try_get(&mut self) -> Result<ReaderGuard<'_, B>, B::UpgradeError> {
        let inner = B::upgrade(&self.inner)?;
        let guard = inner.strategy.begin_guard(&self.tag);

        let which = inner.which.load(Ordering::Acquire);
        let buffer = inner.buffers.read_buffer(which);

        Ok(ReaderGuard {
            value: unsafe { &*buffer },
            raw: RawGuard {
                raw: ManuallyDrop::new(guard),
                keep_alive: inner,
            },
        })
    }
}

impl<'a, B: BufferRef, T: ?Sized> ReaderGuard<'a, B, T> {
    #[inline]
    pub fn raw_guard(this: &Self) -> &RawGuard<B> { &this.raw }

    pub fn map<F, U: ?Sized>(this: Self, f: F) -> ReaderGuard<'a, B, U>
    where
        F: for<'val> FnOnce(&'val T, &RawGuard<B>) -> &'val U,
    {
        ReaderGuard {
            value: f(this.value, Self::raw_guard(&this)),
            raw: this.raw,
        }
    }

    pub fn try_map<F, U: ?Sized>(this: Self, f: F) -> Result<ReaderGuard<'a, B, U>, Self>
    where
        F: for<'val> FnOnce(&'val T, &RawGuard<B>) -> Option<&'val U>,
    {
        match f(this.value, Self::raw_guard(&this)) {
            None => Err(this),
            Some(value) => Ok(ReaderGuard { value, raw: this.raw }),
        }
    }

    pub fn try_map_res<F, U: ?Sized, E>(this: Self, f: F) -> Result<ReaderGuard<'a, B, U>, (Self, E)>
    where
        F: for<'val> FnOnce(&'val T, &RawGuard<B>) -> Result<&'val U, E>,
    {
        match f(this.value, Self::raw_guard(&this)) {
            Err(e) => Err((this, e)),
            Ok(value) => Ok(ReaderGuard { value, raw: this.raw }),
        }
    }
}

impl<'a, B: BufferRef> RawGuard<B> {
    #[inline]
    pub fn strategy(&self) -> &B::Strategy { &self.keep_alive.strategy }

    #[inline]
    pub fn extra(&self) -> &B::Extra { &self.keep_alive.extra }
}

impl<B: BufferRef> Deref for Writer<B> {
    type Target = B::Buffer;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe {
            let inner = &*self.inner;
            let which = inner.which.load_unchecked();
            let write = inner.buffers.write_buffer(which);
            &*write
        }
    }
}

impl<B: BufferRef> DerefMut for Writer<B> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            let inner = &*self.inner;
            let which = inner.which.load_unchecked();
            let write = inner.buffers.write_buffer(which);
            &mut *write
        }
    }
}

impl<T: ?Sized, B: BufferRef> core::ops::Deref for ReaderGuard<'_, B, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target { self.value }
}
