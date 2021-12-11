//! Types for storing minecraft-format blocks. This is in mcpnr-common so it
//! can be reused by a future simulator.

pub mod iter;
mod serialization;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::vec::Vec;

// Should go down
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyValue {
    String(String),
    Byte(i8),
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
    /// 3D extents. If changing this is required then it must be done through
    /// Self::resize because all the other fields rely on it staying
    /// the same for the lifetime of this BlockStorage.
    ///
    /// Mutating this without using Self::resize may lead to UB when accessing the block storage.
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

/// Represents a type index into the BlockStorage's palette.
// Must be repr(transparent) as we transmut &'a mut u32 to &'a mut BlockTypeIndex.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

    pub fn resize(&mut self, sx: u32, sy: u32, sz: u32) -> Result<()> {
        unimplemented!("Resizing BlockStorage {} {} {}", sx, sy, sz)
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
    pub fn get_block_mut(&mut self, x: u32, y: u32, z: u32) -> Result<&mut BlockTypeIndex> {
        if x >= self.extents[0] || y >= self.extents[1] || z >= self.extents[2] {
            return Err(anyhow!(
                "Block index out of bounds ({}, {}, {}) exceeds ({}, {}, {})",
                x,
                y,
                z,
                self.extents[0],
                self.extents[1],
                self.extents[2]
            ));
        }
        debug_assert!(
            self.blocks.len() as u32 == self.extents[0] * self.extents[1] * self.extents[2]
        );
        let i = x + z * self.zsi + y * self.ysi;
        // Safety:
        //   index will be within self.blocks.len() due to the check against extents above
        //   transmute from &'a mut i32 to &'a mut BlockTypeIndex is safe due to repr(transparent) on BlockTypeIndex
        unsafe {
            Ok(std::mem::transmute(
                self.blocks.get_unchecked_mut(i as usize),
            ))
        }
    }
}
