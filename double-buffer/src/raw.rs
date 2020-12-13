use core::pin::Pin;
use std::{
    cell::UnsafeCell,
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

type TagList = Mutex<Slab<Thin<AtomicTag>>>;

#[derive(Debug)]
struct AtomicTag {
    epoch: AtomicU32,
}

impl AtomicTag {
    fn new() -> Self {
        Self {
            epoch: AtomicU32::new(0),
        }
    }
}

pub struct Write<B, E: ?Sized = ()> {
    ptr: *mut B,
    buffers: Arc<Buffers<B, E>>,
}

pub struct Read<B, E: ?Sized = ()> {
    buffers: Weak<Buffers<B, E>>,
    tag: Thin<AtomicTag>,
}

pub type ReadGuard<'read, B, T = B, E = ()> = RawReadGuard<'read, T, ReadTagGuard<B, E>>;
pub struct RawReadGuard<'read, T: ?Sized, TagGuard> {
    value: &'read T,
    tag_guard: TagGuard,
}

pub struct ReadTagGuard<B, E: ?Sized> {
    tag: Thin<AtomicTag>,
    buffers: Arc<Buffers<B, E>>,
}

pub struct Buffers<B, E: ?Sized = ()> {
    ptr: AtomicPtr<B>,
    tag_list: TagList,
    raw: UnsafeCell<[B; 2]>,
    extra: E,
}

pub struct SwapPacket(SmallVec<[(Thin<AtomicTag>, u32); 8]>);

pub struct Swap<B, E: ?Sized> {
    write: Write<B, E>,
    packet: SwapPacket,
}

unsafe impl<B: Send, E: Send> Send for Buffers<B, E> {}
unsafe impl<B: Sync, E: Sync> Sync for Buffers<B, E> {}

unsafe impl<B: Send + Sync> Send for Write<B> {}
unsafe impl<B: Send + Sync> Sync for Write<B> {}

impl<B: Unpin> Unpin for Write<B> {}

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
    pub fn split(self) -> (Write<B, E>, Read<B, E>) { Arc::new(self).split_arc() }

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

    pub fn split_arc(mut self: Arc<Self>) -> (Write<B, E>, Read<B, E>) {
        let buffers = Arc::get_mut(&mut self).expect("Cannot split a shared `Buffers`");
        let ptr = buffers.as_ptr();
        self.ptr.store(ptr, Ordering::Relaxed);
        let read = Read::new(&self);
        let write = Write {
            ptr: unsafe { ptr.add(1) },
            buffers: self,
        };
        (write, read)
    }
}

impl<B, E> Swap<B, E> {
    pub fn reader(&self) -> Read<B, E> { self.write.reader() }

    pub fn read(&self) -> &B { self.write.read() }

    pub fn extra(&self) -> &E { self.write.extra() }

    pub fn continue_swap(mut self) -> Result<Write<B, E>, Self> {
        if unsafe { self.write.continue_buffer_swap_unchecked(&mut self.packet) } {
            Err(self)
        } else {
            Ok(self.write)
        }
    }
}

impl<B, E: ?Sized> Write<B, E> {
    #[inline]
    pub fn reader(&self) -> Read<B, E> { Read::new(&self.buffers) }

    #[inline]
    pub fn read(&self) -> &B {
        unsafe {
            let ptr = &(*self.buffers).ptr;
            let ptr = ptr as *const AtomicPtr<B> as *const *const B;
            &**ptr
        }
    }

    #[inline]
    pub fn split(&mut self) -> (&B, &mut B, &E) {
        unsafe {
            let buffers = &(*self.buffers);
            let read_ptr = &buffers.ptr;
            let read_ptr = read_ptr as *const AtomicPtr<B> as *const *const B;
            (&**read_ptr, &mut *self.ptr, &buffers.extra)
        }
    }

    #[inline]
    pub fn extra(&self) -> &E { &self.buffers.extra }

    pub fn start_buffer_swap(mut self) -> Swap<B, E> {
        let packet = unsafe { self.start_buffer_swap_unchecked() };
        Swap { write: self, packet }
    }

    #[inline]
    pub fn swap_buffers(&mut self) {
        use crossbeam_utils::Backoff;

        let backoff = Backoff::new();

        self.swap_buffers_with(|_| backoff.snooze());
    }

    #[inline]
    pub fn swap_buffers_with<F: FnMut(&E)>(&mut self, ref mut callback: F) {
        fn swap_buffers_with<B, E: ?Sized>(write: &mut Write<B, E>, callback: &mut dyn FnMut(&E)) {
            let mut swap = unsafe { write.start_buffer_swap_unchecked() };
            let swap = &mut swap;

            while unsafe { write.continue_buffer_swap_unchecked(swap) } {
                callback(write.extra());
            }
        }

        swap_buffers_with(self, callback)
    }

    unsafe fn start_buffer_swap_unchecked(&mut self) -> SwapPacket {
        atomic::fence(Ordering::SeqCst);

        self.ptr = self.buffers.ptr.swap(self.ptr, Ordering::Release);

        SwapPacket(
            self.buffers
                .tag_list
                .lock()
                .iter()
                // anything that is potentially accessing a buffer right now
                .filter_map(|(_, tag)| {
                    let tag_value = tag.epoch.load(Ordering::Relaxed);
                    if tag_value % 2 == 1 {
                        Some((tag.clone(), tag_value))
                    } else {
                        None
                    }
                })
                .collect(),
        )
    }

    unsafe fn continue_buffer_swap_unchecked(&mut self, packet: &mut SwapPacket) -> bool {
        let tags = &mut packet.0;

        tags.retain(|(tag, enter_epoch)| {
            let enter_epoch = *enter_epoch;
            let current_epoch = tag.epoch.load(Ordering::Relaxed);
            current_epoch == enter_epoch
        });

        if tags.is_empty() {
            atomic::fence(Ordering::SeqCst);
            false
        } else {
            true
        }
    }

    #[inline]
    pub fn get_pinned_write_buffer(self: Pin<&mut Self>) -> Pin<&mut B> {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(self) as &mut B) }
    }
}

impl<B, E: ?Sized> Read<B, E> {
    #[inline]
    fn new(buffers: &Arc<Buffers<B, E>>) -> Self {
        let tag = Thin::new(AtomicTag::new());
        buffers.tag_list.lock().insert(tag.clone());
        Self {
            buffers: Arc::downgrade(buffers),
            tag,
        }
    }

    #[inline]
    pub fn try_clone(&self) -> Option<Self> { self.buffers.upgrade().as_ref().map(Self::new) }

    #[inline]
    pub fn is_dangling(&self) -> bool { self.buffers.strong_count() == 0 }

    #[inline]
    pub fn get(&mut self) -> ReadGuard<'_, B, B, E> { self.try_get().expect("Tried to read from a dangling `Read<B>`") }

    #[inline]
    pub fn try_get(&mut self) -> Option<ReadGuard<'_, B, B, E>> {
        self.tag.epoch.fetch_add(1, Ordering::Acquire);

        let buffers = self.buffers.upgrade()?;
        let buffer = (*buffers).ptr.load(Ordering::Acquire);

        Some(ReadGuard {
            value: unsafe { &*buffer },
            tag_guard: ReadTagGuard {
                buffers,
                tag: self.tag.clone(),
            },
        })
    }
}

impl<'a, B, E: ?Sized> ReadTagGuard<B, E> {
    pub fn extra(&self) -> &E { &self.buffers.extra }
}

impl<'a, T: ?Sized, TagGuard> RawReadGuard<'a, T, TagGuard> {
    #[inline]
    pub fn tag_guard(this: &Self) -> &TagGuard { &this.tag_guard }

    pub unsafe fn map_tag_guard<NewTagGuard>(
        this: Self,
        f: impl FnOnce(TagGuard) -> NewTagGuard,
    ) -> RawReadGuard<'a, T, NewTagGuard> {
        RawReadGuard {
            value: this.value,
            tag_guard: f(this.tag_guard),
        }
    }

    #[inline]
    pub fn map<F, U: ?Sized>(this: Self, f: F) -> RawReadGuard<'a, U, TagGuard>
    where
        F: for<'val> FnOnce(&'val T, &TagGuard) -> &'val U,
    {
        RawReadGuard {
            value: f(this.value, Self::tag_guard(&this)),
            tag_guard: this.tag_guard,
        }
    }

    #[inline]
    pub fn try_map<F, U: ?Sized>(this: Self, f: F) -> Result<RawReadGuard<'a, U, TagGuard>, Self>
    where
        F: for<'val> FnOnce(&'val T, &TagGuard) -> Option<&'val U>,
    {
        match f(this.value, Self::tag_guard(&this)) {
            None => Err(this),
            Some(value) => Ok(RawReadGuard {
                value,
                tag_guard: this.tag_guard,
            }),
        }
    }

    #[inline]
    pub fn try_map_res<F, U: ?Sized, E>(this: Self, f: F) -> Result<RawReadGuard<'a, U, TagGuard>, (Self, E)>
    where
        F: for<'val> FnOnce(&'val T, &TagGuard) -> Result<&'val U, E>,
    {
        match f(this.value, Self::tag_guard(&this)) {
            Err(e) => Err((this, e)),
            Ok(value) => Ok(RawReadGuard {
                value,
                tag_guard: this.tag_guard,
            }),
        }
    }
}

impl<B, E: ?Sized> Deref for Write<B, E> {
    type Target = B;

    #[inline]
    fn deref(&self) -> &Self::Target { unsafe { &*self.ptr } }
}

impl<B, E: ?Sized> DerefMut for Write<B, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { &mut *self.ptr } }
}

impl<T: ?Sized, TagGuard> Deref for RawReadGuard<'_, T, TagGuard> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target { self.value }
}

impl<B, E: ?Sized> Drop for ReadTagGuard<B, E> {
    #[inline]
    fn drop(&mut self) { self.tag.epoch.fetch_add(1, Ordering::Release); }
}
