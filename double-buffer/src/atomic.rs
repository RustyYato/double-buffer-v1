use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::Strategy;

pub type BufferData<B, E = ()> = crate::BufferData<AtomicBool, AtomicStrategy, B, E>;

#[cfg(feature = "alloc")]
pub mod owned {
    pub type BufferRef<B, E = ()> = core::pin::Pin<std::sync::Arc<super::BufferData<B, E>>>;
    pub type Writer<B, E = ()> = crate::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

#[cfg(feature = "alloc")]
pub mod thin {
    pub type BufferRef<B, E = ()> = std::boxed::Box<crate::thin::ArcInner<super::BufferData<B, E>>>;
    pub type Writer<B, E = ()> = crate::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

pub mod reference {
    pub type BufferRef<'buf_data, B, E = ()> = core::pin::Pin<&'buf_data mut super::BufferData<B, E>>;
    pub type Writer<'buf_data, B, E = ()> = crate::Writer<BufferRef<'buf_data, B, E>>;
    pub type Reader<'buf_data, B, E = ()> = crate::Reader<BufferRef<'buf_data, B, E>>;
    pub type ReaderGuard<'reader, 'buf_data, B, T = B, E = ()> =
        crate::ReaderGuard<'reader, BufferRef<'buf_data, B, E>, T>;
}

#[derive(Default)]
pub struct AtomicStrategy {
    num_readers: AtomicUsize,
}

pub struct RawGuard(());
pub struct Capture(());

pub struct ReaderTag(());
#[derive(Clone, Copy)]
pub struct WriterTag(());

unsafe impl Strategy for AtomicStrategy {
    type ReaderTag = ReaderTag;
    type WriterTag = WriterTag;
    type Capture = Capture;
    type RawGuard = RawGuard;

    #[inline]
    unsafe fn reader_tag(&self) -> Self::ReaderTag { ReaderTag(()) }

    #[inline]
    unsafe fn writer_tag(&self) -> Self::WriterTag { WriterTag(()) }

    #[inline]
    fn fence(&self, _: Self::WriterTag) {}

    #[inline]
    fn capture_readers(&self, _: Self::WriterTag) -> Self::Capture { Capture(()) }

    #[inline]
    fn is_capture_complete(&self, _: &mut Self::Capture, _: Self::WriterTag) -> bool {
        self.num_readers.load(Ordering::Acquire) == 0
    }

    fn begin_guard(&self, _: &Self::ReaderTag) -> Self::RawGuard {
        use crossbeam_utils::Backoff;

        let num_readers = &self.num_readers;
        let mut value = num_readers.load(Ordering::Acquire);

        let backoff = Backoff::new();

        loop {
            if let Some(next_value) = value.checked_add(1) {
                if let Err(current) =
                    num_readers.compare_exchange_weak(value, next_value, Ordering::Acquire, Ordering::Acquire)
                {
                    value = current;
                } else {
                    return RawGuard(())
                }
            }

            crate::snooze(&backoff)
        }
    }

    #[inline]
    fn end_guard(&self, _: Self::RawGuard) { self.num_readers.fetch_sub(1, Ordering::Release); }
}
