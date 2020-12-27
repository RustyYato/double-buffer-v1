use super::{BufferData, BufferRef, BufferRefData, Strategy, TrustedRadium};
#[cfg(feature = "alloc")]
use crate::thin::{Thin, ThinInner};
use core::convert::Infallible;

#[cfg(feature = "alloc")]
use std::{
    boxed::Box,
    rc::{self, Rc},
    sync::{self, Arc},
};

unsafe impl<'a, B, S, E> BufferRef for &'a mut BufferData<S::Whitch, S, B, E>
where
    S: Strategy,
    E: ?Sized,
{
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = Infallible;

    type Strong = &'a BufferRefData<Self>;
    type Weak = &'a BufferRefData<Self>;

    fn split(self) -> (Self::Strong, Self::Weak) { (self, self) }

    fn is_dangling(_: &Self::Weak) -> bool { false }

    fn upgrade(weak: &Self::Weak) -> Result<Self::Strong, Self::UpgradeError> { Ok(*weak) }

    fn downgrade(strong: &Self::Strong) -> Self::Weak { *strong }
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
unsafe impl<B, S, E> BufferRef for Rc<BufferData<S::Whitch, S, B, E>>
where
    S: Strategy,
    E: ?Sized,
{
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = UpgradeFailed;

    type Strong = Rc<BufferRefData<Self>>;
    type Weak = rc::Weak<BufferRefData<Self>>;

    fn split(mut self) -> (Self::Strong, Self::Weak) {
        assert!(Rc::get_mut(&mut self).is_some(), "Tried to split a shared `Rc`!");
        let weak = Rc::downgrade(&self);
        (self, weak)
    }

    fn is_dangling(weak: &Self::Weak) -> bool { rc::Weak::strong_count(&weak) == 0 }

    fn upgrade(weak: &Self::Weak) -> Result<Self::Strong, Self::UpgradeError> { weak.upgrade().ok_or(UpgradeFailed) }

    fn downgrade(strong: &Self::Strong) -> Self::Weak { Rc::downgrade(strong) }
}

#[cfg(feature = "alloc")]
unsafe impl<B, S, E, C> BufferRef for Box<ThinInner<BufferData<S::Whitch, S, B, E>, C>>
where
    C: TrustedRadium<Item = usize>,
    S: Strategy,
    E: ?Sized,
{
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = Infallible;

    type Strong = Thin<BufferRefData<Self>, C>;
    type Weak = Thin<BufferRefData<Self>, C>;

    fn split(self) -> (Self::Strong, Self::Weak) {
        let this = Thin::from(self);
        (Thin::clone(&this), this)
    }

    fn is_dangling(_: &Self::Weak) -> bool { false }

    fn upgrade(weak: &Self::Weak) -> Result<Self::Strong, Self::UpgradeError> { Ok(Thin::clone(weak)) }

    fn downgrade(strong: &Self::Strong) -> Self::Weak { strong.clone() }
}

#[cfg(feature = "alloc")]
unsafe impl<B, S, E> BufferRef for Arc<BufferData<S::Whitch, S, B, E>>
where
    S: Strategy,
    E: ?Sized,
{
    type Buffer = B;
    type Strategy = S;
    type Extra = E;
    type UpgradeError = UpgradeFailed;

    type Strong = Arc<BufferRefData<Self>>;
    type Weak = sync::Weak<BufferRefData<Self>>;

    fn split(mut self) -> (Self::Strong, Self::Weak) {
        assert!(Arc::get_mut(&mut self).is_some(), "Tried to split a shared `Arc`!");
        let weak = Arc::downgrade(&self);
        (self, weak)
    }

    fn is_dangling(weak: &Self::Weak) -> bool { sync::Weak::strong_count(&weak) == 0 }

    fn upgrade(weak: &Self::Weak) -> Result<Self::Strong, Self::UpgradeError> { weak.upgrade().ok_or(UpgradeFailed) }

    fn downgrade(strong: &Self::Strong) -> Self::Weak { Arc::downgrade(strong) }
}
