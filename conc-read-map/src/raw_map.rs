use super::{RawMap, RawMapAccess, RawMapWithCapacity};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    hash::Hash,
};

impl<K: Hash + Eq, V> RawMap for HashMap<K, V> {
    type Key = K;
    type Value = V;

    fn new() -> (Self, Self) {
        let a = Self::new();
        let b = Self::with_hasher(a.hasher().clone());

        (a, b)
    }

    fn len(&self) -> usize { self.len() }

    fn clear(&mut self) { self.clear() }

    fn is_empty(&self) -> bool { self.is_empty() }

    fn insert(&mut self, key: Self::Key, value: Self::Value) { self.insert(key, value); }

    fn reserve(&mut self, cap: usize) { self.reserve(cap) }

    fn remove(&mut self, key: &Self::Key) { self.remove(key); }
}

impl<K: Hash + Eq, V> RawMapWithCapacity for HashMap<K, V> {
    fn with_capacity(cap: usize) -> (Self, Self) {
        let a = Self::with_capacity(cap);
        let b = Self::with_capacity_and_hasher(cap, a.hasher().clone());

        (a, b)
    }
}

impl<K: Hash + Eq, V, Q: ?Sized> RawMapAccess<Q> for HashMap<K, V>
where
    K: Borrow<Q>,
    Q: Hash + Eq,
    Self: Clone,
{
    fn get(&self, key: &Q) -> Option<&Self::Value> { self.get(key) }
}

impl<K: Ord + Eq, V> RawMap for BTreeMap<K, V> {
    type Key = K;
    type Value = V;

    fn new() -> (Self, Self) {
        let a = Self::new();
        let b = Self::new();

        (a, b)
    }

    fn len(&self) -> usize { self.len() }

    fn is_empty(&self) -> bool { self.is_empty() }

    fn clear(&mut self) { self.clear() }

    fn insert(&mut self, key: Self::Key, value: Self::Value) { self.insert(key, value); }

    fn remove(&mut self, key: &Self::Key) { self.remove(key); }

    fn reserve(&mut self, _: usize) {}
}

impl<K: Ord + Eq, V, Q: ?Sized> RawMapAccess<Q> for BTreeMap<K, V>
where
    K: Borrow<Q>,
    Q: Ord + Eq,
    Self: Clone,
{
    fn get(&self, key: &Q) -> Option<&Self::Value> { self.get(key) }
}
