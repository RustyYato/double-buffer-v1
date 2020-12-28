#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! __imp_make_newtype {
    ($strategy:ty, $capture_error:ty, $thin:ident, $($ref_count:tt)*) => {
        #[cfg(feature = "alloc")]
        pub mod owned {
            type BufferRefInternal<B, E = ()> = $($ref_count)*<super::BufferData<B, E>>;
            $crate::__imp_newtype_impl_inner! {
                $strategy, $capture_error, ($crate::buffer_ref::UpgradeFailed)
            }
        }

        #[cfg(feature = "alloc")]
        pub mod thin {
            type BufferRefInternal<B, E = ()> = std::boxed::Box<$crate::thin::$thin<super::BufferData<B, E>>>;
            $crate::__imp_newtype_impl_inner! {
                $strategy, $capture_error, (core::convert::Infallible)
            }
        }

        pub mod reference {
            type BufferRefInternal<'buf_data, B, E = ()> = &'buf_data mut super::BufferData<B, E>;
            $crate::__imp_newtype_impl_inner! {
                $strategy, $capture_error, (core::convert::Infallible), 'buf_data
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __imp_newtype_impl_inner {
    (@clone $strategy:ty, $capture_error:ty, core::convert::Infallible $(, $buf_data:lifetime)?) => {
        impl<$($buf_data,)? B, E: ?Sized> Clone for Reader<$($buf_data, )? B, E> {
            fn clone(&self) -> Self {
                Self(Clone::clone(&self.0))
            }
        }
    };
    (@clone $strategy:ty, $capture_error:ty, $upgrade_error:ty $(, $buf_data:lifetime)?) => {};

    ($strategy:ty, $capture_error:ty, ($($upgrade_error:tt)*) $(, $buf_data:lifetime)?) => {
        pub struct Writer<$($buf_data,)? B, E: ?Sized = ()>(pub $crate::raw::Writer<BufferRef<$($buf_data,)? B, E>>);
        pub struct Reader<$($buf_data,)? B, E: ?Sized = ()>(pub $crate::raw::Reader<BufferRef<$($buf_data,)? B, E>>);
        pub type ReaderGuard<'reader, $($buf_data,)? B, T = B, E = ()> =
            $crate::raw::ReaderGuard<'reader, BufferRef<$($buf_data,)? B, E>, T>;
        pub type BufferRef<$($buf_data,)? B,E = ()> = BufferRefInternal<$($buf_data,)? B, E>;

        pub fn new<$($buf_data,)? B, E: ?Sized>(buffers: BufferRefInternal<$($buf_data,)? B, E>) -> (Reader<$($buf_data,)? B, E>, Writer<$($buf_data,)? B, E>) {
            let (reader, writer) = crate::new(buffers);
            (Reader(reader), Writer(writer))
        }

        impl<$($buf_data,)? B, E: ?Sized> Writer<$($buf_data,)? B, E> {
            pub fn reader(this: &Self) -> Reader<$($buf_data, )? B, E> { Reader($crate::raw::Writer::reader(&this.0)) }
            pub fn read(this: &Self) -> &B { $crate::raw::Writer::read(&this.0) }
            pub fn strategy(this: &Self) -> &$strategy { $crate::raw::Writer::strategy(&this.0) }
            pub fn extra(this: &Self) -> &E { $crate::raw::Writer::extra(&this.0) }
            pub fn split(this: &Self) -> $crate::raw::Split<'_, BufferRef<$($buf_data, )? B, E>> { $crate::raw::Writer::split(&this.0) }
            pub fn split_mut(this: &mut Self) -> $crate::raw::SplitMut<'_, BufferRef<$($buf_data, )? B, E>> {
                $crate::raw::Writer::split_mut(&mut this.0)
            }
            pub fn swap_buffers(this: &mut Self) { $crate::raw::Writer::swap_buffers(&mut this.0) }
            pub fn swap_buffers_with<F: FnMut(&Self)>(this: &mut Self, mut f: F) {
                let f = move |writer: &_| f(unsafe { &*(writer as *const _ as *const Self) });
                $crate::raw::Writer::swap_buffers_with(&mut this.0, f)
            }
            pub unsafe fn swap_buffers_unchecked(this: &mut Self) {
                $crate::raw::Writer::swap_buffers_unchecked(&mut this.0)
            }
            pub unsafe fn start_buffer_swap(this: &mut Self) -> $crate::raw::Swap<BufferRef<$($buf_data, )? B, E>> {
                $crate::raw::Writer::start_buffer_swap(&mut this.0)
            }
            pub unsafe fn try_start_buffer_swap(
                this: &mut Self,
            ) -> Result<$crate::raw::Swap<BufferRef<$($buf_data, )? B, E>>, $capture_error> {
                $crate::raw::Writer::try_start_buffer_swap(&mut this.0)
            }
            pub fn finish_swap(this: &Self, swap: $crate::raw::Swap<BufferRef<$($buf_data, )? B, E>>) {
                $crate::raw::Writer::finish_swap(&this.0, swap)
            }
            pub fn finish_swap_with<F: FnMut()>(this: &Self, swap: $crate::raw::Swap<BufferRef<$($buf_data, )? B, E>>, f: F) {
                $crate::raw::Writer::finish_swap_with(&this.0, swap, f)
            }
        }

        impl<$($buf_data,)? B, E: ?Sized> core::ops::Deref for Writer<$($buf_data,)? B, E> {
            type Target = B;

            fn deref(&self) -> &B {
                &self.0
            }
        }

        impl<$($buf_data,)? B, E: ?Sized> core::ops::DerefMut for Writer<$($buf_data,)? B, E> {
            fn deref_mut(&mut self) -> &mut B {
                &mut self.0
            }
        }

        impl<$($buf_data,)? B, E: ?Sized> Reader<$($buf_data,)? B, E> {
            pub fn try_clone(&self) -> Result<Self, $($upgrade_error)*> { $crate::raw::Reader::try_clone(&self.0).map(Self) }
            pub fn is_dangling(&self) -> bool { $crate::raw::Reader::is_dangling(&self.0) }
            pub fn get(&mut self) -> ReaderGuard<'_, $($buf_data,)? B, B, E> { $crate::raw::Reader::get(&mut self.0) }
            pub fn try_get(&mut self) -> Result<ReaderGuard<'_, $($buf_data,)? B, B, E>, $($upgrade_error)*> { $crate::raw::Reader::try_get(&mut self.0) }
        }

        $crate::__imp_newtype_impl_inner!{@clone $strategy, $capture_error, $($upgrade_error)* $(, $buf_data)?}
    }
}
