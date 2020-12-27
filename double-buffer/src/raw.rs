use core::{
    cell::UnsafeCell,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    sync::atomic::Ordering,
};

use crate::*;

pub struct Writer<B: BufferRef> {
    inner: B::Strong,
    tag: WriterTag<B>,
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
    keep_alive: B::Strong,
}

impl<B: BufferRef> Drop for RawGuard<B> {
    fn drop(&mut self) { unsafe { self.keep_alive.strategy.end_guard(ManuallyDrop::take(&mut self.raw)) } }
}

pub struct Buffers<B>(UnsafeCell<[B; 2]>);

pub struct BufferData<W, S, B, E: ?Sized> {
    which: W,
    pub buffers: Buffers<B>,
    pub strategy: S,
    pub extra: E,
}

pub struct Swap<B: BufferRef> {
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
    let reader_tag = unsafe { writer.strategy.reader_tag() };
    let writer_tag = unsafe { writer.strategy.writer_tag() };
    (
        Reader {
            inner: reader,
            tag: reader_tag,
        },
        Writer {
            inner: writer,
            tag: writer_tag,
        },
    )
}

#[cold]
fn snooze(backoff: &crossbeam_utils::Backoff) { backoff.snooze() }

#[derive(Default)]
pub struct BufferDataBuilder<S, B, E> {
    pub buffers: [B; 2],
    pub strategy: S,
    pub extra: E,
}

unsafe impl<B: Send> Send for Buffers<B> {}
unsafe impl<B: Sync> Sync for Buffers<B> {}

impl<B> Buffers<B> {
    fn get_raw(&self, item: bool) -> *mut B { unsafe { self.0.get().cast::<B>().offset(isize::from(item)) } }
    fn read_buffer(&self, item: bool) -> *mut B { self.get_raw(item) }
    fn write_buffer(&self, item: bool) -> *mut B { self.get_raw(!item) }

    pub fn get(&self) -> &[B; 2] { unsafe { &*self.0.get() } }

    pub fn get_mut(&mut self) -> &mut [B; 2] { unsafe { &mut *self.0.get() } }
}

impl<B, S: Strategy, E> BufferDataBuilder<S, B, E> {
    pub fn build<W: TrustedRadium<Item = bool>>(self) -> BufferData<W, S, B, E> {
        BufferData {
            which: W::new(false),
            buffers: Buffers(UnsafeCell::new(self.buffers)),
            strategy: self.strategy,
            extra: self.extra,
        }
    }
}

impl<W, B: Default, S> Default for BufferData<W, S, B, ()>
where
    W: TrustedRadium<Item = bool>,
    B: Default,
    S: Default + Strategy,
{
    #[inline]
    fn default() -> Self { BufferDataBuilder::default().build() }
}

impl<W, B, S> BufferData<W, S, B, ()>
where
    W: TrustedRadium<Item = bool>,
    S: Default + Strategy,
{
    #[inline]
    pub fn new(front: B, back: B) -> Self {
        BufferDataBuilder {
            buffers: [front, back],
            strategy: Default::default(),
            extra: Default::default(),
        }
        .build()
    }
}

impl<B, S, E: ?Sized> BufferData<S::Whitch, S, B, E>
where
    S: Default + Strategy,
{
    pub fn split_mut(&mut self) -> (Reader<&mut Self>, Writer<&mut Self>) { new(self) }
}

struct FinishSwapOnDrop<'a, B: BufferRef> {
    strategy: &'a B::Strategy,
    swap: Swap<B>,
    backoff: crossbeam_utils::Backoff,
}

pub(super) fn is_swap_completed<B: BufferRef>(strategy: &B::Strategy, swap: &mut Swap<B>) -> bool {
    strategy.is_capture_complete(&mut swap.capture)
}

impl<B: BufferRef> Drop for FinishSwapOnDrop<'_, B> {
    #[inline]
    fn drop(&mut self) {
        while !self.strategy.is_swap_completed(&mut self.swap) {
            snooze(&self.backoff)
        }
    }
}

impl<B: BufferRef> Writer<B> {
    #[inline]
    pub fn reader(this: &Self) -> Reader<B> {
        let tag = unsafe { this.inner.strategy.reader_tag() };
        Reader {
            tag,
            inner: B::downgrade(&this.inner),
        }
    }

    #[inline]
    pub fn read(this: &Self) -> &B::Buffer {
        unsafe {
            let inner = &*this.inner;
            let which = inner.which.load_unchecked();
            let read_buffer = inner.buffers.read_buffer(which);
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

    pub fn swap_buffers(this: &mut Self) {
        unsafe {
            let swap = Self::start_buffer_swap(this);
            Self::finish_swap(this, swap)
        }
    }

    pub fn swap_buffers_with<F: FnMut(Split<B>)>(this: &mut Self, mut f: F) {
        let swap = unsafe { Self::start_buffer_swap(this) };
        let split = Self::split(this);
        let f = move || f(split);
        Self::finish_swap_with(this, swap, f)
    }

    pub unsafe fn swap_buffers_unchecked(this: &mut Self) { this.inner.which.fetch_xor(true, Ordering::Release); }

    #[inline]
    pub unsafe fn start_buffer_swap(this: &mut Self) -> Swap<B> {
        let inner = &*this.inner;
        inner.strategy.fence();

        // `fetch_not` == `fetch_xor(true)`
        inner.which.fetch_xor(true, Ordering::Release);

        let capture = inner.strategy.capture_readers(&mut this.tag);

        Swap { capture }
    }

    pub fn finish_swap(this: &Self, swap: Swap<B>) {
        let mut on_drop = FinishSwapOnDrop {
            strategy: &this.inner.strategy,
            swap,
            backoff: crossbeam_utils::Backoff::new(),
        };
        let FinishSwapOnDrop {
            strategy: _,
            swap,
            backoff,
        } = &mut on_drop;

        let strategy = &this.inner.strategy;

        while !strategy.is_swap_completed(swap) {
            snooze(&backoff)
        }

        strategy.fence();

        core::mem::forget(on_drop);
    }

    pub fn finish_swap_with<F: FnMut()>(this: &Self, swap: Swap<B>, ref mut f: F) {
        #[cold]
        #[inline(never)]
        fn cold(f: &mut dyn FnMut()) { f() }

        fn finish_swap_with<B: BufferRef>(strategy: &B::Strategy, swap: Swap<B>, f: &mut dyn FnMut()) {
            let mut on_drop = FinishSwapOnDrop {
                strategy,
                swap,
                backoff: crossbeam_utils::Backoff::new(),
            };
            let swap = &mut on_drop.swap;

            while !strategy.is_swap_completed(swap) {
                cold(f)
            }

            strategy.fence();

            core::mem::forget(on_drop)
        }

        finish_swap_with(&this.inner.strategy, swap, f)
    }
}

impl<B: BufferRef> Reader<B> {
    #[inline]
    pub fn try_clone(&self) -> Result<Self, B::UpgradeError> {
        let inner = B::upgrade(&self.inner)?;
        let tag = unsafe { inner.strategy.reader_tag() };
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
        let keep_alive = B::upgrade(&self.inner)?;
        let inner = &*keep_alive;
        let guard = inner.strategy.begin_guard(&mut self.tag);

        let which = inner.which.load(Ordering::Acquire);
        let buffer = inner.buffers.read_buffer(which);

        Ok(ReaderGuard {
            value: unsafe { &*buffer },
            raw: RawGuard {
                raw: ManuallyDrop::new(guard),
                keep_alive,
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

impl<B: BufferRef<UpgradeError = core::convert::Infallible>> Clone for Reader<B> {
    fn clone(&self) -> Self {
        match self.try_clone() {
            Ok(reader) => reader,
            Err(infallible) => match infallible {},
        }
    }
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
