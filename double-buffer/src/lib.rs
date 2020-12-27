#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(all(feature = "alloc", not(feature = "std")))]
extern crate alloc as std;

#[cfg(feature = "alloc")]
pub mod op;

#[cfg(feature = "alloc")]
pub mod left_right;

#[cfg(feature = "alloc")]
pub mod thin;

mod raw;
pub use raw::*;

pub mod atomic;
pub mod local;
#[cfg(feature = "alloc")]
pub mod sync;

#[cfg(test)]
mod tests;

mod buffer_ref;

use core::{ops::Deref, pin::Pin};
use radium::Radium;

pub unsafe trait TrustedRadium: Radium {
    #[doc(hidden)]
    const IS_LOCAL: bool;
    #[doc(hidden)]
    unsafe fn load_unchecked(&self) -> Self::Item;
}

unsafe impl TrustedRadium for core::cell::Cell<bool> {
    const IS_LOCAL: bool = true;
    unsafe fn load_unchecked(&self) -> Self::Item { self.get() }
}

unsafe impl TrustedRadium for core::sync::atomic::AtomicBool {
    const IS_LOCAL: bool = false;
    unsafe fn load_unchecked(&self) -> Self::Item {
        core::ptr::read(self as *const core::sync::atomic::AtomicBool as *const bool)
    }
}

unsafe impl TrustedRadium for core::cell::Cell<usize> {
    const IS_LOCAL: bool = true;
    unsafe fn load_unchecked(&self) -> Self::Item { self.get() }
}

unsafe impl TrustedRadium for core::sync::atomic::AtomicUsize {
    const IS_LOCAL: bool = false;
    unsafe fn load_unchecked(&self) -> Self::Item {
        core::ptr::read(self as *const core::sync::atomic::AtomicUsize as *const usize)
    }
}

pub type BufferRefData<BR> = BufferData<
    <BR as BufferRef>::Whitch,
    <BR as BufferRef>::Strategy,
    <BR as BufferRef>::Buffer,
    <BR as BufferRef>::Extra,
>;

type ReaderTag<BR> = <<BR as BufferRef>::Strategy as Strategy>::ReaderTag;
type WriterTag<BR> = <<BR as BufferRef>::Strategy as Strategy>::WriterTag;
type Capture<BR> = <<BR as BufferRef>::Strategy as Strategy>::Capture;

pub unsafe trait BufferRef: Sized {
    type Whitch: TrustedRadium<Item = bool>;
    type Buffer;
    type Strategy: Strategy;
    type Extra: ?Sized;
    type UpgradeError: core::fmt::Debug;

    type Strong: Clone + Deref<Target = BufferRefData<Self>>;
    type Weak: Clone;

    fn split(self) -> (Pin<Self::Strong>, Self::Weak);

    fn is_dangling(weak: &Self::Weak) -> bool;

    fn upgrade(weak: &Self::Weak) -> Result<Pin<Self::Strong>, Self::UpgradeError>;

    fn downgrade(strong: &Pin<Self::Strong>) -> Self::Weak;
}

pub unsafe trait Strategy: Sized {
    type ReaderTag;
    type WriterTag: Copy;
    type Capture;
    type RawGuard;

    unsafe fn reader_tag(&self) -> Self::ReaderTag;

    unsafe fn writer_tag(&self) -> Self::WriterTag;

    fn fence(&self, tag: Self::WriterTag);

    fn capture_readers(&self, tag: Self::WriterTag) -> Self::Capture;

    fn is_capture_complete(&self, capture: &mut Self::Capture, tag: Self::WriterTag) -> bool;

    fn begin_guard(&self, tag: &mut Self::ReaderTag) -> Self::RawGuard;

    fn end_guard(&self, guard: Self::RawGuard);

    fn is_swap_completed<B: BufferRef<Strategy = Self>>(&self, swap: &mut raw::Swap<B>) -> bool {
        raw::is_swap_completed(self, swap)
    }
}
