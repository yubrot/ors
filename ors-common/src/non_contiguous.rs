use core::convert::TryInto;
use core::fmt::Debug;
use core::iter::FromIterator;
use core::mem;
use core::mem::MaybeUninit;

/// Allocation-free fixed size array with non-contiguous indices.
#[derive(Debug, Clone)]
pub struct Array<I, V, const N: usize> {
    len: usize,
    buckets: [Option<(I, V)>; N],
}

pub trait ArrayIndex: Eq + Copy {
    fn array_index(self) -> usize;
}

impl<T> ArrayIndex for T
where
    T: Eq + Copy + TryInto<usize>,
    <T as TryInto<usize>>::Error: Debug,
{
    fn array_index(self) -> usize {
        self.try_into().unwrap()
    }
}

impl<I: ArrayIndex, V, const N: usize> Array<I, V, N> {
    pub fn new() -> Self {
        // TODO: [const { None }; N] with inline const expressions
        let mut buckets = MaybeUninit::uninit_array();
        for bucket in &mut buckets[..] {
            bucket.write(None);
        }
        Self {
            len: 0,
            buckets: unsafe { MaybeUninit::array_assume_init(buckets) },
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        for bucket in self.buckets.iter_mut() {
            *bucket = None;
        }
        self.len = 0;
    }

    pub fn get(&self, i: I) -> Option<&V> {
        match self.bucket_index(i) {
            Some(BucketIndex::Occupied(index)) => {
                let bucket = self.buckets[index].as_ref();
                Some(&bucket.unwrap().1)
            }
            _ => None,
        }
    }

    pub fn get_mut(&mut self, i: I) -> Option<&mut V> {
        match self.bucket_index(i) {
            Some(BucketIndex::Occupied(index)) => {
                let bucket = self.buckets[index].as_mut();
                Some(&mut bucket.unwrap().1)
            }
            _ => None,
        }
    }

    pub fn insert(&mut self, i: I, v: V) -> Option<V> {
        match self.bucket_index(i).expect("Array is full") {
            BucketIndex::Vacant(index) => {
                self.buckets[index] = Some((i, v));
                self.len += 1;
                None
            }
            BucketIndex::Occupied(index) => {
                let bucket = self.buckets[index].as_mut();
                Some(mem::replace(&mut bucket.unwrap().1, v))
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(I, V)> {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (I, V)> {
        self.into_iter()
    }

    fn bucket_index(&self, i: I) -> Option<BucketIndex> {
        for offset in 0..N {
            let index = (i.array_index() + offset) % N; // open addressing
            match self.buckets[index] {
                None => return Some(BucketIndex::Vacant(index)),
                Some((j, _)) if i == j => return Some(BucketIndex::Occupied(index)),
                Some(_) => {}
            }
        }
        None
    }
}

#[derive(Debug)]
enum BucketIndex {
    Vacant(usize),
    Occupied(usize), // TODO: Support remove operation with Robin Hood Hashing method
}

impl<I: ArrayIndex, V, const N: usize> Default for Array<I, V, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: ArrayIndex, V, const N: usize> FromIterator<(I, V)> for Array<I, V, N> {
    fn from_iter<T: IntoIterator<Item = (I, V)>>(iter: T) -> Self {
        let mut array = Self::new();
        for (i, v) in iter {
            array.insert(i, v);
        }
        array
    }
}

impl<I: ArrayIndex, V, const N: usize> IntoIterator for Array<I, V, N> {
    type Item = (I, V);
    type IntoIter = core::iter::Flatten<<[Option<(I, V)>; N] as IntoIterator>::IntoIter>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self.buckets).flatten()
    }
}

impl<'a, I: ArrayIndex, V, const N: usize> IntoIterator for &'a Array<I, V, N> {
    type Item = &'a (I, V);
    type IntoIter = core::iter::Flatten<core::slice::Iter<'a, Option<(I, V)>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.buckets.iter().flatten()
    }
}

impl<'a, I: ArrayIndex, V, const N: usize> IntoIterator for &'a mut Array<I, V, N> {
    type Item = &'a mut (I, V);
    type IntoIter = core::iter::Flatten<core::slice::IterMut<'a, Option<(I, V)>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.buckets.iter_mut().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec;

    #[test]
    fn test_array() {
        let mut array: Array<u32, i32, 16> = Array::new();
        assert_eq!(array.len(), 0);
        assert_eq!(array.insert(1, 1), None);
        assert_eq!(array.insert(2, 2), None);
        assert_eq!(array.insert(4, 3), None);
        assert_eq!(array.insert(8, 4), None);
        assert_eq!(array.len(), 4);

        assert_eq!(array.get(0), None);
        assert_eq!(array.get(1), Some(&1));
        assert_eq!(array.get(2), Some(&2));
        assert_eq!(array.get(3), None);

        assert_eq!(array.insert(3, 5), None);
        assert_eq!(array.insert(4, 6), Some(3));
        assert_eq!(array.insert(5, 7), None);
        assert_eq!(array.len(), 6);

        assert_eq!(array.insert(17, 8), None);
        assert_eq!(array.insert(18, 9), None);

        assert_eq!(
            array.into_iter().collect::<BTreeMap<u32, i32>>(),
            vec![
                (1, 1),
                (2, 2),
                (3, 5),
                (4, 6),
                (5, 7),
                (8, 4),
                (17, 8),
                (18, 9)
            ]
            .into_iter()
            .collect()
        );
    }
}
