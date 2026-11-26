//! Typed indices. A node, an edge and a line (an input route) each have their own index type
//! ([`NodeId`], [`EdgeId`], [`LineId`]), and a list indexed by one of them ([`IdVec`]) takes only
//! that type, so the compiler stops a node's index from being used where an edge's belongs. Each
//! is a plain `usize` underneath (`#[repr(transparent)]`): the same numbering, the same order, no
//! cost.

use std::fmt;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

/// An index type: a `usize` with its own name ([`NodeId`], [`EdgeId`], [`LineId`]).
pub trait Id: Copy + Eq + Ord + fmt::Debug {
    /// The id of index `i`.
    fn new(i: usize) -> Self;
    /// Its index.
    fn index(self) -> usize;
}

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(transparent)]
        pub struct $name(usize);

        impl $name {
            /// The id of index `i`.
            #[must_use]
            #[inline]
            pub const fn new(i: usize) -> Self {
                Self(i)
            }

            /// Its index.
            #[must_use]
            #[inline]
            pub const fn index(self) -> usize {
                self.0
            }
        }

        impl Id for $name {
            #[inline]
            fn new(i: usize) -> Self {
                Self(i)
            }

            #[inline]
            fn index(self) -> usize {
                self.0
            }
        }

        /// The plain index.
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(
    /// A node of the line graph: an index into [`Topology::nodes`](crate::topology::Topology::nodes).
    NodeId
);
id_type!(
    /// An edge of the line graph: an index into [`Topology::edges`](crate::topology::Topology::edges).
    EdgeId
);
id_type!(
    /// A line: the index of an input route, in the caller's order.
    LineId
);

/// A list indexed by one id type: `IdVec<NodeId, Point>` takes a [`NodeId`] and nothing else. A
/// `Vec` underneath, in the same order: the element at index `i` has id `I::new(i)`.
pub struct IdVec<I, T> {
    raw: Vec<T>,
    id: PhantomData<fn(I) -> I>,
}

impl<I: Id, T> IdVec<I, T> {
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            raw: Vec::new(),
            id: PhantomData,
        }
    }

    /// An empty list with room for `n` elements.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Vec::with_capacity(n).into()
    }

    /// How many elements it holds.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Whether it holds none.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// Adds `value` at the end and returns its id.
    pub fn push(&mut self, value: T) -> I {
        let id = I::new(self.raw.len());
        self.raw.push(value);
        id
    }

    /// The element with id `id`, or None past the end.
    #[must_use]
    #[inline]
    pub fn get(&self, id: I) -> Option<&T> {
        self.raw.get(id.index())
    }

    /// The element with id `id`, mutable, or None past the end.
    #[must_use]
    #[inline]
    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        self.raw.get_mut(id.index())
    }

    /// The elements in order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.raw.iter()
    }

    /// The elements in order, mutable.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.raw.iter_mut()
    }

    /// The elements in order, each with its id.
    pub fn iter_enumerated(
        &self,
    ) -> impl DoubleEndedIterator<Item = (I, &T)> + ExactSizeIterator + Clone {
        self.raw.iter().enumerate().map(|(i, v)| (I::new(i), v))
    }

    /// The elements in order, mutable, each with its id.
    pub fn iter_mut_enumerated(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (I, &mut T)> + ExactSizeIterator {
        self.raw.iter_mut().enumerate().map(|(i, v)| (I::new(i), v))
    }

    /// The elements in order, owned, each with its id.
    pub fn into_iter_enumerated(
        self,
    ) -> impl DoubleEndedIterator<Item = (I, T)> + ExactSizeIterator {
        self.raw
            .into_iter()
            .enumerate()
            .map(|(i, v)| (I::new(i), v))
    }

    /// Every id, in order.
    pub fn ids(
        &self,
    ) -> impl DoubleEndedIterator<Item = I> + ExactSizeIterator + Clone + use<I, T> {
        (0..self.raw.len()).map(I::new)
    }

    /// The last element, or None when empty.
    #[must_use]
    pub fn last(&self) -> Option<&T> {
        self.raw.last()
    }

    /// The elements as a plain slice, in order.
    #[must_use]
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.raw
    }

    /// The elements as a plain `Vec`, in order.
    #[must_use]
    pub fn into_raw(self) -> Vec<T> {
        self.raw
    }
}

impl<I: Id, T> Index<I> for IdVec<I, T> {
    type Output = T;

    /// # Panics
    /// When `id` is past the end, as a `Vec` does; [`IdVec::get`] is the checked form.
    #[inline]
    fn index(&self, id: I) -> &T {
        &self.raw[id.index()]
    }
}

impl<I: Id, T> IndexMut<I> for IdVec<I, T> {
    /// # Panics
    /// When `id` is past the end, as a `Vec` does; [`IdVec::get_mut`] is the checked form.
    #[inline]
    fn index_mut(&mut self, id: I) -> &mut T {
        &mut self.raw[id.index()]
    }
}

impl<I, T> From<Vec<T>> for IdVec<I, T> {
    /// The elements of `raw`, element `i` with id `i`.
    fn from(raw: Vec<T>) -> Self {
        Self {
            raw,
            id: PhantomData,
        }
    }
}

impl<I, T> FromIterator<T> for IdVec<I, T> {
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        iter.into_iter().collect::<Vec<T>>().into()
    }
}

impl<I, T> Extend<T> for IdVec<I, T> {
    fn extend<It: IntoIterator<Item = T>>(&mut self, iter: It) {
        self.raw.extend(iter);
    }
}

impl<I, T> IntoIterator for IdVec<I, T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.into_iter()
    }
}

impl<'a, I, T> IntoIterator for &'a IdVec<I, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter()
    }
}

impl<'a, I, T> IntoIterator for &'a mut IdVec<I, T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.raw.iter_mut()
    }
}

impl<I, T> Default for IdVec<I, T> {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl<I, T: Clone> Clone for IdVec<I, T> {
    fn clone(&self) -> Self {
        self.raw.clone().into()
    }

    fn clone_from(&mut self, source: &Self) {
        self.raw.clone_from(&source.raw);
    }
}

/// Printed as the plain list.
impl<I, T: fmt::Debug> fmt::Debug for IdVec<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.raw.fmt(f)
    }
}

impl<I, T: PartialEq> PartialEq for IdVec<I, T> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<I, T: Eq> Eq for IdVec<I, T> {}

/// Equal to a plain `Vec` with the same elements in the same order.
impl<I, T: PartialEq> PartialEq<Vec<T>> for IdVec<I, T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.raw == *other
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn ids_number_the_elements_in_order() {
        let mut v: IdVec<NodeId, char> = IdVec::new();
        assert!(v.is_empty());
        assert_eq!(v.push('a'), NodeId::new(0));
        assert_eq!(v.push('b'), NodeId::new(1));
        assert_eq!(v[NodeId::new(1)], 'b');
        v[NodeId::new(0)] = 'z';
        assert_eq!(v, vec!['z', 'b']);
        assert_eq!(
            v.ids().collect::<Vec<_>>(),
            [NodeId::new(0), NodeId::new(1)]
        );
        let pairs: Vec<(NodeId, char)> = v.iter_enumerated().map(|(i, &c)| (i, c)).collect();
        assert_eq!(pairs, [(NodeId::new(0), 'z'), (NodeId::new(1), 'b')]);
        assert_eq!(NodeId::new(7).index(), 7);
        assert_eq!(EdgeId::new(3).to_string(), "3");
        assert_eq!(format!("{v:?}"), "['z', 'b']");
    }

    #[test]
    fn an_empty_list_has_no_elements_and_no_ids() {
        let v: IdVec<EdgeId, u8> = IdVec::default();
        assert_eq!(v.len(), 0);
        assert_eq!(v.get(EdgeId::new(0)), None);
        assert_eq!(v.ids().count(), 0);
        assert_eq!(v.iter_enumerated().count(), 0);
        assert_eq!(v.last(), None);
        assert!(v.into_raw().is_empty());
    }

    #[test]
    fn get_past_the_end_is_none_not_a_panic() {
        let mut v: IdVec<LineId, u8> = vec![1, 2].into();
        assert_eq!(v.get(LineId::new(2)), None);
        assert_eq!(v.get(LineId::new(usize::MAX)), None);
        assert_eq!(v.get_mut(LineId::new(5)), None);
        if let Some(x) = v.get_mut(LineId::new(1)) {
            *x = 9;
        }
        assert_eq!(v.as_slice(), [1, 9]);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn indexing_past_the_end_panics_as_a_vec_does() {
        let v: IdVec<LineId, u8> = vec![1].into();
        let _ = v[LineId::new(1)];
    }

    proptest! {
        /// An `IdVec` is its `Vec`: the same elements, order and ids, whichever way it is built.
        #[test]
        fn the_same_elements_in_the_same_order(raw in prop::collection::vec(any::<u16>(), 0..40)) {
            let collected: IdVec<EdgeId, u16> = raw.iter().copied().collect();
            let mut pushed: IdVec<EdgeId, u16> = IdVec::with_capacity(raw.len());
            for (i, &x) in raw.iter().enumerate() {
                prop_assert_eq!(pushed.push(x), EdgeId::new(i));
            }
            let mut extended: IdVec<EdgeId, u16> = IdVec::new();
            extended.extend(raw.iter().copied());
            prop_assert_eq!(&collected, &pushed);
            prop_assert_eq!(&collected, &extended);
            prop_assert_eq!(&collected, &raw);
            for (id, &x) in collected.iter_enumerated() {
                prop_assert_eq!(collected[id], x);
                prop_assert_eq!(raw[id.index()], x);
            }
            prop_assert_eq!(collected.iter().copied().collect::<Vec<_>>(), raw.clone());
            prop_assert_eq!(
                collected.clone().into_iter_enumerated().map(|(i, x)| (i.index(), x)).collect::<Vec<_>>(),
                raw.iter().copied().enumerate().collect::<Vec<_>>()
            );
            prop_assert_eq!(collected.into_raw(), raw);
        }
    }
}
