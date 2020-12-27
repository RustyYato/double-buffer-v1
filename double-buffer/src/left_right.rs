use crate::{raw, BufferRef, Strategy};

use std::vec::Vec;

use crate::op::Operation;

pub struct Writer<B: BufferRef, O> {
    writer: raw::Writer<B>,
    ops: Vec<O>,
    applied: usize,
    swap: raw::Swap<B>,
}

pub struct WriterRef<'a, O> {
    ops: &'a mut Vec<O>,
}

pub trait LeftRightStrategy: crate::Seal {}
impl crate::Seal for crate::sync::SyncStrategy {}
impl crate::Seal for crate::sync::park::ParkStrategy {}
impl LeftRightStrategy for crate::sync::SyncStrategy {}
impl LeftRightStrategy for crate::sync::park::ParkStrategy {}

impl<B: BufferRef, O> From<crate::raw::Writer<B>> for Writer<B, O>
where
    B::Strategy: LeftRightStrategy,
{
    #[inline]
    fn from(mut writer: crate::raw::Writer<B>) -> Self {
        let swap = unsafe { crate::raw::Writer::start_buffer_swap(&mut writer) };
        Writer {
            writer,
            ops: Vec::new(),
            swap,
            applied: 0,
        }
    }
}

impl<B: BufferRef, O> Writer<B, O> {
    pub fn reader(&self) -> crate::raw::Reader<B> { crate::raw::Writer::reader(&self.writer) }

    pub fn read(&self) -> &B::Buffer { crate::raw::Writer::read(&self.writer) }

    pub fn extra(&self) -> &B::Extra { crate::raw::Writer::extra(&self.writer) }
}

impl<B: BufferRef, O: Operation<B::Buffer>> Writer<B, O> {
    #[inline]
    pub fn split(&mut self) -> (&B::Buffer, WriterRef<'_, O>, &B::Extra) {
        let split = crate::raw::Writer::split_mut(&mut self.writer);
        (split.read, WriterRef { ops: &mut self.ops }, split.extra)
    }

    #[inline]
    pub fn apply(&mut self, op: O) { self.ops.push(op); }

    pub fn apply_all<I: IntoIterator<Item = O>>(&mut self, ops: I) { self.ops.extend(ops); }

    pub fn flush(&mut self) {
        let strategy = crate::raw::Writer::strategy(&self.writer);
        while !strategy.is_swap_completed(&mut self.swap) {}

        let buffer = &mut *self.writer;
        let ops = self.ops.drain(..self.applied);
        self.applied = 0;
        for op in ops {
            op.apply_once(buffer);
        }

        for op in self.ops.iter_mut() {
            op.apply(buffer);
            self.applied += 1;
        }

        self.swap = unsafe { crate::raw::Writer::start_buffer_swap(&mut self.writer) };
    }

    #[inline]
    pub fn operations(&self) -> &[O] { &self.ops }
}

impl<O> WriterRef<'_, O> {
    #[inline]
    pub fn apply(&mut self, op: O) { self.ops.push(op); }

    #[inline]
    pub fn apply_all<I: IntoIterator<Item = O>>(&mut self, ops: I) { self.ops.extend(ops); }

    #[inline]
    pub fn operations(&self) -> &[O] { &self.ops }
}

struct Counter(i64);

impl crate::op::Operation<Counter> for i64 {
    fn apply(&mut self, buffer: &mut Counter) { buffer.0 += *self; }
}

#[test]
fn left_right() {
    use crate::raw::BufferDataBuilder;

    let mut buffer_data = BufferDataBuilder {
        strategy: crate::sync::SyncStrategy::default(),
        buffers: [Counter(0), Counter(0)],
        extra: (),
    }
    .build();
    let (mut reader, writer) = buffer_data.split_mut();
    let mut writer = Writer::from(writer);

    writer.apply(10);
    writer.apply(20);
    writer.flush();
    writer.apply(-30);
    assert_eq!(reader.get().0, 30);
    writer.flush();
    assert_eq!(reader.get().0, 0);
}
