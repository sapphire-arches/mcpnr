//! Types for storing minecraft-format blocks. This is in mcpnr-common so it
//! can be reused by a future simulator.

pub mod iter;
mod serialization;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fmt::Display;
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

    pub fn is_sticky(&self) -> bool {
        match self.name.as_str() {
            "minecraft:honey_block" => true,
            "minecraft:slime_block" => true,
            _ => false,
        }
    }

    pub fn is_pushable(&self) -> bool {
        match self.name.as_str() {
            "minecraft:air" => false,
            "minecraft:obsidian" => false,
            "minecraft:bedrock" => false,
            // TODO: other unpushable blocks
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

impl Position {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn in_bounding_box(&self, min: &Self, max: &Self) -> bool {
        let x = min.x <= self.x && self.x < max.x;
        let y = min.y <= self.y && self.y < max.y;
        let z = min.z <= self.z && self.z < max.z;

        x && y && z
    }

    pub fn offset(&self, d: Direction) -> Self {
        match d {
            Direction::North => Position::new(self.x, self.y, self.z - 1),
            Direction::South => Position::new(self.x, self.y, self.z + 1),
            Direction::East => Position::new(self.x + 1, self.y, self.z),
            Direction::West => Position::new(self.x - 1, self.y, self.z),
            Direction::Up => Position::new(self.x, self.y + 1, self.z),
            Direction::Down => Position::new(self.x, self.y - 1, self.z),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Z-
    North,
    /// Z+,
    South,
    /// X+
    East,
    /// X-
    West,
    /// +Y
    Up,
    /// -Y
    Down,
}

impl Direction {
    #[inline]
    pub fn mirror(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}

pub const PLANAR_DIRECTIONS: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
];

#[allow(unused)]
pub const ALL_DIRECTIONS: [Direction; 6] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
    Direction::Up,
    Direction::Down,
];
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

    pub fn iter_block_coords(&self) -> iter::BlockCoordIter {
        iter::BlockCoordIter::new(self)
    }

    pub fn iter_block_coords_mut(&mut self) -> iter::BlockCoordMutIter {
        iter::BlockCoordMutIter::new(self)
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

    pub fn extents(&self) -> &[u32; 3] {
        &self.extents
    }

    pub fn info_for_index(&self, index: BlockTypeIndex) -> Option<&Block> {
        self.palette.get(index.0 as usize)
    }

    #[inline]
    pub fn get_block(&self, x: u32, y: u32, z: u32) -> Result<&BlockTypeIndex> {
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
        //   transmute from &'a i32 to &'a BlockTypeIndex is safe due to repr(transparent) on BlockTypeIndex
        unsafe { Ok(std::mem::transmute(self.blocks.get_unchecked(i as usize))) }
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
