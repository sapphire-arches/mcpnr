//! Implementation of the TETRIS legalizer, first described in "Method and system for high speed
//! detailed placement of cells within an integrated circuit design" (USPTO patent 6370673)

use std::{
    cmp::Ordering,
    mem::{ManuallyDrop, MaybeUninit},
};

use itertools::Itertools;
use mcpnr_common::{BLOCKS_PER_Z_ROW, BLOCKS_PER_TIER};
use nalgebra::Vector3;

use crate::{
    config::GeometryConfig,
    placement_cell::{LegalizedCell, PlacementCell},
};

use super::Legalizer;

pub struct TetrisLegalizer {
    /// The "left limit" from the TETRIS paper. Represents how far left of the original X location
    /// of the cell we're allowed to insert.
    left_limit: u32,
}

impl TetrisLegalizer {
    pub fn new(left_limit: u32) -> Self {
        TetrisLegalizer { left_limit }
    }
}

impl Legalizer for TetrisLegalizer {
    fn legalize(&self, config: &GeometryConfig, cells: &Vec<PlacementCell>) -> Vec<LegalizedCell> {
        let _span = tracing::info_span!("tetris_legalize").entered();
        // !!!! INTERNAL SAFETY REQUIREMENTS !!!!
        // We build the output vector out of order, which means we allocate the whole thing as
        // MaybeUninits and then initialize them as we get through the cell_order. Doing things
        // this way avoids a bunch of needless memory traffic unswizling the array after
        // construction (`output` always matches the order of `cells`) but carries some risk of
        // us causing memory safety problems.
        let mut output: Vec<MaybeUninit<LegalizedCell>> = (0..cells.len())
            .map(|_| unsafe { MaybeUninit::uninit().assume_init() })
            .collect();

        // We can't actually sort the cell list because NetlistHypergraph::metadata relies on being
        // the same order as the original cells. Therefore we sort an index array and just eat the
        // bad cache performance. In principle we could copy and then sort, but we would still need
        // this mapping to construct the output array in the correct order.
        let cell_order = {
            let mut order = (0..cells.len()).collect_vec();
            // Sort by the `x` coordinate, with preference to locked cells.
            order.sort_unstable_by(|a, b| {
                let a = &cells[*a];
                let b = &cells[*b];

                if a.pos_locked ^ b.pos_locked {
                    if a.pos_locked {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    }
                } else {
                    a.x.total_cmp(&b.x)
                }
            });
            order
        };

        // The heart of the TETRIS algorithm is maintaining a sense of the "left most" (minimum X
        // coordinate) for every possible (y,z) tuple that won't collide with something already
        // legalized. The brute-force way to do this is to have 1 bit per location in the block
        // grid, but we make a few optimizations.
        //   1) we reduce the (y,z) space to be represented in terms of rows of 6 blocks and 1-high
        //      layers (as that's the size of most of the cells)
        //   2) we store only the leftmost free x coordinate for each of these tuples. This can
        //      break horribly if there are fixed position cells that are located on the +x edge
        //      but we ignore that possibility for now (and warn about it in CellFactory::build_*).
        //
        // For each cell then, we:
        //  - immediately lock it and update the min_x[(y,z)] if it's pos-locked
        //  - for each row(y,z)
        //      - compute the "cost" if we were to put the cell there, taking left_limit in to
        //        account
        //      - if this cost is better than any we've seen before, keep it in mind
        //  - select the best found row and update the min_x for that row
        //
        let max_y = config.size_y;
        // Takes a (layer, row coordinate) pair for (y,z) and converts it to the row index
        let row_idx = |y: u32, z: u32| {
            (y + z * max_y) as usize
        };
        let mut min_x = Vec::with_capacity((max_y * config.size_z / BLOCKS_PER_Z_ROW) as usize);
        for _ in 0..min_x.capacity() {
            min_x.push(0u32);
        }

        for cell_i in cell_order {
            let cell = &cells[cell_i];
            let mut legalized = LegalizedCell::from_placement(cell);

            // locked cells need to be where  they say they are, regardless of what else we're
            // doing to them. Other cells get properly legalized.
            if !cell.pos_locked {
                let mut min_cost = f32::INFINITY;
                let mut min_cost_pos = Vector3::new(0u32, 0, 0);
                for (i, &x) in min_x.iter().enumerate() {
                    let x = if legalized.x > self.left_limit && x < legalized.x - self.left_limit {
                        legalized.x
                    } else {
                        x
                    };
                    let y = (i as u32) % max_y;
                    let z_row = (i as u32) / max_y;

                    let min_pos = Vector3::new(x as f32, y as f32, (z_row * BLOCKS_PER_Z_ROW) as f32);
                    let cell_pos = Vector3::new(cell.x, cell.tier_y, cell.z);
                    let delta = (min_pos - cell_pos).abs();

                    let cost = delta.x + delta.y * BLOCKS_PER_TIER as f32 + delta.z * BLOCKS_PER_Z_ROW as f32;

                    if cost < min_cost && x + legalized.sx <= config.size_x {
                        min_cost = cost;
                        min_cost_pos = Vector3::new(x, y, z_row * BLOCKS_PER_Z_ROW);
                    }
                }

                legalized.x = min_cost_pos.x;
                legalized.tier_y = min_cost_pos.y;
                legalized.z = min_cost_pos.z;
            }

            let row_x = legalized.x;
            let row_y = legalized.tier_y;
            let row_z = legalized.z / BLOCKS_PER_Z_ROW;
            min_x[row_idx(row_y, row_z)] = row_x + legalized.sx;

            // See INTERNAL SAFETY REQUIREMENTS comment above
            output[cell_i].write(legalized);
        }

        {
            let mut output = ManuallyDrop::new(output);
            let length = output.len();
            let capacity = output.capacity();
            let data = output.as_mut_ptr();

            // See INTERNAL SAFETY REQUIREMENTS comment above
            //
            // Do not drop the original "output" because we've rebuilt it here
            unsafe { Vec::from_raw_parts(std::mem::transmute(data), length, capacity) }
        }
    }
}
