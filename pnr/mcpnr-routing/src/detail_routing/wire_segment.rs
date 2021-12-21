use anyhow::{anyhow, bail, ensure, Context, Result};
use mcpnr_common::block_storage::{Block, BlockStorage};

use super::{Direction, Layer};

pub const WIRE_GRID_SCALE: i32 = 2;

/// Wire position. This is the "real" coordinate divided by the WIRE_GRID_SCALE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WirePosition {
    x: i32,
    y: i32,
}

impl WirePosition {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn offset(self, d: Direction) -> Result<WirePosition> {
        match d {
            Direction::North => Ok(WirePosition::new(self.x, self.y - 1)),
            Direction::South => Ok(WirePosition::new(self.x, self.y + 1)),
            Direction::East => Ok(WirePosition::new(self.x + 1, self.y)),
            Direction::West => Ok(WirePosition::new(self.x - 1, self.y)),
            _ => Err(anyhow!("Can not offset position in direction {:?}", d)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WireTierLayer {
    tier: u32,
    layer: Layer,
}

impl WireTierLayer {
    pub fn new(tier: u32, layer: Layer) -> Self {
        Self { tier, layer }
    }

    pub fn adjacent(self, other: Self) -> bool {
        if other < self {
            other.adjacent(self)
        } else {
            let delta_tier = other.tier - self.tier;
            match delta_tier {
                0 => self.layer.next() == other.layer.next(),
                1 => self.layer == Layer::M3 && other.layer == Layer::LI,
                _ => false,
            }
        }
    }
}

/// Splat a wire segment into the output storage.
///
/// start_position represents the cell being routed through, with signal flowing into the cell in
/// the direction `input.1` on the layer `input.0` and flowing out of the cell on layer `output.0`
/// in direction `output.1`.
///
/// Returns the position to which signal was routed.
pub fn splat_wire_segment(
    o: &mut BlockStorage,
    start_position: WirePosition,
    input: (WireTierLayer, Direction),
    output: (WireTierLayer, Direction),
) -> Result<(WirePosition, WireTierLayer)> {
    ensure!(
        input.0.adjacent(output.0),
        "Layers are not adjacent: {:?} -> {:?}",
        input.0,
        output.0
    );

    ensure!(
        input.0.tier == output.0.tier,
        "ITVs are not yet supported, {:?} -> {:?}",
        input,
        output
    );

    // TODO: cache these
    let b_calcite = o.add_new_block_type(Block::new("minecraft:calcite".into()));
    let b_redstone = o.add_new_block_type(Block::new("minecraft:redstone_wire".into()));

    if input.0 == output.0 {
        let ix0: u32 = (start_position.x * WIRE_GRID_SCALE)
            .try_into()
            .context("Start X")?;
        let iz0: u32 = (start_position.y * WIRE_GRID_SCALE)
            .try_into()
            .context("Start Z")?;
        let iy = input.0.tier * 16 + input.0.layer.to_y_idx();
        // Same layer routing, very easy.
        match (input.1, output.1) {
            (Direction::South, Direction::West)
            | (Direction::West, Direction::South)
            | (Direction::North, Direction::North)
            | (Direction::South, Direction::South) => {
                // North-South wire
                // South-West wire
                // _ x
                // _ x
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 0)?) = b_calcite;
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 1)?) = b_calcite;
                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 0)?) = b_redstone;
                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 1)?) = b_redstone;
            }
            (Direction::East, Direction::East)
            | (Direction::West, Direction::West)
            | (Direction::North, Direction::East)
            | (Direction::East, Direction::North) => {
                // East-West wire
                // North-East wire
                // _ _
                // x x
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 0)?) = b_calcite;
                (*o.get_block_mut(ix0 + 1, iy + 0, iz0 + 0)?) = b_calcite;
                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 0)?) = b_redstone;
                (*o.get_block_mut(ix0 + 1, iy + 1, iz0 + 0)?) = b_redstone;
            }
            (Direction::North, Direction::West) | (Direction::West, Direction::North) => {
                // North-West wire
                // _ _
                // _ x
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 0)?) = b_calcite;
                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 0)?) = b_redstone;
            }
            (Direction::South, Direction::East) | (Direction::East, Direction::South) => {
                // South-East wire
                // _ x
                // x x
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 0)?) = b_calcite;
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 1)?) = b_calcite;
                (*o.get_block_mut(ix0 + 1, iy + 0, iz0 + 0)?) = b_calcite;

                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 0)?) = b_redstone;
                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 1)?) = b_redstone;
                (*o.get_block_mut(ix0 + 1, iy + 1, iz0 + 0)?) = b_redstone;
            }

            (i, o) => {
                bail!("Unsupported direction combination {:?} -> {:?}", i, o);
            }
        }
        Ok((start_position.offset(input.1)?, input.0))
    } else {
        // We don't care about directionality, the wire legalizer should fix that for us.
        // Therefore we ensure the input is always lower in the stackup than the output
        let (input, output) = if input.0 < output.0 {
            (input, output)
        } else {
            (output, input)
        };

        if output.0.layer == Layer::M0 {
            // Layers are not the same and the higher layer is M0, lower layer must be LI
            assert_eq!(input.0.layer, Layer::LI);
            bail!("Leaving LI not supported");
        } else {
            bail!("Metal-Metal vias not supported");
        }
    }
}
