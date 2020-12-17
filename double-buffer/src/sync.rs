use crate::{thin::SyncThin as Thin, Strategy};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use smallvec::SmallVec;

#[cfg(feature = "std")]
use parking_lot::Mutex;
#[cfg(not(feature = "std"))]
use spin::Mutex;

#[cfg(feature = "std")]
pub mod park;

pub type BufferData<B, E = ()> = crate::BufferData<AtomicBool, B, SyncStrategy, E>;

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
pub struct SyncStrategy {
    tag_list: Mutex<SmallVec<[Thin<AtomicU32>; 8]>>,
}

pub struct Tag(Thin<AtomicU32>);
pub struct Capture {
    active: SmallVec<[Thin<AtomicU32>; 8]>,
}

pub struct RawGuard {
    tag: Thin<AtomicU32>,
}

unsafe impl Strategy for SyncStrategy {
    type ReaderTag = Tag;
    type Capture = Capture;
    type RawGuard = RawGuard;

    #[inline]
    fn create_tag(&self) -> Self::ReaderTag {
        let tag = Thin::new(AtomicU32::new(0));
        self.tag_list.lock().push(tag.clone());
        Tag(tag)
    }

    #[inline]
    fn fence(&self) { core::sync::atomic::fence(Ordering::SeqCst); }

    #[inline]
    fn capture_readers(&self) -> Self::Capture {
        let mut active = SmallVec::new();

        self.tag_list.lock().retain(|tag| {
            let is_alive = Thin::strong_count(tag) != 1;

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
    fn begin_guard(&self, tag: &Self::ReaderTag) -> Self::RawGuard {
        tag.0.fetch_add(1, Ordering::Acquire);
        RawGuard { tag: tag.0.clone() }
    }

    #[inline]
    fn end_guard(&self, guard: Self::RawGuard) { guard.tag.fetch_add(1, Ordering::Acquire); }
}
