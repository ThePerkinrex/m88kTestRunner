#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunksIter<I>
where
    I: Iterator,
{
    it: I,
    n: usize,
}

impl<I> Iterator for ChunksIter<I>
where
    I: Iterator,
{
    type Item = Vec<I::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut acc = Vec::with_capacity(self.n);
        for (_, e) in (0..self.n).zip(&mut self.it) {
            acc.push(e);
        }

        if acc.is_empty() {
            None
        } else {
            Some(acc)
        }
    }
}

pub trait IteratorExt: Iterator + Sized {
    fn chunks(self, n: usize) -> ChunksIter<Self>;
}

impl<I> IteratorExt for I
where
    I: Iterator,
{
    fn chunks(self, n: usize) -> ChunksIter<Self> {
        ChunksIter { it: self, n }
    }
}
