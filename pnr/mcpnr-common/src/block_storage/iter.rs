use super::{BlockStorage, BlockTypeIndex};

pub struct BlockIndexIter<'a> {
    inner: std::slice::Iter<'a, u32>,
}

impl<'a> BlockIndexIter<'a> {
    pub(super) fn new(parent: &'a BlockStorage) -> Self {
        Self {
            inner: parent.blocks.iter()
        }
    }
}

impl<'a> Iterator for BlockIndexIter<'a> {
    type Item = BlockTypeIndex;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|x| BlockTypeIndex(*x))
    }
}
