use crate::blocking as raw;

pub trait Operation<B>: Sized {
    fn apply(&mut self, buffer: &mut B);
    #[inline]
    fn into_apply(mut self, buffer: &mut B) { self.apply(buffer) }
}

pub struct Write<B, O> {
    write: raw::Write<B>,
    ops: Vec<O>,
}

pub struct WriteRef<'a, B, O> {
    buffer: &'a mut B,
    ops: &'a mut Vec<O>,
}

#[inline]
pub fn new_op_writer_with<B, O>(front: B, back: B) -> (Write<B, O>, raw::Read<B>) {
    let (write, r) = raw::Buffers::new(front, back).split();
    (Write::from(write), r)
}

#[inline]
pub fn new_op_writer<B: Default, O>() -> (Write<B, O>, raw::Read<B>) { new_op_writer_with(B::default(), B::default()) }

impl<B, O> From<raw::Write<B>> for Write<B, O> {
    #[inline]
    fn from(write: raw::Write<B>) -> Self { Write { write, ops: Vec::new() } }
}

impl<B, O> Write<B, O> {
    pub fn reader(&self) -> raw::Read<B> { self.write.reader() }

    pub fn read(&self) -> &B { self.write.read() }

    #[inline]
    fn as_ref(&mut self) -> WriteRef<'_, B, O> {
        WriteRef {
            buffer: &mut self.write,
            ops: &mut self.ops,
        }
    }
}

impl<B, O: Operation<B>> Write<B, O> {
    #[inline]
    pub fn split(&mut self) -> (&B, WriteRef<'_, B, O>) {
        let (read, write, ()) = self.write.split();
        (read, WriteRef {
            buffer: write,
            ops: &mut self.ops,
        })
    }

    #[inline]
    pub fn apply(&mut self, op: O) { self.as_ref().apply(op); }

    #[inline]
    pub fn apply_all<I: IntoIterator<Item = O>>(&mut self, ops: I) { self.as_ref().apply_all(ops); }

    #[cold]
    fn flush_slow(&mut self) {
        self.write.swap_buffers();
        let buffer = &mut self.write as &mut B;
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

impl<B, O: Operation<B>> WriteRef<'_, B, O> {
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
    pub fn by_ref(&mut self) -> WriteRef<'_, B, O> {
        WriteRef {
            buffer: self.buffer,
            ops: self.ops,
        }
    }
}
