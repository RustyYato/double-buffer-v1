use super::{BufferData, BufferRef, BufferRefData, Strategy, TrustedRadium};
#[cfg(feature = "alloc")]
use crate::thin::{Thin, ThinInner};
use core::{convert::Infallible, pin::Pin};

#[cfg(feature = "alloc")]
use std::{
    boxed::Box,
    rc::{self, Rc},
    sync::{self, Arc},
};

unsafe impl<'a, R, B, S, E> BufferRef for Pin<&'a mut BufferData<R, S, B, E>>
where
    R: TrustedRadium<Item = bool>,
    S: Strategy,
    E: ?Sized,
{
    type Whitch = R;
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = Infallible;

    type Strong = &'a BufferRefData<Self>;
    type Weak = Pin<&'a BufferRefData<Self>>;

    fn split(self) -> (Pin<Self::Strong>, Self::Weak) {
        let buffer_data = self.into_ref();
        (buffer_data, buffer_data)
    }

    fn is_dangling(_: &Self::Weak) -> bool { false }

    fn upgrade(weak: &Self::Weak) -> Result<Pin<Self::Strong>, Self::UpgradeError> { Ok(*weak) }

    fn downgrade(strong: &Pin<Self::Strong>) -> Self::Weak { *strong }
}

#[derive(Debug)]
#[cfg(feature = "alloc")]
pub struct UpgradeFailed;

#[cfg(feature = "alloc")]
pub struct PinnedRcWeak<T: ?Sized>(rc::Weak<T>);
#[cfg(feature = "alloc")]
impl<T: ?Sized> Clone for PinnedRcWeak<T> {
    fn clone(&self) -> Self { Self(self.0.clone()) }
}

#[cfg(feature = "alloc")]
unsafe impl<R, B, S, E> BufferRef for Pin<Rc<BufferData<R, S, B, E>>>
where
    R: TrustedRadium<Item = bool>,
    S: Strategy,
    E: ?Sized,
{
    type Whitch = R;
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = UpgradeFailed;

    type Strong = Rc<BufferRefData<Self>>;
    type Weak = PinnedRcWeak<BufferRefData<Self>>;

    fn split(self) -> (Pin<Self::Strong>, Self::Weak) {
        unsafe {
            let mut this = Pin::into_inner_unchecked(self);
            assert!(Rc::get_mut(&mut this).is_some(), "Tried to split a shared `Rc`!");
            let weak = Rc::downgrade(&this);
            (Pin::new_unchecked(this), PinnedRcWeak(weak))
        }
    }

    fn is_dangling(weak: &Self::Weak) -> bool { rc::Weak::strong_count(&weak.0) == 0 }

    fn upgrade(weak: &Self::Weak) -> Result<Pin<Self::Strong>, Self::UpgradeError> {
        Ok(unsafe { Pin::new_unchecked(weak.0.upgrade().ok_or(UpgradeFailed)?) })
    }

    fn downgrade(strong: &Pin<Self::Strong>) -> Self::Weak {
        let strong = unsafe { core::mem::transmute::<&Pin<Self::Strong>, &Self::Strong>(strong) };
        PinnedRcWeak(Rc::downgrade(strong))
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R, B, S, E, C> BufferRef for Box<ThinInner<BufferData<R, S, B, E>, C>>
where
    R: TrustedRadium<Item = bool>,
    C: TrustedRadium<Item = usize>,
    S: Strategy,
    E: ?Sized,
{
    type Whitch = R;
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = Infallible;

    type Strong = Thin<BufferRefData<Self>, C>;
    type Weak = Pin<Thin<BufferRefData<Self>, C>>;

    fn split(self) -> (Pin<Self::Strong>, Self::Weak) {
        unsafe {
            let this = Thin::from(self);
            (Pin::new_unchecked(Thin::clone(&this)), Pin::new_unchecked(this))
        }
    }

    fn is_dangling(_: &Self::Weak) -> bool { false }

    fn upgrade(weak: &Self::Weak) -> Result<Pin<Self::Strong>, Self::UpgradeError> { Ok(Pin::clone(weak)) }

    fn downgrade(strong: &Pin<Self::Strong>) -> Self::Weak { strong.clone() }
}

#[cfg(feature = "alloc")]
pub struct PinnedArcWeak<T: ?Sized>(sync::Weak<T>);
#[cfg(feature = "alloc")]
impl<T: ?Sized> Clone for PinnedArcWeak<T> {
    fn clone(&self) -> Self { Self(self.0.clone()) }
}

#[cfg(feature = "alloc")]
unsafe impl<R, B, S, E> BufferRef for Pin<Arc<BufferData<R, S, B, E>>>
where
    R: TrustedRadium<Item = bool>,
    S: Strategy,
    E: ?Sized,
{
    type Whitch = R;
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = UpgradeFailed;

    type Strong = Arc<BufferRefData<Self>>;
    type Weak = PinnedArcWeak<BufferRefData<Self>>;

    fn split(self) -> (Pin<Self::Strong>, Self::Weak) {
        unsafe {
            let mut this = Pin::into_inner_unchecked(self);
            assert!(Arc::get_mut(&mut this).is_some(), "Tried to split a shared `Arc`!");
            let weak = Arc::downgrade(&this);
            (Pin::new_unchecked(this), PinnedArcWeak(weak))
        }
    }

    fn is_dangling(weak: &Self::Weak) -> bool { sync::Weak::strong_count(&weak.0) == 0 }

    fn upgrade(weak: &Self::Weak) -> Result<Pin<Self::Strong>, Self::UpgradeError> {
        Ok(unsafe { Pin::new_unchecked(weak.0.upgrade().ok_or(UpgradeFailed)?) })
    }

    fn downgrade(strong: &Pin<Self::Strong>) -> Self::Weak {
        let strong = unsafe { core::mem::transmute::<&Pin<Self::Strong>, &Self::Strong>(strong) };
        PinnedArcWeak(Arc::downgrade(strong))
    }
}
