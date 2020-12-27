use crate::{sync::Capture as RawCapture, Strategy};
use core::sync::atomic::AtomicBool;
use crossbeam_utils::Backoff;
use parking_lot::Condvar;

pub type BufferData<B, E = ()> = crate::BufferData<AtomicBool, ParkStrategy, B, E>;

pub mod owned {
    pub type BufferRef<B, E = ()> = std::sync::Arc<super::BufferData<B, E>>;
    pub type Writer<B, E = ()> = crate::raw::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::raw::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::raw::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

pub mod thin {
    pub type BufferRef<B, E = ()> = std::boxed::Box<crate::thin::ArcInner<super::BufferData<B, E>>>;
    pub type Writer<B, E = ()> = crate::raw::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::raw::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::raw::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

pub mod reference {
    pub type BufferRef<'buf_data, B, E = ()> = &'buf_data mut super::BufferData<B, E>;
    pub type Writer<'buf_data, B, E = ()> = crate::raw::Writer<BufferRef<'buf_data, B, E>>;
    pub type Reader<'buf_data, B, E = ()> = crate::raw::Reader<BufferRef<'buf_data, B, E>>;
    pub type ReaderGuard<'reader, 'buf_data, B, T = B, E = ()> =
        crate::raw::ReaderGuard<'reader, BufferRef<'buf_data, B, E>, T>;
}

#[derive(Default)]
pub struct ParkStrategy {
    raw: super::SyncStrategy,
    cv: Condvar,
}

pub struct Capture {
    raw: RawCapture,
    backoff: Backoff,
}

pub struct ReaderTag(super::ReaderTag);
pub struct WriterTag(super::WriterTag);
pub struct RawGuard(super::RawGuard);

impl ParkStrategy {
    #[cold]
    #[inline(never)]
    fn park(&self) {
        self.cv
            .wait_for(&mut self.raw.tag_list.lock(), std::time::Duration::from_micros(100));
    }
}

unsafe impl Strategy for ParkStrategy {
    type Whitch = AtomicBool;
    type ReaderTag = ReaderTag;
    type WriterTag = WriterTag;
    type Capture = Capture;
    type RawGuard = RawGuard;

    #[inline]
    unsafe fn reader_tag(&self) -> Self::ReaderTag { ReaderTag(self.raw.reader_tag()) }

    #[inline]
    unsafe fn writer_tag(&self) -> Self::WriterTag { WriterTag(self.raw.writer_tag()) }

    #[inline]
    fn fence(&self) { self.raw.fence() }

    #[inline]
    fn capture_readers(&self, WriterTag(tag): &mut Self::WriterTag) -> Self::Capture {
        Capture {
            raw: self.raw.capture_readers(tag),
            backoff: Backoff::new(),
        }
    }

    #[inline]
    fn is_capture_complete(&self, capture: &mut Self::Capture) -> bool {
        #[cold]
        fn cold(strategy: &ParkStrategy, backoff: &Backoff) {
            if backoff.is_completed() {
                strategy.park();
            } else {
                backoff.snooze();
            }
        }

        let is_completed = self.raw.is_capture_complete(&mut capture.raw);

        if !is_completed {
            cold(self, &capture.backoff)
        }

        is_completed
    }

    #[inline]
    fn begin_guard(&self, ReaderTag(tag): &mut Self::ReaderTag) -> Self::RawGuard {
        RawGuard(self.raw.begin_guard(tag))
    }

    #[inline]
    fn end_guard(&self, RawGuard(guard): Self::RawGuard) { self.raw.end_guard(guard) }
}
