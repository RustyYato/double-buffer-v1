use crate::op::Operation;
use std::{collections::HashMap, hash::Hash, rc::Rc};

pub enum MapOp<K, V> {
    Insert(K, V),
    Remove(K),
    Clear,
}

impl<K: Clone + Hash + Eq, V: Clone> Operation<HashMap<K, V>> for MapOp<K, V> {
    fn apply(&mut self, buffer: &mut HashMap<K, V>) {
        match self {
            MapOp::Insert(k, v) => {
                buffer.insert(k.clone(), v.clone());
            }
            MapOp::Remove(k) => {
                buffer.remove(k);
            }
            MapOp::Clear => buffer.clear(),
        }
    }

    fn apply_once(self, buffer: &mut HashMap<K, V>) {
        match self {
            MapOp::Insert(k, v) => {
                buffer.insert(k, v);
            }
            MapOp::Remove(k) => {
                buffer.remove(&k);
            }
            MapOp::Clear => buffer.clear(),
        }
    }
}

#[test]
fn map_ops() {
    let buffer_data = Rc::new(crate::local::BufferData::<_, ()>::default());
    let (mut r, w) = crate::new(buffer_data);
    let mut w = crate::op::Writer::from(w);

    w.apply(MapOp::Insert(0, "hello"));
    w.apply(MapOp::Insert(1, "world"));

    assert_eq!(r.get().len(), 0);

    w.flush();

    w.apply(MapOp::Remove(0));
    w.apply(MapOp::Insert(2, "!"));

    assert_eq!(r.get().len(), 2);
    assert_eq!(r.get().get(&0), Some(&"hello"));
    assert_eq!(r.get().get(&1), Some(&"world"));

    w.flush();

    w.apply(MapOp::Clear);

    assert_eq!(r.get().len(), 2);
    assert_eq!(r.get().get(&0), None);
    assert_eq!(r.get().get(&1), Some(&"world"));
    assert_eq!(r.get().get(&2), Some(&"!"));

    w.flush();

    assert_eq!(r.get().len(), 0);
}
