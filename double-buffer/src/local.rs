use core::{
    cell::{Cell, UnsafeCell},
    marker::Unpin,
    ops::{Deref, DerefMut},
    pin::Pin,
    ptr,
};
use std::rc::{Rc, Weak};

pub struct Writer<B, E: ?Sized = ()> {
    ptr: *mut B,
    buffers: Rc<Buffers<B, E>>,
}

pub struct Reader<B, E: ?Sized = ()> {
    buffers: Weak<Buffers<B, E>>,
}

pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::RawReaderGuard<'reader, T, ReadGuard<B, E>>;

pub struct ReadGuard<B, E: ?Sized> {
    buffers: Rc<Buffers<B, E>>,
}

pub struct Buffers<B, E: ?Sized = ()> {
    ptr: Cell<*mut B>,
    num_readers: Cell<usize>,
    raw: UnsafeCell<[B; 2]>,
    extra: E,
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
            num_readers: Cell::new(0),
            ptr: Cell::new(ptr::null_mut()),
            extra: (),
        }
    }
}

impl<B, E> Buffers<B, E> {
    #[inline]
    pub fn split(self) -> (Reader<B, E>, Writer<B, E>) { Rc::new(self).split_rc() }

    #[inline]
    pub fn extra<Ex>(self, extra: Ex) -> Buffers<B, Ex> {
        Buffers {
            raw: self.raw,
            num_readers: self.num_readers,
            ptr: self.ptr,
            extra,
        }
    }

    #[inline]
    pub fn swap_extra<F: FnOnce(E) -> Ex, Ex>(self, swap_extra: F) -> Buffers<B, Ex> {
        Buffers {
            raw: self.raw,
            num_readers: self.num_readers,
            ptr: self.ptr,
            extra: swap_extra(self.extra),
        }
    }
}

impl<B, E: ?Sized> Buffers<B, E> {
    #[inline]
    fn as_ptr(&self) -> *mut B { self.raw.get().cast() }

    pub fn split_rc(mut self: Rc<Self>) -> (Reader<B, E>, Writer<B, E>) {
        let buffers = Rc::get_mut(&mut self).expect("Cannot split a shared `Buffers`");
        let ptr = buffers.as_ptr();
        self.ptr.set(ptr);
        let reader = Reader::new(&self);
        let writer = Writer {
            ptr: unsafe { ptr.add(1) },
            buffers: self,
        };
        (reader, writer)
    }
}

impl<B, E: ?Sized> Writer<B, E> {
    #[inline]
    pub fn reader(&self) -> Reader<B, E> { Reader::new(&self.buffers) }

    #[inline]
    pub fn read(&self) -> &B { unsafe { &*self.buffers.ptr.get() } }

    #[inline]
    pub fn extra(&self) -> &E { &self.buffers.extra }

    #[inline]
    pub fn num_readers(&self) -> usize { self.buffers.num_readers.get() }

    #[inline]
    pub fn split(&mut self) -> (&B, &mut B, &E) {
        unsafe {
            let buffers = &*self.buffers;
            let reader_ptr = buffers.ptr.get();
            (&*reader_ptr, &mut *self.ptr, &buffers.extra)
        }
    }

    #[inline]
    pub fn swap_buffers(&mut self) {
        #[inline(never)]
        fn swap_buffers_fail() -> ! {
            panic!("Tried to swap buffers of a local-double buffer while readers were reading!")
        }

        if !self.try_swap_buffers() {
            swap_buffers_fail()
        }
    }

    #[inline]
    pub fn try_swap_buffers(&mut self) -> bool {
        if self.buffers.num_readers.get() == 0 {
            self.ptr = self.buffers.ptr.replace(self.ptr);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn get_pinned_write_buffer(self: Pin<&mut Self>) -> Pin<&mut B> {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(self) as &mut B) }
    }
}

impl<B, E: ?Sized> Reader<B, E> {
    #[inline]
    fn new(buffers: &Rc<Buffers<B, E>>) -> Self {
        Self {
            buffers: Rc::downgrade(buffers),
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
        let buffers = self.buffers.upgrade()?;

        buffers.num_readers.set(
            buffers
                .num_readers
                .get()
                .checked_add(1)
                .expect("Tried to read more than `usize::MAX` times!"),
        );

        let buffer = (*buffers).ptr.get();

        Some(ReaderGuard {
            value: unsafe { &*buffer },
            tag_guard: ReadGuard { buffers },
        })
    }
}

impl<'a, B, E: ?Sized> ReadGuard<B, E> {
    pub fn extra(&self) -> &E { &self.buffers.extra }
}

impl<B, E: ?Sized> Deref for Writer<B, E> {
    type Target = B;

    #[inline]
    fn deref(&self) -> &Self::Target { unsafe { &*self.ptr } }
}

impl<B, E: ?Sized> DerefMut for Writer<B, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target { unsafe { &mut *self.ptr } }
}

impl<B, E: ?Sized> Drop for ReadGuard<B, E> {
    #[inline]
    fn drop(&mut self) {
        self.buffers
            .num_readers
            .set(self.buffers.num_readers.get().wrapping_sub(1))
    }
}
