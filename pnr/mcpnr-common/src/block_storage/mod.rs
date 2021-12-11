//! Types for storing minecraft-format blocks. This is in mcpnr-common so it
//! can be reused by a future simulator.

pub mod iter;
mod serialization;

use std::collections::HashMap;
use std::vec::Vec;

// Should go down
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyValue {
    STR(String),
    BYTE(i8),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub properties: Option<HashMap<String, PropertyValue>>,
}

impl Block {
    pub fn new(name: String) -> Self {
        Self {
            name,
            properties: None,
        }
    }
}

pub struct BlockStorage {
    /// 3D extents. If changing this is required then it must be done through a
    /// proper function call because all the other fields rely on it staying
    /// the same for the lifetime of this BlockStorage
    pub(self) extents: [u32; 3],
    /// Scale to use for Z coordinates when computing indicies
    pub(self) zsi: u32,
    /// Scale to use for X coordinates when computing indicies
    pub(self) ysi: u32,

    pub(self) palette: Vec<Block>,

    /// Only indexes into the palette for now, if tile entity support is
    /// required then some sort of overlay for that will need to be added.
    ///
    /// Stored in x - z - y order
    pub(self) blocks: Vec<u32>,
}

#[repr(transparent)]
pub struct BlockTypeIndex(u32);

impl BlockStorage {
    pub fn new(sx: u32, sy: u32, sz: u32) -> Self {
        let total_size = (sx * sy * sz) as usize;

        let mut blocks = Vec::with_capacity(total_size);
        blocks.resize(total_size, 0);

        let zsi = sx;
        let ysi = sx * sz;

        Self {
            extents: [sx, sy, sz],
            zsi,
            ysi,
            palette: vec![Block {
                name: "minecraft:air".into(),
                properties: None,
            }],
            blocks,
        }
    }

    pub fn iter_block_indicies(&self) -> iter::BlockIndexIter {
        iter::BlockIndexIter::new(self)
    }

    pub fn add_new_block_type(&mut self, b: Block) -> BlockTypeIndex {
        // Very stupid implementation. Only fix if it shows up in a profile
        // because there will probably never be more than like 30 entries in
        // this array for our usecases.
        for (i, bti) in self.palette.iter().enumerate() {
            if bti == &b {
                return BlockTypeIndex(i as u32);
            }
        }
        let iidx = self.palette.len();
        self.palette.push(b);
        return BlockTypeIndex(iidx as u32);
    }

    #[inline]
    pub fn set_block(&mut self, x: u32, y: u32, z: u32, bti: BlockTypeIndex) {
        let i = x + z * self.zsi + y * self.ysi;
        self.blocks.get_mut(i as usize).map(|v| *v = bti.0);
    }
}
