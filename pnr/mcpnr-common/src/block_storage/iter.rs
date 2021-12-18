use super::{BlockStorage, BlockTypeIndex};

pub struct BlockIndexIter<'a> {
    inner: std::slice::Iter<'a, u32>,
}

impl<'a> BlockIndexIter<'a> {
    pub(super) fn new(parent: &'a BlockStorage) -> Self {
        Self {
            inner: parent.blocks.iter(),
        }
    }
}

impl<'a> Iterator for BlockIndexIter<'a> {
    type Item = BlockTypeIndex;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|x| BlockTypeIndex(*x))
    }
}

pub struct BlockCoordIter<'a> {
    parent: &'a BlockStorage,
    inner: std::iter::Enumerate<std::slice::Iter<'a, u32>>,
}

impl<'a> BlockCoordIter<'a> {
    pub(super) fn new(parent: &'a BlockStorage) -> Self {
        Self {
            parent,
            inner: parent.blocks.iter().enumerate(),
        }
    }
}

impl<'a> Iterator for BlockCoordIter<'a> {
    type Item = ((u32, u32, u32), BlockTypeIndex);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(i, v)| {
            // Avoid branches and save some storage by recomputing the indicies instead of
            // explicitly incrementing them. No idea if this is actually a good perf tradeoff
            let i = i as u32;
            let x = i % self.parent.extents[0];
            let z = (i / self.parent.zsi) % self.parent.extents[2];
            let y = (i / self.parent.ysi) % self.parent.extents[1];
            ((x, y, z), BlockTypeIndex(*v))
        })
    }
}

pub struct BlockCoordMutIter<'a> {
    zsi: u32,
    ysi: u32,
    inner: std::iter::Enumerate<std::slice::IterMut<'a, u32>>,
}

impl<'a> BlockCoordMutIter<'a> {
    pub(super) fn new(parent: &'a mut BlockStorage) -> Self {
        Self {
            zsi: parent.zsi,
            ysi: parent.ysi,
            inner: parent.blocks.iter_mut().enumerate(),
        }
    }
}

impl<'a> Iterator for BlockCoordMutIter<'a> {
    type Item = ((u32, u32, u32), &'a mut BlockTypeIndex);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(i, v)| {
            // Avoid branches and save some storage by recomputing the indicies instead of
            // explicitly incrementing them. No idea if this is actually a good perf tradeoff
            let i = i as u32;
            let x = i % self.zsi;
            let z = (i % self.ysi) / self.zsi;
            let y = i / self.ysi;
            // transmute from &'a mut i32 to &'a mut BlockTypeIndex is safe due to
            // repr(transparent) on BlockTypeIndex
            let bti: &mut BlockTypeIndex = unsafe { std::mem::transmute(v) };
            ((x, y, z), bti)
        })
    }
}
