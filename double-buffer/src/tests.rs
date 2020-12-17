use test_crossbeam_channel::bounded;
use test_crossbeam_utils::thread::scope;

use std::sync::Arc;

use crate::{new, sync::BufferData, Writer};

#[cfg(feature = "std")]
mod map;

#[test]
pub fn is_dangling() {
    let buffer_data = Arc::new(BufferData::new((), ()));
    let (r, _) = new(buffer_data);

    assert!(r.is_dangling());
}

#[test]
fn clone_read() {
    let buffer_data = Arc::new(BufferData::new((), ()));
    let (r, _w) = new(buffer_data);

    r.try_clone().unwrap();
}

#[test]
fn write_before_read() {
    let buffer_data = Arc::new(BufferData::new(0, 0));
    let (mut r, mut w) = new(buffer_data);

    let buffer = &mut *w;
    *buffer = 20;
    assert_eq!(*r.get(), 0);
    Writer::swap_buffers(&mut w);
    assert_eq!(*r.get(), 20);
    Writer::swap_buffers(&mut w);
    assert_eq!(*r.get(), 0);
}

#[test]
#[ignore = "this test will block forever"]
fn swap_while_read() {
    let buffer_data = Arc::new(BufferData::new(0, 0));
    let (mut r, mut w) = new(buffer_data);

    let _guard = r.get();

    Writer::swap_buffers(&mut w);
}

#[test]
#[cfg_attr(miri, ignore)]
fn wait() {
    let buffer_data = Arc::new(BufferData::new(0, 0));
    let (mut r, mut w) = new(buffer_data);

    let r = &mut r;

    let (tx0, rx0) = bounded(1);
    let (tx1, rx1) = bounded(1);
    let (tx2, rx2) = bounded(1);

    let _ = scope(move |s| {
        let _ = s.spawn(move |_| {
            let _ = tx0.send(());

            let _ = rx1.recv();
            let _r = r.get();
            let _ = tx2.send(());
        });

        let _ = rx0.recv();
        Writer::swap_buffers(&mut w);
        let _ = tx1.send(());
        let _ = rx2.recv();
    });
}

#[test]
#[ignore = "this test will block forever"]
fn blocks() {
    let buffer_data = Arc::new(BufferData::new(0, 0));
    let (mut r, mut w) = new(buffer_data);

    let (tx0, rx0) = bounded(1);
    let (tx1, rx1) = bounded(1);

    let _ = tx0.send(&mut r);

    let _ = scope(move |s| {
        let _ = s.spawn(move |_| {
            let x = rx0.recv().unwrap();
            let _ = tx1.send(x.get());
        });

        let _y = rx1.recv().unwrap();

        Writer::swap_buffers(&mut w);
    });
}
