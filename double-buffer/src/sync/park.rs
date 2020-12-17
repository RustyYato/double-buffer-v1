use crate::{
    sync::{Capture as RawCapture, SyncStrategy},
    Strategy,
};
use core::sync::atomic::AtomicBool;
use crossbeam_utils::Backoff;
use parking_lot::Condvar;

pub type BufferData<B, E = ()> = crate::BufferData<AtomicBool, B, ParkStrategy, E>;

pub mod owned {
    pub type BufferRef<B, E = ()> = core::pin::Pin<std::sync::Arc<super::BufferData<B, E>>>;
    pub type Writer<B, E = ()> = crate::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

pub mod thin {
    pub type BufferRef<B, E = ()> = std::boxed::Box<crate::thin::SyncThinInner<super::BufferData<B, E>>>;
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
pub struct ParkStrategy {
    raw: super::SyncStrategy,
    cv: Condvar,
}

pub struct Capture {
    raw: RawCapture,
    backoff: Backoff,
}

impl ParkStrategy {
    #[cold]
    #[inline(never)]
    fn park(&self) {
        self.cv
            .wait_for(&mut self.raw.tag_list.lock(), std::time::Duration::from_millis(10));
    }
}

unsafe impl Strategy for ParkStrategy {
    type ReaderTag = <SyncStrategy as Strategy>::ReaderTag;
    type Capture = Capture;
    type RawGuard = <SyncStrategy as Strategy>::RawGuard;

    #[inline]
    fn create_tag(&self) -> Self::ReaderTag { self.raw.create_tag() }

    #[inline]
    fn fence(&self) { self.raw.fence() }

    #[inline]
    fn capture_readers(&self) -> Self::Capture {
        Capture {
            raw: self.raw.capture_readers(),
            backoff: Backoff::new(),
        }
    }

    #[inline]
    fn is_capture_complete(&self, capture: &mut Self::Capture) -> bool {
        let is_completed = self.raw.is_capture_complete(&mut capture.raw);

        if !is_completed {
            if capture.backoff.is_completed() {
                self.park();
            } else {
                capture.backoff.snooze();
            }
        }

        is_completed
    }

    #[inline]
    fn begin_guard(&self, tag: &Self::ReaderTag) -> Self::RawGuard { self.raw.begin_guard(tag) }

    #[inline]
    fn end_guard(&self, guard: Self::RawGuard) { self.raw.end_guard(guard) }
}
