//! Implementation of the TETRIS legalizer, first described in "Method and system for high speed
//! detailed placement of cells within an integrated circuit design" (USPTO patent 6370673)

use std::{
    cmp::Ordering,
    mem::{ManuallyDrop, MaybeUninit},
};

use itertools::Itertools;

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
        // construction (`output` is always matches the order of `cells`) but carries some risk of
        // us causing memory safety problems.

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
        let mut output: ManuallyDrop<Vec<MaybeUninit<LegalizedCell>>> = ManuallyDrop::new(
            (0..cells.len())
                .map(|_| unsafe { MaybeUninit::uninit().assume_init() })
                .collect(),
        );

        for cell_i in cell_order {
            let cell = &cells[cell_i];
            let legalized = LegalizedCell::from_placement(cell);
            log::info!("{}", cell_i);

            // See INTERNAL SAFETY REQUIREMENTS comment above
            output[cell_i].write(legalized);
        }

        {
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
