use core::cell::Cell;

use crate::Strategy;

pub type BufferData<B, E = ()> = crate::BufferData<core::cell::Cell<bool>, LocalStrategy, B, E>;

#[cfg(feature = "alloc")]
pub mod owned {
    pub type BufferRef<B, E = ()> = std::rc::Rc<super::BufferData<B, E>>;
    pub type Writer<B, E = ()> = crate::raw::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::raw::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::raw::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

#[cfg(feature = "alloc")]
pub mod thin {
    pub type BufferRef<B, E = ()> = std::boxed::Box<crate::thin::RcInner<super::BufferData<B, E>>>;
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
pub struct LocalStrategy {
    num_readers: Cell<usize>,
}

pub struct RawGuard(());
pub struct Capture(());

pub struct ReaderTag(());
pub struct WriterTag(());

pub struct CaptureError;

impl core::fmt::Debug for CaptureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Tried to start a swap while there are readers active")
    }
}

#[cold]
#[inline(never)]
fn begin_guard_fail<T>() -> T { panic!("Tried to create too many readers!") }

impl LocalStrategy {
    pub fn try_swap_buffers<B: crate::BufferRef<Strategy = Self>>(writer: &mut crate::raw::Writer<B>) -> bool {
        use crate::raw::Writer;

        let strategy: &Self = Writer::strategy(writer);

        let can_swap = strategy.num_readers.get() == 0;

        if can_swap {
            unsafe {
                Writer::swap_buffers_unchecked(writer);
            }
        }

        can_swap
    }
}

unsafe impl Strategy for LocalStrategy {
    type Whitch = core::cell::Cell<bool>;
    type ReaderTag = ReaderTag;
    type WriterTag = WriterTag;
    type RawGuard = RawGuard;

    type FastCapture = Capture;
    type CaptureError = CaptureError;
    type Capture = Capture;

    #[inline]
    unsafe fn reader_tag(&self) -> Self::ReaderTag { ReaderTag(()) }

    #[inline]
    unsafe fn writer_tag(&self) -> Self::WriterTag { WriterTag(()) }

    #[inline]
    fn try_capture_readers(&self, _: &mut Self::WriterTag) -> Result<Self::FastCapture, Self::CaptureError> {
        if self.num_readers.get() == 0 {
            Ok(Capture(()))
        } else {
            Err(CaptureError)
        }
    }

    #[inline]
    fn finish_capture_readers(&self, _: &mut Self::WriterTag, capture: Self::FastCapture) -> Self::Capture { capture }

    #[inline]
    fn readers_have_exited(&self, _: &mut Self::Capture) -> bool { true }

    #[inline]
    fn begin_guard(&self, _: &mut Self::ReaderTag) -> Self::RawGuard {
        let num_readers = &self.num_readers;
        num_readers.set(match num_readers.get().checked_add(1) {
            Some(x) => x,
            None => begin_guard_fail(),
        });
        RawGuard(())
    }

    #[inline]
    fn end_guard(&self, _: Self::RawGuard) {
        let num_readers = &self.num_readers;
        num_readers.set(num_readers.get().wrapping_sub(1));
    }
}
