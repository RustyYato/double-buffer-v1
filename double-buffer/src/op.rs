use crate::blocking as raw;

pub trait Operation<B>: Sized {
    fn apply(&mut self, buffer: &mut B);
    #[inline]
    fn into_apply(mut self, buffer: &mut B) { self.apply(buffer) }
}

pub struct Writer<B, O> {
    writer: raw::Writer<B>,
    ops: Vec<O>,
}

pub struct WriterRef<'a, B, O> {
    buffer: &'a mut B,
    ops: &'a mut Vec<O>,
}

#[inline]
pub fn new_op_writer_with<B, O>(front: B, back: B) -> (raw::Reader<B>, Writer<B, O>) {
    let (reader, writer) = raw::Buffers::new(front, back).split();
    (reader, Writer::from(writer))
}

#[inline]
pub fn new_op_writer<B: Default, O>() -> (raw::Reader<B>, Writer<B, O>) {
    new_op_writer_with(B::default(), B::default())
}

impl<B, O> From<raw::Writer<B>> for Writer<B, O> {
    #[inline]
    fn from(writer: raw::Writer<B>) -> Self {
        Writer {
            writer,
            ops: Vec::new(),
        }
    }
}

impl<B, O> Writer<B, O> {
    pub fn reader(&self) -> raw::Reader<B> { self.writer.reader() }

    pub fn read(&self) -> &B { self.writer.read() }

    #[inline]
    fn as_ref(&mut self) -> WriterRef<'_, B, O> {
        WriterRef {
            buffer: &mut self.writer,
            ops: &mut self.ops,
        }
    }
}

impl<B, O: Operation<B>> Writer<B, O> {
    #[inline]
    pub fn split(&mut self) -> (&B, WriterRef<'_, B, O>) {
        let (reader, writer, ()) = self.writer.split();
        (reader, WriterRef {
            buffer: writer,
            ops: &mut self.ops,
        })
    }

    #[inline]
    pub fn apply(&mut self, op: O) { self.as_ref().apply(op); }

    #[inline]
    pub fn apply_all<I: IntoIterator<Item = O>>(&mut self, ops: I) { self.as_ref().apply_all(ops); }

    #[cold]
    fn flush_slow(&mut self) {
        self.writer.swap_buffers();
        let buffer = &mut self.writer as &mut B;
        self.ops.drain(..).for_each(|op| op.into_apply(buffer))
    }

    #[inline]
    pub fn flush(&mut self) {
        if !self.ops.is_empty() {
            self.flush_slow();
        }
    }

    #[inline]
    pub fn operations(&self) -> &[O] { &self.ops }
}

impl<B, O: Operation<B>> WriterRef<'_, B, O> {
    #[inline]
    pub fn apply(&mut self, mut op: O) {
        op.apply(self.buffer);
        self.ops.push(op);
    }

    #[inline]
    pub fn apply_all<I: IntoIterator<Item = O>>(&mut self, ops: I) {
        let buf: &mut B = self.buffer;
        self.ops.extend(ops.into_iter().map(|mut op| {
            op.apply(buf);
            op
        }));
    }

    #[inline]
    pub fn operations(&self) -> &[O] { &self.ops }

    #[inline]
    pub fn by_ref(&mut self) -> WriterRef<'_, B, O> {
        WriterRef {
            buffer: self.buffer,
            ops: self.ops,
        }
    }
}
