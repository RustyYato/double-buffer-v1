use crate::{thin::Arc, Strategy};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use smallvec::SmallVec;

#[cfg(feature = "std")]
use parking_lot::Mutex;
#[cfg(not(feature = "std"))]
use spin::Mutex;

#[cfg(feature = "std")]
pub mod park;

pub type BufferData<B, E = ()> = crate::BufferData<AtomicBool, SyncStrategy, B, E>;

crate::__imp_make_newtype! {
    crate::sync::SyncStrategy, core::convert::Infallible, ArcInner, std::sync::Arc
}

#[derive(Default)]
pub struct SyncStrategy {
    tag_list: Mutex<SmallVec<[Arc<AtomicU32>; 8]>>,
}

pub struct RawGuard {
    tag: Arc<AtomicU32>,
}

pub struct Capture {
    active: SmallVec<[Arc<AtomicU32>; 8]>,
}

pub struct ReaderTag(Arc<AtomicU32>);
pub struct WriterTag(());

unsafe impl Strategy for SyncStrategy {
    type Whitch = AtomicBool;
    type ReaderTag = ReaderTag;
    type WriterTag = WriterTag;
    type RawGuard = RawGuard;

    type FastCapture = ();
    type CaptureError = core::convert::Infallible;
    type Capture = Capture;

    #[inline]
    unsafe fn reader_tag(&self) -> Self::ReaderTag {
        let tag = Arc::new(AtomicU32::new(0));
        self.tag_list.lock().push(tag.clone());
        ReaderTag(tag)
    }

    #[inline]
    unsafe fn writer_tag(&self) -> Self::WriterTag { WriterTag(()) }

    #[inline]
    fn try_capture_readers(&self, _: &mut Self::WriterTag) -> Result<Self::FastCapture, Self::CaptureError> {
        core::sync::atomic::fence(Ordering::SeqCst);
        Ok(())
    }

    #[inline]
    fn finish_capture_readers(&self, _: &mut Self::WriterTag, (): Self::FastCapture) -> Self::Capture {
        let mut active = SmallVec::new();

        self.tag_list.lock().retain(|tag| {
            let is_alive = Arc::strong_count(tag) != 1;

            if is_alive && tag.load(Ordering::Acquire) & 1 == 1 {
                active.push(tag.clone())
            }

            is_alive
        });

        Capture { active }
    }

    #[inline]
    fn readers_have_exited(&self, capture: &mut Self::Capture) -> bool {
        capture.active.retain(|tag| tag.load(Ordering::Relaxed) & 1 == 1);

        let readers_have_exited = capture.active.is_empty();

        if readers_have_exited {
            core::sync::atomic::fence(Ordering::SeqCst);
        }

        readers_have_exited
    }

    #[inline]
    fn begin_guard(&self, tag: &mut Self::ReaderTag) -> Self::RawGuard {
        tag.0.fetch_add(1, Ordering::Acquire);
        RawGuard { tag: tag.0.clone() }
    }

    #[inline]
    fn end_guard(&self, guard: Self::RawGuard) { guard.tag.fetch_add(1, Ordering::Acquire); }
}
