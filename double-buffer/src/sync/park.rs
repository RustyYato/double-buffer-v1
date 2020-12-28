use crate::{sync::Capture as RawCapture, Strategy};
use core::sync::atomic::AtomicBool;
use crossbeam_utils::Backoff;
use parking_lot::Condvar;

pub type BufferData<B, E = ()> = crate::BufferData<AtomicBool, ParkStrategy, B, E>;

crate::__imp_make_newtype! {
    crate::sync::park::ParkStrategy, core::convert::Infallible, ArcInner, std::sync::Arc
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
    type RawGuard = RawGuard;

    type FastCapture = ();
    type CaptureError = core::convert::Infallible;
    type Capture = Capture;

    #[inline]
    unsafe fn reader_tag(&self) -> Self::ReaderTag { ReaderTag(self.raw.reader_tag()) }

    #[inline]
    unsafe fn writer_tag(&self) -> Self::WriterTag { WriterTag(self.raw.writer_tag()) }

    #[inline]
    fn try_capture_readers(
        &self,
        WriterTag(tag): &mut Self::WriterTag,
    ) -> Result<Self::FastCapture, Self::CaptureError> {
        self.raw.try_capture_readers(tag)
    }

    #[inline]
    fn finish_capture_readers(&self, WriterTag(tag): &mut Self::WriterTag, (): Self::FastCapture) -> Self::Capture {
        Capture {
            raw: self.raw.finish_capture_readers(tag, ()),
            backoff: Backoff::new(),
        }
    }

    #[inline]
    fn readers_have_exited(&self, capture: &mut Self::Capture) -> bool {
        #[cold]
        fn cold(strategy: &ParkStrategy, backoff: &Backoff) {
            if backoff.is_completed() {
                strategy.park();
            } else {
                backoff.snooze();
            }
        }

        let is_completed = self.raw.readers_have_exited(&mut capture.raw);

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
