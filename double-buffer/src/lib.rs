#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(feature = "alloc", not(feature = "std")))]
extern crate alloc as std;

#[cfg(feature = "alloc")]
pub mod op;

#[cfg(feature = "alloc")]
pub mod left_right;

#[cfg(feature = "alloc")]
pub mod thin;

pub mod raw;
pub use raw::{new, BufferData, Reader, Writer};

pub mod atomic;
pub mod local;
#[cfg(feature = "alloc")]
pub mod sync;

#[cfg(test)]
mod tests;

mod buffer_ref;

use core::ops::Deref;
use radium::Radium;

use seal::Seal;
mod seal {
    // this forbid lint prevents accidentally leaking `Seal` in obvious ways
    #[forbid(missing_docs)]
    pub trait Seal {}
}

pub unsafe trait TrustedRadium: Radium + Seal {
    #[doc(hidden)]
    const IS_LOCAL: bool;
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item;
}

impl Seal for core::cell::Cell<bool> {}
unsafe impl TrustedRadium for core::cell::Cell<bool> {
    #[doc(hidden)]
    const IS_LOCAL: bool = true;
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item { self.get() }
}

impl Seal for core::sync::atomic::AtomicBool {}
unsafe impl TrustedRadium for core::sync::atomic::AtomicBool {
    #[doc(hidden)]
    const IS_LOCAL: bool = false;
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item {
        core::ptr::read(self as *const core::sync::atomic::AtomicBool as *const bool)
    }
}

impl Seal for core::cell::Cell<usize> {}
unsafe impl TrustedRadium for core::cell::Cell<usize> {
    #[doc(hidden)]
    const IS_LOCAL: bool = true;
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item { self.get() }
}

impl Seal for core::sync::atomic::AtomicUsize {}
unsafe impl TrustedRadium for core::sync::atomic::AtomicUsize {
    #[doc(hidden)]
    const IS_LOCAL: bool = false;
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item {
        core::ptr::read(self as *const core::sync::atomic::AtomicUsize as *const usize)
    }
}

pub type BufferRefData<BR> =
    BufferData<Whitch<BR>, <BR as BufferRef>::Strategy, <BR as BufferRef>::Buffer, <BR as BufferRef>::Extra>;

type Whitch<BR> = <<BR as BufferRef>::Strategy as Strategy>::Whitch;
type ReaderTag<BR> = <<BR as BufferRef>::Strategy as Strategy>::ReaderTag;
type WriterTag<BR> = <<BR as BufferRef>::Strategy as Strategy>::WriterTag;
type Capture<BR> = <<BR as BufferRef>::Strategy as Strategy>::Capture;

pub unsafe trait BufferRef: Sized {
    type Buffer;
    type Strategy: Strategy;
    type Extra: ?Sized;
    type UpgradeError: core::fmt::Debug;

    type Strong: Clone + Deref<Target = BufferRefData<Self>>;
    type Weak: Clone;

    fn split(self) -> (Self::Strong, Self::Weak);

    fn is_dangling(weak: &Self::Weak) -> bool;

    fn upgrade(weak: &Self::Weak) -> Result<Self::Strong, Self::UpgradeError>;

    fn downgrade(strong: &Self::Strong) -> Self::Weak;
}

pub unsafe trait Strategy: Sized {
    type Whitch: TrustedRadium<Item = bool>;
    type ReaderTag;
    type WriterTag;
    type Capture;
    type RawGuard;

    unsafe fn reader_tag(&self) -> Self::ReaderTag;

    unsafe fn writer_tag(&self) -> Self::WriterTag;

    fn fence(&self);

    fn capture_readers(&self, tag: &mut Self::WriterTag) -> Self::Capture;

    fn is_capture_complete(&self, capture: &mut Self::Capture) -> bool;

    fn begin_guard(&self, tag: &mut Self::ReaderTag) -> Self::RawGuard;

    fn end_guard(&self, guard: Self::RawGuard);

    fn is_swap_completed<B: BufferRef<Strategy = Self>>(&self, swap: &mut raw::Swap<B>) -> bool {
        raw::is_swap_completed(self, swap)
    }
}
