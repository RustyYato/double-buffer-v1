use core::pin::Pin;
use std::{
    cell::UnsafeCell,
    future::Future,
    marker::Unpin,
    ops::{Deref, DerefMut},
    ptr,
    sync::{
        atomic::{self, AtomicPtr, AtomicU32, Ordering},
        Arc, Weak,
    },
};

use crate::thin::Thin;
use slab::Slab;
use smallvec::SmallVec;
use spin::Mutex;

type TagList = Mutex<Slab<Thin<AtomicU32>>>;

struct SyncPtr<B>(*mut B);

unsafe impl<B> Send for SyncPtr<B> {}
unsafe impl<B> Sync for SyncPtr<B> {}

pub struct Writer<B, E: ?Sized = ()> {
    ptr: SyncPtr<B>,
    buffers: Arc<Buffers<B, E>>,
}

pub struct Reader<B, E: ?Sized = ()> {
    buffers: Weak<Buffers<B, E>>,
    epoch: Thin<AtomicU32>,
}

pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::RawReaderGuard<'reader, T, TagGuard<B, E>>;

pub struct TagGuard<B, E: ?Sized> {
    epoch: Thin<AtomicU32>,
    buffers: Arc<Buffers<B, E>>,
}

pub struct Buffers<B, E: ?Sized = ()> {
    ptr: AtomicPtr<B>,
    tag_list: TagList,
    raw: UnsafeCell<[B; 2]>,
    extra: E,
}

pub struct RawSwap(SmallVec<[(Thin<AtomicU32>, u32); 8]>);

pub struct Swap<B, E: ?Sized> {
    writer: Writer<B, E>,
    packet: RawSwap,
}

unsafe impl<B: Send, E: Send> Send for Buffers<B, E> {}
unsafe impl<B: Sync, E: Sync> Sync for Buffers<B, E> {}

impl<B: Unpin> Unpin for Writer<B> {}

impl<B: Default, E: Default> Default for Buffers<B, E> {
    #[inline]
    fn default() -> Self { Buffers::new(Default::default(), Default::default()).extra(Default::default()) }
}

impl<B> Buffers<B> {
    #[inline]
    pub fn new(front: B, back: B) -> Self {
        Self {
            raw: UnsafeCell::new([front, back]),
            tag_list: TagList::default(),
            ptr: AtomicPtr::new(ptr::null_mut()),
            extra: (),
        }
    }
}

impl<B, E> Buffers<B, E> {
    #[inline]
    pub fn split(self) -> (Reader<B, E>, Writer<B, E>) { Arc::new(self).split_arc() }

    #[inline]
    pub fn extra<Ex>(self, extra: Ex) -> Buffers<B, Ex> {
        Buffers {
            raw: self.raw,
            tag_list: self.tag_list,
            ptr: AtomicPtr::new(ptr::null_mut()),
            extra,
        }
    }

    #[inline]
    pub fn swap_extra<F: FnOnce(E) -> Ex, Ex>(self, swap_extra: F) -> Buffers<B, Ex> {
        Buffers {
            raw: self.raw,
            tag_list: self.tag_list,
            ptr: AtomicPtr::new(ptr::null_mut()),
            extra: swap_extra(self.extra),
        }
    }
}

impl<B, E: ?Sized> Buffers<B, E> {
    #[inline]
    fn as_ptr(&self) -> *mut B { self.raw.get().cast() }

    pub fn split_arc(mut self: Arc<Self>) -> (Reader<B, E>, Writer<B, E>) {
        let buffers = Arc::get_mut(&mut self).expect("Cannot split a shared `Buffers`");
        let ptr = buffers.as_ptr();
        self.ptr.store(ptr, Ordering::Relaxed);
        let reader = Reader::new(&self);
        let writer = Writer {
            ptr: unsafe { SyncPtr(ptr.add(1)) },
            buffers: self,
        };
        (reader, writer)
    }
}

impl<B, E> Swap<B, E> {
    pub fn reader(&self) -> Reader<B, E> { self.writer.reader() }

    pub fn read(&self) -> &B { self.writer.read() }

    pub fn extra(&self) -> &E { self.writer.extra() }

    pub fn continue_swap(mut self) -> Result<Writer<B, E>, Self> {
        if unsafe { self.packet.continue_buffer_swap_unchecked() } {
            Err(self)
        } else {
            Ok(self.writer)
        }
    }
}

impl RawSwap {
    pub unsafe fn continue_buffer_swap_unchecked(&mut self) -> bool {
        let tags = &mut self.0;

        tags.retain(|(epoch, enter_epoch)| {
            let enter_epoch = *enter_epoch;
            let current_epoch = epoch.load(Ordering::Relaxed);
            current_epoch == enter_epoch
        });

        if tags.is_empty() {
            atomic::fence(Ordering::SeqCst);
            false
        } else {
            true
        }
    }
}

impl<B, E: ?Sized> Writer<B, E> {
    #[inline]
    pub fn reader(&self) -> Reader<B, E> { Reader::new(&self.buffers) }

    #[inline]
    pub fn read(&self) -> &B {
        unsafe {
            let ptr = &(*self.buffers).ptr;
            let ptr = ptr as *const AtomicPtr<B> as *const *const B;
            &**ptr
        }
    }

    #[inline]
    pub fn extra(&self) -> &E { &self.buffers.extra }

    #[inline]
    pub fn split(&mut self) -> (&B, &mut B, &E) {
        unsafe {
            let buffers = &(*self.buffers);
            let reader_ptr = &buffers.ptr;
            let reader_ptr = reader_ptr as *const AtomicPtr<B> as *const *const B;
            (&**reader_ptr, &mut *self.ptr.0, &buffers.extra)
        }
    }

    pub fn start_buffer_swap(mut self) -> Swap<B, E> {
        let packet = unsafe { self.start_buffer_swap_unchecked() };
        Swap { writer: self, packet }
    }

    #[inline]
    pub fn swap_buffers(&mut self) {
        use crossbeam_utils::Backoff;

        let backoff = Backoff::new();

        self.swap_buffers_with(|_| backoff.snooze());
    }

    #[inline]
    pub fn swap_buffers_with<'a, F: FnMut(&'a E)>(&'a mut self, ref mut callback: F) {
        fn swap_buffers_with<'a, B, E: ?Sized>(writer: &'a mut Writer<B, E>, callback: &mut dyn FnMut(&'a E)) {
            let mut packet = unsafe { writer.start_buffer_swap_unchecked() };
            let extra = writer.extra();

            while unsafe { packet.continue_buffer_swap_unchecked() } {
                callback(extra);
            }
        }

        swap_buffers_with(self, callback)
    }

    #[inline]
    pub async fn async_swap_buffers_with<'a, F, A>(&'a mut self, ref mut callback: F)
    where
        F: FnMut(&'a E) -> A,
        A: Future<Output = ()>,
    {
        async fn swap_buffers_with<'a, B, E: ?Sized, A: Future<Output = ()>>(
            writer: &'a mut Writer<B, E>,
            callback: &mut dyn FnMut(&'a E) -> A,
        ) {
            let mut packet = unsafe { writer.start_buffer_swap_unchecked() };
            let extra = writer.extra();

            while unsafe { packet.continue_buffer_swap_unchecked() } {
                callback(extra).await;
            }
        }

        swap_buffers_with(self, callback).await
    }

    pub unsafe fn start_buffer_swap_unchecked(&mut self) -> RawSwap {
        atomic::fence(Ordering::SeqCst);

        self.ptr.0 = self.buffers.ptr.swap(self.ptr.0, Ordering::Release);

        RawSwap(
            self.buffers
                .tag_list
                .lock()
                .iter()
                // anything that is potentially accessing a buffer right now
                .filter_map(|(_, epoch)| {
                    let tag_value = epoch.load(Ordering::Relaxed);
                    if tag_value % 2 == 1 {
                        Some((epoch.clone(), tag_value))
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }

    #[inline]
    pub fn get_pinned_write_buffer(self: Pin<&mut Self>) -> Pin<&mut B> {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(self) as &mut B) }
    }
}

impl<B, E: ?Sized> Reader<B, E> {
    #[inline]
    fn new(buffers: &Arc<Buffers<B, E>>) -> Self {
        let epoch = Thin::new(AtomicU32::new(0));
        buffers.tag_list.lock().insert(epoch.clone());
        Self {
            buffers: Arc::downgrade(buffers),
            epoch,
        }
    }

    #[inline]
    pub fn try_clone(&self) -> Option<Self> { self.buffers.upgrade().as_ref().map(Self::new) }

    #[inline]
    pub fn is_dangling(&self) -> bool { self.buffers.strong_count() == 0 }

    #[inline]
    pub fn get(&mut self) -> ReaderGuard<'_, B, B, E> {
        self.try_get().expect("Tried to reader from a dangling `Reader<B>`")
    }

    #[inline]
    pub fn try_get(&mut self) -> Option<ReaderGuard<'_, B, B, E>> {
        self.epoch.fetch_add(1, Ordering::Acquire);

        let buffers = self.buffers.upgrade()?;
        let buffer = (*buffers).ptr.load(Ordering::Acquire);

        Some(ReaderGuard {
            value: unsafe { &*buffer },
            tag_guard: TagGuard {
                buffers,
                epoch: self.epoch.clone(),
            },
        })
    }
}

impl<'a, B, E: ?Sized> TagGuard<B, E> {
    pub fn extra(&self) -> &E { &self.buffers.extra }
}

impl<B, E: ?Sized> Deref for Writer<B, E> {
    type Target = B;

    #[inline]
    fn deref(&self) -> &Self::Target { unsafe { &*self.ptr.0 } }
}

impl<B, E: ?Sized> DerefMut for Writer<B, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { &mut *self.ptr.0 } }
}

impl<B, E: ?Sized> Drop for TagGuard<B, E> {
    #[inline]
    fn drop(&mut self) { self.epoch.fetch_add(1, Ordering::Release); }
}
