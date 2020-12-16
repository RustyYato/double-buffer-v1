#![forbid(unsafe_code)]

use std::{borrow::Cow, sync::Arc};

use sync_wrapper::SyncWrapper;
extern crate double_buffer as db;

mod raw_map;

pub trait RawMap: Sized {
    type Key;
    type Value;

    fn new() -> (Self, Self);

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool { self.len() == 0 }

    fn clear(&mut self);

    fn insert(&mut self, key: Self::Key, value: Self::Value);

    fn reserve(&mut self, cap: usize);

    fn remove(&mut self, key: &Self::Key);
}

pub trait RawMapRetain: RawMap {
    fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&Self::Key, &mut Self::Value) -> bool;
}

pub trait RawMapWithCapacity: RawMap {
    fn with_capacity(cap: usize) -> (Self, Self);
}

pub trait RawMapAccess<Q: ?Sized>: RawMap {
    fn get(&self, key: &Q) -> Option<&Self::Value>;
}

pub struct Write<'env, M, K, V> {
    map: db::op::Writer<db::sync::park::owned::BufferRef<M>, MapOps<'env, M, K, V>>,
}

pub struct Read<M> {
    map: db::sync::park::owned::Reader<M>,
}

pub enum MapOps<'env, M, K, V> {
    Insert(K, V),
    Remove(K),
    Clear,
    Reserve(usize),
    Call(Call<'env, M>),
}

impl<M> Clone for Read<M> {
    fn clone(&self) -> Self {
        Read {
            map: self.map.try_clone().expect("Tried to clone dangling map"),
        }
    }
}

pub type WriteMap<'env, M> = Write<'env, M, <M as RawMap>::Key, <M as RawMap>::Value>;
pub fn new<'env, M: RawMap>() -> (Read<M>, WriteMap<'env, M>) {
    let (a, b) = M::new();
    let (r, w) = db::new(Arc::pin(db::BufferData::new(a, b)));
    let w = db::op::Writer::from(w);

    (Read { map: r }, Write { map: w })
}

impl<M: RawMap> db::op::Operation<M> for MapOps<'_, M, M::Key, M::Value>
where
    M::Key: Clone,
    M::Value: Clone,
{
    fn apply(&mut self, map: &mut M) {
        match self {
            MapOps::Insert(key, value) => map.insert(key.clone(), value.clone()),
            MapOps::Remove(key) => map.remove(key),
            MapOps::Clear => map.clear(),
            &mut MapOps::Reserve(additional) => map.reserve(additional),
            MapOps::Call(Call(ref mut call)) => call.get_mut()(map, Order::First),
        }
    }

    fn apply_once(self, map: &mut M) {
        match self {
            MapOps::Insert(key, value) => map.insert(key, value),
            MapOps::Remove(key) => map.remove(&key),
            MapOps::Clear => map.clear(),
            MapOps::Reserve(additional) => map.reserve(additional),
            MapOps::Call(Call(mut call)) => call.get_mut()(map, Order::Second),
        }
    }
}

pub struct Call<'env, M>(SyncWrapper<Box<dyn 'env + FnMut(&mut M, Order) + Send>>);

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum Order {
    First,
    Second,
}

impl Order {
    #[inline]
    pub fn is_first(self) -> bool {
        match self {
            Order::First => true,
            Order::Second => false,
        }
    }

    #[inline]
    pub fn is_second(self) -> bool {
        match self {
            Order::First => false,
            Order::Second => true,
        }
    }
}

impl<'env, M: RawMap> WriteMap<'env, M>
where
    MapOps<'env, M, M::Key, M::Value>: db::op::Operation<M>,
{
    pub fn flush(&mut self) { self.map.flush() }

    pub fn reserve(&mut self, additional: usize) -> &mut Self {
        self.map.apply(MapOps::Reserve(additional));
        self
    }

    pub fn insert(&mut self, key: M::Key, value: M::Value) -> &mut Self {
        self.map.apply(MapOps::Insert(key, value));
        self
    }

    pub fn remove<'a>(&mut self, key: impl Into<Cow<'a, M::Key>>) -> &mut Self
    where
        M::Key: 'a + Clone,
    {
        self.map.apply(MapOps::Remove(key.into().into_owned()));
        self
    }

    pub fn clear(&mut self) -> &mut Self {
        self.map.apply(MapOps::Clear);
        self
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: 'env + Send + FnMut(&M::Key, &mut M::Value, Order) -> bool,
        M: RawMapRetain,
    {
        self.map
            .apply(MapOps::Call(Call(SyncWrapper::new(Box::new(move |map, order| {
                map.retain(|key, value| f(key, value, order))
            })))))
    }
}

impl<M> Read<M> {
    pub fn get_map(&mut self) -> db::sync::park::owned::ReaderGuard<'_, M> { self.map.get() }

    pub fn get<Q>(&mut self, key: &Q) -> Option<db::sync::park::owned::ReaderGuard<'_, M, M::Value>>
    where
        Q: ?Sized,
        M: RawMapAccess<Q>,
    {
        db::ReaderGuard::try_map(self.get_map(), move |x, _| x.get(key)).ok()
    }
}

impl<M: RawMap> Read<M> {
    pub fn len(&mut self) -> usize { self.get_map().len() }

    pub fn is_empty(&mut self) -> bool { self.get_map().is_empty() }
}
