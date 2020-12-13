use crossbeam_channel::bounded;
use crossbeam_utils::thread::scope;

use crate::raw::Buffers;

#[cfg(feature = "parking_lot")]
mod map;

#[test]
pub fn is_dangling() {
    let (r, _) = Buffers::new((), ()).split();

    assert!(r.is_dangling());
}

#[test]
fn clone_read() {
    let (r, _w) = Buffers::new((), ()).split();

    r.try_clone().unwrap();
}

#[test]
fn write_before_read() {
    let (mut r, mut w) = Buffers::new(0, 0).split();

    let buffer = &mut *w;
    *buffer = 20;
    assert_eq!(*r.get(), 0);
    w.swap_buffers();
    assert_eq!(*r.get(), 20);
    w.swap_buffers();
    assert_eq!(*r.get(), 0);
}

#[test]
#[ignore = "this test will block forever"]
fn swap_while_read() {
    let (mut r, mut w) = Buffers::new(0, 0).split();

    let _guard = r.get();

    w.swap_buffers();
}

#[test]
#[cfg_attr(miri, ignore)]
fn wait() {
    let (mut r, mut w) = Buffers::new(0, 0).split();
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
        w.swap_buffers();
        let _ = tx1.send(());
        let _ = rx2.recv();
    });
}

#[test]
#[ignore = "this test will block forever"]
fn blocks() {
    let (mut r, mut w) = Buffers::new(0, 0).split();

    let (tx0, rx0) = bounded(1);
    let (tx1, rx1) = bounded(1);

    let _ = tx0.send(&mut r);

    let _ = scope(move |s| {
        let _ = s.spawn(move |_| {
            let x = rx0.recv().unwrap();
            let _ = tx1.send(x.get());
        });

        let _y = rx1.recv().unwrap();

        w.swap_buffers();
    });
}
