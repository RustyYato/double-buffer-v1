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
    type Capture = Capture;
    type RawGuard = RawGuard;

    #[inline]
    unsafe fn reader_tag(&self) -> Self::ReaderTag {
        let tag = Arc::new(AtomicU32::new(0));
        self.tag_list.lock().push(tag.clone());
        ReaderTag(tag)
    }

    #[inline]
    unsafe fn writer_tag(&self) -> Self::WriterTag { WriterTag(()) }

    #[inline]
    fn fence(&self) { core::sync::atomic::fence(Ordering::SeqCst); }

    #[inline]
    fn capture_readers(&self, _: &mut Self::WriterTag) -> Self::Capture {
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
    fn is_capture_complete(&self, capture: &mut Self::Capture) -> bool {
        capture.active.retain(|tag| tag.load(Ordering::Relaxed) & 1 == 1);

        capture.active.is_empty()
    }

    #[inline]
    fn begin_guard(&self, tag: &mut Self::ReaderTag) -> Self::RawGuard {
        tag.0.fetch_add(1, Ordering::Acquire);
        RawGuard { tag: tag.0.clone() }
    }

    #[inline]
    fn end_guard(&self, guard: Self::RawGuard) { guard.tag.fetch_add(1, Ordering::Acquire); }
}
