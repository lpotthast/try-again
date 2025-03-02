use std::marker::PhantomData;

#[derive(Debug)]
pub enum Finite {}

#[derive(Debug)]
pub enum Infinite {}

#[derive(Debug)]
pub enum Unknown {}

pub type FiniteIterator<I> = TrackedIterator<I, Finite>;
#[expect(unused)]
pub type InfiniteIterator<I> = TrackedIterator<I, Infinite>;
#[expect(unused)]
pub type UnknownIterator<I> = TrackedIterator<I, Unknown>;

/// Wrapper for iterators with finiteness tracking.
#[derive(Debug)]
pub struct TrackedIterator<I: Iterator, F> {
    pub(crate) inner: I,
    pub(crate) _marker: PhantomData<F>,
}

impl<I: Iterator, F> Iterator for TrackedIterator<I, F> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Extension trait to convert standard iterators into tracked ones.
pub trait IntoTrackedIterator: Iterator + Sized {
    fn into_tracked(self) -> TrackedIterator<Self, Unknown> {
        TrackedIterator {
            inner: self,
            _marker: PhantomData,
        }
    }
}

impl<I: Iterator> IntoTrackedIterator for I {}

impl<I: Iterator, F> TrackedIterator<I, F> {
    /// Returns an iterator known to be finite!
    pub fn take(self, n: usize) -> TrackedIterator<std::iter::Take<I>, Finite> {
        TrackedIterator {
            inner: self.inner.take(n),
            _marker: PhantomData,
        }
    }

    /// Finiteness-preserving adaptor.
    pub fn map<B, Func>(self, f: Func) -> TrackedIterator<std::iter::Map<I, Func>, F>
    where
        Func: FnMut(I::Item) -> B,
    {
        TrackedIterator {
            inner: self.inner.map(f),
            _marker: PhantomData,
        }
    }

    /// Finiteness-preserving adaptor.
    pub fn filter<Pred>(self, predicate: Pred) -> TrackedIterator<std::iter::Filter<I, Pred>, F>
    where
        Pred: FnMut(&I::Item) -> bool,
    {
        TrackedIterator {
            inner: self.inner.filter(predicate),
            _marker: PhantomData,
        }
    }

    // Add more adapters as needed...
}

/// Iteration over any `Vec` is known to be `Finite`.
impl<T> From<Vec<T>> for TrackedIterator<std::vec::IntoIter<T>, Finite> {
    fn from(vec: Vec<T>) -> Self {
        TrackedIterator {
            inner: vec.into_iter(),
            _marker: PhantomData,
        }
    }
}

/// Iteration over any slice is known to be `Finite`.
impl<'a, T> From<&'a [T]> for TrackedIterator<std::slice::Iter<'a, T>, Finite> {
    fn from(slice: &'a [T]) -> Self {
        TrackedIterator {
            inner: slice.iter(),
            _marker: PhantomData,
        }
    }
}
