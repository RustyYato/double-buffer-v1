#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc as std;

pub mod local;
pub mod raw;

#[cfg(feature = "parking_lot")]
pub mod blocking;
#[cfg(feature = "parking_lot")]
#[forbid(unsafe_code)]
pub mod op;

mod backoff;
mod thin;

#[cfg(test)]
mod tests;

pub struct RawReaderGuard<'reader, T: ?Sized, TagGuard> {
    value: &'reader T,
    tag_guard: TagGuard,
}

impl<'a, T: ?Sized, TagGuard> RawReaderGuard<'a, T, TagGuard> {
    #[inline]
    pub fn tag_guard(this: &Self) -> &TagGuard { &this.tag_guard }

    pub unsafe fn map_tag_guard<NewTagGuard>(
        this: Self,
        f: impl FnOnce(TagGuard) -> NewTagGuard,
    ) -> RawReaderGuard<'a, T, NewTagGuard> {
        RawReaderGuard {
            value: this.value,
            tag_guard: f(this.tag_guard),
        }
    }

    #[inline]
    pub fn map<F, U: ?Sized>(this: Self, f: F) -> RawReaderGuard<'a, U, TagGuard>
    where
        F: for<'val> FnOnce(&'val T, &TagGuard) -> &'val U,
    {
        RawReaderGuard {
            value: f(this.value, Self::tag_guard(&this)),
            tag_guard: this.tag_guard,
        }
    }

    #[inline]
    pub fn try_map<F, U: ?Sized>(this: Self, f: F) -> Result<RawReaderGuard<'a, U, TagGuard>, Self>
    where
        F: for<'val> FnOnce(&'val T, &TagGuard) -> Option<&'val U>,
    {
        match f(this.value, Self::tag_guard(&this)) {
            None => Err(this),
            Some(value) => Ok(RawReaderGuard {
                value,
                tag_guard: this.tag_guard,
            }),
        }
    }

    #[inline]
    pub fn try_map_res<F, U: ?Sized, E>(this: Self, f: F) -> Result<RawReaderGuard<'a, U, TagGuard>, (Self, E)>
    where
        F: for<'val> FnOnce(&'val T, &TagGuard) -> Result<&'val U, E>,
    {
        match f(this.value, Self::tag_guard(&this)) {
            Err(e) => Err((this, e)),
            Ok(value) => Ok(RawReaderGuard {
                value,
                tag_guard: this.tag_guard,
            }),
        }
    }
}

impl<T: ?Sized, TagGuard> core::ops::Deref for RawReaderGuard<'_, T, TagGuard> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target { self.value }
}
