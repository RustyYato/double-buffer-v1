use crate::{raw, BufferRef, Strategy};

use std::collections::VecDeque;

use crate::op::Operation;

pub struct Writer<B: BufferRef, O> {
    writer: raw::Writer<B>,
    ops: VecDeque<O>,
    applied: usize,
    swap: raw::Swap<B>,
}

pub struct WriterRef<'a, O> {
    ops: &'a mut VecDeque<O>,
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
            ops: VecDeque::new(),
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
    pub fn register(&mut self, op: O) { self.ops.push_back(op); }

    pub fn flush(&mut self) {
        let strategy = crate::raw::Writer::strategy(&self.writer);
        while !strategy.is_swap_completed(&mut self.swap) {}

        let buffer = &mut *self.writer;

        while let Some(applied) = self.applied.checked_sub(1) {
            self.applied = applied;
            match self.ops.pop_front() {
                Some(op) => op.apply_once(buffer),
                None => {
                    self.applied = 0;
                    break
                }
            }
        }

        for op in self.ops.iter_mut() {
            self.applied += 1;
            op.apply(buffer);
        }

        self.swap = unsafe { crate::raw::Writer::start_buffer_swap(&mut self.writer) };
    }

    #[inline]
    pub fn operations(&self) -> &VecDeque<O> { &self.ops }
}

impl<B: BufferRef, O> Extend<O> for Writer<B, O> {
    fn extend<T: IntoIterator<Item = O>>(&mut self, iter: T) { self.ops.extend(iter) }
}

impl<O> Extend<O> for WriterRef<'_, O> {
    fn extend<T: IntoIterator<Item = O>>(&mut self, iter: T) { self.ops.extend(iter) }
}

impl<O> WriterRef<'_, O> {
    #[inline]
    pub fn register(&mut self, op: O) { self.ops.push_back(op); }

    #[inline]
    pub fn operations(&self) -> &VecDeque<O> { &self.ops }
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

    writer.register(10);
    writer.register(20);
    writer.flush();
    writer.register(-30);
    assert_eq!(reader.get().0, 30);
    writer.flush();
    assert_eq!(reader.get().0, 0);
}
