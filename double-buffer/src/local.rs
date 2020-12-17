use core::cell::Cell;

use crate::Strategy;

pub type BufferData<B, E = ()> = crate::BufferData<core::cell::Cell<bool>, LocalStrategy, B, E>;

#[cfg(feature = "alloc")]
pub mod owned {
    pub type BufferRef<B, E = ()> = core::pin::Pin<std::rc::Rc<super::BufferData<B, E>>>;
    pub type Writer<B, E = ()> = crate::Writer<BufferRef<B, E>>;
    pub type Reader<B, E = ()> = crate::Reader<BufferRef<B, E>>;
    pub type ReaderGuard<'reader, B, T = B, E = ()> = crate::ReaderGuard<'reader, BufferRef<B, E>, T>;
}

#[cfg(feature = "alloc")]
pub mod thin {
    pub type BufferRef<B, E = ()> = std::boxed::Box<crate::thin::LocalThinInner<super::BufferData<B, E>>>;
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
pub struct LocalStrategy {
    num_readers: Cell<usize>,
}

pub struct RawGuard(());
pub struct Capture(());

#[cold]
#[inline(never)]
fn swap_buffers_fail() -> ! { panic!("Tried to swap buffers of a local-double buffer while readers were reading!") }

unsafe impl Strategy for LocalStrategy {
    type ReaderTag = ();
    type Capture = Capture;
    type RawGuard = RawGuard;

    #[inline]
    fn create_tag(&self) -> Self::ReaderTag {}

    #[inline]
    fn fence(&self) {}

    #[inline]
    fn capture_readers(&self) -> Self::Capture {
        if self.num_readers.get() != 0 {
            swap_buffers_fail()
        }

        Capture(())
    }

    #[inline]
    fn is_capture_complete(&self, _: &mut Self::Capture) -> bool { true }

    #[inline]
    fn begin_guard(&self, _: &Self::ReaderTag) -> Self::RawGuard {
        let num_readers = &self.num_readers;
        num_readers.set(
            num_readers
                .get()
                .checked_add(1)
                .expect("Tried to create too many readers!"),
        );
        RawGuard(())
    }

    #[inline]
    fn end_guard(&self, _: Self::RawGuard) {
        let num_readers = &self.num_readers;
        num_readers.set(num_readers.get().wrapping_sub(1));
    }
}
