use parking_lot::{Condvar, Mutex};
use std::{
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::Arc,
};

use crate::raw;

#[repr(transparent)]
pub struct Writer<B, E: ?Sized = ()> {
    raw: raw::Writer<B, Extra<E>>,
}

#[repr(transparent)]
pub struct Reader<B, E: ?Sized = ()> {
    raw: raw::Reader<B, Extra<E>>,
}

#[repr(transparent)]
pub struct TagGuard<B, E: ?Sized = ()> {
    raw: raw::TagGuard<B, Extra<E>>,
}

#[repr(transparent)]
pub struct Buffers<B, E: ?Sized = ()> {
    raw: raw::Buffers<B, Extra<E>>,
}

pub type ReaderGuard<'reader, B, T = B, E = ()> = raw::RawReaderGuard<'reader, T, TagGuard<B, E>>;

struct Extra<E: ?Sized> {
    lock: Mutex<()>,
    cv: Condvar,
    extra: E,
}

pub struct Swap<B, E: ?Sized> {
    raw: raw::Swap<B, Extra<E>>,
}

impl<B: Default, E: Default> Default for Buffers<B, E> {
    #[inline]
    fn default() -> Self { Buffers::new(Default::default(), Default::default()).extra(Default::default()) }
}

impl<B> Buffers<B> {
    #[inline]
    pub fn new(front: B, back: B) -> Self {
        Self {
            raw: raw::Buffers::new(front, back).extra(Extra {
                lock: Mutex::new(()),
                cv: Condvar::new(),
                extra: (),
            }),
        }
    }
}

impl<B, E> Buffers<B, E> {
    #[inline]
    pub fn split(self) -> (Reader<B, E>, Writer<B, E>) { Arc::new(self).split_arc() }

    #[inline]
    pub fn extra<Ex>(self, extra: Ex) -> Buffers<B, Ex> {
        Buffers {
            raw: self.raw.swap_extra(|old| Extra {
                lock: old.lock,
                cv: old.cv,
                extra,
            }),
        }
    }

    #[inline]
    pub fn swap_extra<F: FnOnce(E) -> Ex, Ex>(self, swap_extra: F) -> Buffers<B, Ex> {
        Buffers {
            raw: self.raw.swap_extra(|old| Extra {
                lock: old.lock,
                cv: old.cv,
                extra: swap_extra(old.extra),
            }),
        }
    }
}

impl<B, E: ?Sized> Buffers<B, E> {
    pub fn split_arc(self: Arc<Self>) -> (Reader<B, E>, Writer<B, E>) {
        let this = unsafe { Arc::from_raw(Arc::into_raw(self) as *const raw::Buffers<B, Extra<E>>) };
        let (reader, writer) = this.split_arc();
        (Reader { raw: reader }, Writer { raw: writer })
    }
}

impl<B, E: ?Sized> Reader<B, E> {
    #[inline]
    pub fn try_clone(&self) -> Option<Self> {
        Some(Self {
            raw: self.raw.try_clone()?,
        })
    }

    #[inline]
    pub fn is_dangling(&self) -> bool { self.raw.is_dangling() }

    #[inline]
    pub fn get(&mut self) -> ReaderGuard<'_, B, B, E> {
        self.try_get().expect("Tried to reader from a dangling `Reader<B>`")
    }

    #[inline]
    pub fn try_get(&mut self) -> Option<ReaderGuard<'_, B, B, E>> {
        fn map_tag_guard<B, E: ?Sized>(raw: raw::TagGuard<B, Extra<E>>) -> TagGuard<B, E> { TagGuard { raw } }

        fn map_guard<B, E: ?Sized>(raw: raw::ReaderGuard<B, B, Extra<E>>) -> ReaderGuard<B, B, E> {
            unsafe { raw::RawReaderGuard::map_tag_guard(raw, map_tag_guard) }
        }

        self.raw.try_get().map(map_guard)
    }
}

#[cold]
#[inline(never)]
fn sleep(lock: &Mutex<()>, cv: &Condvar) { cv.wait(&mut lock.lock()); }

impl<B, E> Swap<B, E> {
    pub fn reader(&self) -> Reader<B, E> { Reader { raw: self.raw.reader() } }

    pub fn read(&self) -> &B { self.raw.read() }

    pub fn extra(&self) -> &E { &self.raw.extra().extra }

    pub fn sleep(&mut self) {
        let Extra { lock, cv, .. } = self.raw.extra();
        sleep(lock, cv);
    }

    pub fn continue_swap(self) -> Result<Writer<B, E>, Self> {
        match self.raw.continue_swap() {
            Ok(raw) => Ok(Writer { raw }),
            Err(raw) => Err(Self { raw }),
        }
    }
}

impl<B, E: ?Sized> Writer<B, E> {
    #[inline]
    pub fn reader(&self) -> Reader<B, E> { Reader { raw: self.raw.reader() } }

    #[inline]
    pub fn read(&self) -> &B { self.raw.read() }

    #[inline]
    pub fn split(&mut self) -> (&B, &mut B, &E) {
        let (reader, writer, Extra { extra, .. }) = self.raw.split();
        (reader, writer, extra)
    }

    #[inline]
    pub fn extra(&self) -> &E { &self.raw.extra().extra }

    pub fn start_buffer_swap(self) -> Swap<B, E> {
        Swap {
            raw: self.raw.start_buffer_swap(),
        }
    }

    pub fn swap_buffers(&mut self) { self.swap_buffers_with(drop); }

    pub fn swap_buffers_with<'a, F: FnMut(&'a E)>(&'a mut self, mut callback: F) {
        use crossbeam_utils::Backoff;

        let backoff = Backoff::new();

        self.raw.swap_buffers_with(move |Extra { extra, lock, cv }| {
            callback(extra);

            if backoff.is_completed() {
                sleep(lock, cv);
                backoff.reset();
            } else {
                backoff.spin();
            }
        })
    }

    #[inline]
    pub fn get_pinned_write_buffer(self: Pin<&mut Self>) -> Pin<&mut B> {
        unsafe { Pin::new_unchecked(Pin::into_inner_unchecked(self) as &mut B) }
    }
}

impl<'a, B, Ex: ?Sized> TagGuard<B, Ex> {
    #[inline]
    pub fn extra(&self) -> &Ex { &self.raw.extra().extra }
}

impl<B, E: ?Sized> Deref for Writer<B, E> {
    type Target = B;

    #[inline]
    fn deref(&self) -> &Self::Target { &self.raw }
}

impl<B, E: ?Sized> DerefMut for Writer<B, E> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.raw }
}

impl<B, E: ?Sized> Drop for TagGuard<B, E> {
    #[inline]
    fn drop(&mut self) { self.raw.extra().cv.notify_one(); }
}
