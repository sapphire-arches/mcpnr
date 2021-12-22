use anyhow::{anyhow, bail, ensure, Context, Result};
use log::debug;
use mcpnr_common::block_storage::{Block, BlockStorage};

use crate::detail_routing::Position;

use super::{Direction, Layer};

pub const WIRE_GRID_SCALE: i32 = 2;

/// Wire position. This is the "real" coordinate divided by the WIRE_GRID_SCALE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WirePosition {
    pub x: i32,
    pub y: i32,
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
                0 => self.layer == other.layer || self.layer.next() == other.layer,
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
    let b_air = o.add_new_block_type(Block::new("minecraft:air".into()));
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
        (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 0)?) = b_calcite;
        (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 0)?) = b_redstone;

        match (input.1, output.1) {
            (Direction::North, Direction::West)
            | (Direction::West, Direction::North)
            | (Direction::North, Direction::North)
            | (Direction::South, Direction::South) => {
                // North-South wire
                // North-West wire
                // _ x
                // _ x
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 1)?) = b_calcite;
                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 1)?) = b_redstone;
            }
            (Direction::East, Direction::East)
            | (Direction::West, Direction::West)
            | (Direction::South, Direction::East)
            | (Direction::East, Direction::South) => {
                // East-West wire
                // South-East wire
                // _ _
                // x x
                (*o.get_block_mut(ix0 + 1, iy + 0, iz0 + 0)?) = b_calcite;
                (*o.get_block_mut(ix0 + 1, iy + 1, iz0 + 0)?) = b_redstone;
            }
            (Direction::North, Direction::East) | (Direction::East, Direction::North) => {
                // North-East wire
                // _ _
                // _ x

                // Already set above, nothing to do but not error
            }
            (Direction::South, Direction::West) | (Direction::West, Direction::South) => {
                // South-West wire
                // _ x
                // x x
                (*o.get_block_mut(ix0 + 0, iy + 0, iz0 + 1)?) = b_calcite;
                (*o.get_block_mut(ix0 + 1, iy + 0, iz0 + 0)?) = b_calcite;

                (*o.get_block_mut(ix0 + 0, iy + 1, iz0 + 1)?) = b_redstone;
                (*o.get_block_mut(ix0 + 1, iy + 1, iz0 + 0)?) = b_redstone;
            }

            (i, o) => {
                bail!("Unsupported direction combination {:?} -> {:?}", i, o);
            }
        }
        Ok((start_position.offset(output.1)?, input.0))
    } else {
        // We don't care about directionality, the wire legalizer should fix that for us.
        // Therefore we ensure the input is always lower in the stackup than the output
        let end_position = start_position.offset(input.1)?.offset(output.1)?;
        let (start_position, input, output) = if input.0 < output.0 {
            (start_position, input, output)
        } else {
            (
                start_position.offset(input.1)?,
                (output.0, input.1.mirror()),
                (input.0, output.1.mirror()),
            )
        };

        let mut set_with_delta = |p: Position, x: i32, y: i32, z: i32| -> Result<()> {
            let x: u32 = (p.x + x).try_into().context("NE fill X")?;
            let y: u32 = (p.y + y).try_into().context("NE fill Y")?;
            let z: u32 = (p.z + z).try_into().context("NE fill Z")?;

            (*o.get_block_mut(x, y + 0, z)?) = b_calcite;
            (*o.get_block_mut(x, y + 1, z)?) = b_redstone;
            (*o.get_block_mut(x, y + 2, z)?) = b_air;

            Ok(())
        };

        let start_position = Position::new(
            start_position.x * WIRE_GRID_SCALE,
            (input.0.tier * 16 + input.0.layer.to_y_idx()) as i32,
            start_position.y * WIRE_GRID_SCALE,
        );
        if output.0.layer == Layer::M0 {
            // Layers are not the same and the higher layer is M0, lower layer must be LI
            assert_eq!(input.0.layer, Layer::LI);

            // Certain I/O direction require special handling
            let start_position = match (input.1, output.1) {
                (Direction::North, Direction::North | Direction::West) => {
                    start_position.offset(Direction::South)
                }
                (Direction::North, Direction::East) => {
                    set_with_delta(start_position, 1, 4, -2).context("NE fill")?;
                    start_position.offset(Direction::South)
                }
                (Direction::South, Direction::South) => start_position,
                (Direction::South, Direction::West) => start_position.offset(Direction::North),
                (Direction::South, Direction::East) => {
                    set_with_delta(start_position, 1, 4, 2).context("SE fill")?;
                    start_position.offset(Direction::North)
                }
                (Direction::East, Direction::North) => start_position.offset(Direction::West),
                (Direction::East, Direction::South) => {
                    set_with_delta(start_position, 2, 4, 1).context("ES fill")?;
                    start_position.offset(Direction::West)
                }
                (Direction::East, Direction::East) => start_position,
                (Direction::West, Direction::North | Direction::West) => {
                    start_position.offset(Direction::East)
                }
                (Direction::West, Direction::South) => {
                    set_with_delta(start_position, -2, 4, 1).context("WS fill")?;

                    start_position.offset(Direction::East)
                }
                (i, o) => bail!(
                    "Unsupported inter-layer via in direction {:?} -> {:?}",
                    i,
                    o
                ),
            };

            // Generate the standard 4-block ramp
            let mut next_position = start_position;
            for _ in 0..4 {
                let x: u32 = next_position.x.try_into().context("LI->M0 ramp start X")?;
                let y: u32 = next_position.y.try_into().context("LI->M0 ramp start Y")?;
                let z: u32 = next_position.z.try_into().context("LI->M0 ramp start Z")?;

                (*o.get_block_mut(x, y + 0, z)?) = b_calcite;
                (*o.get_block_mut(x, y + 1, z)?) = b_redstone;
                (*o.get_block_mut(x, y + 2, z)?) = b_air;

                next_position = next_position.offset(input.1).offset(Direction::Up);
            }
        } else {
            // Generate an appropriate start position for the ramp and the last block (because that
            // one depends on the output direction
            let (start_position, max_steps) = match (input.1, output.1) {
                (Direction::North, Direction::North | Direction::West) => {
                    (start_position.offset(Direction::South), 4)
                }
                (Direction::North, Direction::East) => {
                    set_with_delta(start_position, 1, 3, -2).context("NE fill")?;

                    (start_position.offset(Direction::South), 4)
                }
                (Direction::South, Direction::South) => (start_position, 4),
                (Direction::South, Direction::East) => {
                    set_with_delta(start_position, 1, 3, 2).context("SE fill")?;

                    (start_position, 3)
                }
                (Direction::South, Direction::West) => (start_position, 3),
                (Direction::East, Direction::North) => (start_position, 3),
                (Direction::East, Direction::South) => {
                    set_with_delta(start_position, 2, 3, 1).context("ES fill")?;

                    (start_position, 3)
                }
                (Direction::East, Direction::East) => (start_position, 4),
                (Direction::West, Direction::North) => (start_position.offset(Direction::East), 4),
                (Direction::West, Direction::South) => {
                    set_with_delta(start_position, -2, 3, 1).context("WS fill")?;

                    (start_position.offset(Direction::East), 4)
                }
                (Direction::West, Direction::West) => (start_position.offset(Direction::East), 4),
                (i, o) => bail!(
                    "Unsupported inter-metal-layer via in direction {:?} -> {:?}",
                    i,
                    o
                ),
            };

            // Generate the standard 4-block ramp
            let mut next_position = start_position;
            for _ in 0..max_steps {
                let x: u32 = next_position
                    .x
                    .try_into()
                    .context("inter-metal ramp start X")?;
                let y: u32 = next_position
                    .y
                    .try_into()
                    .context("inter-metal ramp start Y")?;
                let z: u32 = next_position
                    .z
                    .try_into()
                    .context("inter-metal ramp start Z")?;

                (*o.get_block_mut(x, y + 0, z)?) = b_calcite;
                (*o.get_block_mut(x, y + 1, z)?) = b_redstone;
                (*o.get_block_mut(x, y + 2, z)?) = b_air;

                next_position = next_position.offset(input.1).offset(Direction::Up);
            }
        }

        Ok((end_position, output.0))
    }
}
