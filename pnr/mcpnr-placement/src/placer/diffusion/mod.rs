use std::iter::FusedIterator;

use ndarray::{s, Array3, Axis, Zip};

use crate::core::NetlistHypergraph;

#[cfg(test)]
mod test;

/// Container for all data required for a diffusion-based placement iteration
///
/// Since the term "cell" already means "an atomic unit inside the netlist," we instead call entry
/// in the density tensor a "region." This nicely aligns with terminology used in global routing,
/// where regions represent large slabs of the unit cell grid.
///
/// Based on:
///   - Diffusion-based placement migration, Proceedings of Design Automation Conference 2005, page 515-520
///   - DPlace2.0 - A stable and efficient analytical placement based on diffusion, Asian
///     South-Pacific Design Automation Conference 2008 Page 346 - 351
pub struct DiffusionPlacer {
    /// Size of each diffusion region.
    region_size: usize,
    /// The amount of cell volume contained in each placer region
    density: Array3<f32>,
    /// X velocity field
    vel_x: Array3<f32>,
    /// Y velocity field
    vel_y: Array3<f32>,
}

impl DiffusionPlacer {
    /// Construct a new diffusion placer, with size `(size_x, size_y, size_z)` (specified in cell
    /// grid units) and the given `region_size` (also in cell grid units). The diffusion will then
    /// take place on a grid of size `(size_x / region_size, size_y / region_size, size_z /
    /// region_size)`.
    pub fn new(size_x: usize, size_y: usize, size_z: usize, region_size: usize) -> Self {
        // TODO: handle this more gracefully
        assert!(size_x & region_size == 0);
        assert!(size_y & region_size == 0);
        assert!(size_z & region_size == 0);

        let shape = [
            size_x / region_size,
            size_y / region_size,
            size_z / region_size,
        ];

        Self {
            region_size,
            density: Array3::zeros(shape),
            vel_x: Array3::zeros(shape),
            vel_y: Array3::zeros(shape),
        }
    }

    /// Fill in the density field using the given netlist
    pub fn splat(&mut self, net: &NetlistHypergraph) {
        // This is the obvious algorithm, which splats each cell one by one. It's possible other
        // strategies are more efficient, e.g. iterating over the region grid instead

        for cell in net.cells.iter() {
            let cell_x_start = clamp_coord(cell.x);
            let cell_y_start = clamp_coord(cell.y);
            let cell_z_start = clamp_coord(cell.z);

            let cell_x_end = clamp_coord(cell.x + cell.sx);
            let cell_y_end = clamp_coord(cell.y + cell.sy);
            let cell_z_end = clamp_coord(cell.z + cell.sz);

            let mut cell_x = cell_x_start;
            let mut cell_y = cell_y_start;
            let mut cell_z = cell_z_start;

            let region_x_start = cell_x as usize / self.region_size;
            let region_y_start = cell_y as usize / self.region_size;
            let region_z_start = cell_z as usize / self.region_size;

            let region_x_end = cell_x_end as usize / self.region_size;
            let region_y_end = cell_y_end as usize / self.region_size;
            let region_z_end = cell_z_end as usize / self.region_size;

            cell_z = cell_z_start;
            for region_z in region_z_start..=region_z_end {
                let span_z = advance_coord(&mut cell_z, cell_z_end, region_z, self.region_size);
                cell_y = cell_y_start;
                for region_y in region_y_start..=region_y_end {
                    let span_y = advance_coord(&mut cell_y, cell_y_end, region_y, self.region_size);
                    cell_x = cell_x_start;
                    for region_x in region_x_start..=region_x_end {
                        let span_x =
                            advance_coord(&mut cell_x, cell_x_end, region_x, self.region_size);

                        let coord = (region_x, region_y, region_z);
                        dbg!(coord, span_x, span_y, span_z);
                        self.density[(coord)] += span_x * span_y * span_z;
                    }
                }
            }
        }
    }

    /// Step the density forward in time.
    ///
    /// Uses the "forward-time centered space" scheme, as recommended by the "Diffusion-Based Placement
    /// Migration" paper.
    pub fn step_time(&mut self, dt: f32) {
        let mut density_prime = self.density.clone();

        // The FTCS scheme is formulated like:
        //  d(x) = d(x) + (dt / 2) * (d(x+1) + d(x-1) - 2d(x))
        // where the (dt/2) term is repeated for each individual axis.
        //
        // Since we have 3 axis to step, we want to subtract 6 * (dt/2) and then add the
        //   (dt/2) * (x+1, x-1, y+1, y-1, z+1, z-1)
        // offset values.
        let self_scale = 1.0 - (3.0 * dt);
        density_prime.iter_mut().for_each(|x| {
            *x = *x * self_scale;
        });

        let offset_scale = dt / 2.0;

        // x+1 slice
        Zip::from(density_prime.slice_mut(s![.., .., ..-1]))
            .and(self.density.slice(s![.., .., 1..]))
            .for_each(|prime, orig| {
                *prime += orig * offset_scale;
            });

        // x-1 slice
        Zip::from(density_prime.slice_mut(s![.., .., 1..]))
            .and(self.density.slice(s![.., .., ..-1]))
            .for_each(|prime, orig| {
                *prime += orig * offset_scale;
            });

        // y+1 slice
        Zip::from(density_prime.slice_mut(s![.., ..-1, ..]))
            .and(self.density.slice(s![.., 1.., ..]))
            .for_each(|prime, orig| {
                *prime += orig * offset_scale;
            });

        // y-1 slice
        Zip::from(density_prime.slice_mut(s![.., 1.., ..]))
            .and(self.density.slice(s![.., ..-1, ..]))
            .for_each(|prime, orig| {
                *prime += orig * offset_scale;
            });

        // z+1 slice
        Zip::from(density_prime.slice_mut(s![..-1, .., ..]))
            .and(self.density.slice(s![1.., .., ..]))
            .for_each(|prime, orig| {
                *prime += orig * offset_scale;
            });

        // z-1 slice
        Zip::from(density_prime.slice_mut(s![1.., .., ..]))
            .and(self.density.slice(s![..-1, .., ..]))
            .for_each(|prime, orig| {
                *prime += orig * offset_scale;
            });

        // TODO: probably want to keep the density_prime array around to reduce allocation
        // throughput.
        std::mem::swap(&mut self.density, &mut density_prime);
    }
}

fn clamp_coord(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else {
        x
    }
}

fn advance_coord(cell: &mut f32, end: f32, region: usize, region_size: usize) -> f32 {
    let next_cell = ((region + 1) * region_size) as f32;
    let span = if end < next_cell { end } else { next_cell } - *cell;
    dbg!(*cell, end, region, next_cell, span);

    *cell = next_cell;

    assert!(span >= 0.0);

    span
}
