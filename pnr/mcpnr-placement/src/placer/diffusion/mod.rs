use ndarray::{s, Array3, Axis, Slice, Zip};
use tracing::info_span;

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
    /// Z velocity field
    vel_z: Array3<f32>,
}

impl DiffusionPlacer {
    /// Construct a new diffusion placer, with size `(size_x, size_y, size_z)` (specified in cell
    /// grid units) and the given `region_size` (also in cell grid units). The diffusion will then
    /// take place on a grid of size `(2 + size_x / region_size, 2 + size_y / region_size, 2 + size_z /
    /// region_size)`.
    ///
    /// We add 2 cells to act as a border across which cells cannot traverse, without having to
    /// deal with the complexity of ensuring nonzero velocity to push cells off the borders of the
    /// placement region.
    pub fn new(size_x: usize, size_y: usize, size_z: usize, region_size: usize) -> Self {
        // TODO: handle this more gracefully
        assert!(size_x % region_size == 0);
        assert!(size_y % region_size == 0);
        assert!(size_z % region_size == 0);

        let shape = [
            2 + size_x / region_size,
            2 + size_y / region_size,
            2 + size_z / region_size,
        ];

        Self {
            region_size,
            density: Array3::zeros(shape),
            vel_x: Array3::zeros(shape),
            vel_y: Array3::zeros(shape),
            vel_z: Array3::zeros(shape),
        }
    }

    /// Fill in the density field using the given netlist
    pub fn splat(&mut self, net: &NetlistHypergraph) {
        let region_size_f = self.region_size as f32;
        let size_f = (self.density.shape()[0] - 2) as f32;

        // Start with a clean slate
        self.density.fill(0.0);

        // This is the obvious algorithm, which splats each cell one by one. It's possible other
        // strategies are more efficient, e.g. iterating over the region grid instead and then
        // finding the cells in an acceleration structure.
        for cell in net.cells.iter() {
            // We add 1 after clamping to ensure placement inside the "live" region and not the
            // margins
            let cell_x_start = region_size_f + cell.x.clamp(0.0, size_f);
            let cell_y_start = region_size_f + cell.y.clamp(0.0, size_f);
            let cell_z_start = region_size_f + cell.z.clamp(0.0, size_f);

            let cell_x_end = region_size_f + (cell.x + cell.sx).clamp(0.0, size_f);
            let cell_y_end = region_size_f + (cell.y + cell.sy).clamp(0.0, size_f);
            let cell_z_end = region_size_f + (cell.z + cell.sz).clamp(0.0, size_f);

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
                        self.density[(coord)] += span_x * span_y * span_z;
                    }
                }
            }
        }

        // Fill the margins with some high density value to encourage cell movement away from the
        // absolute edges.
        let margin_fill = (self.region_size as f32).powi(3) * 8.0;
        for axis in 0..3 {
            let axis = Axis(axis);
            Zip::from(self.density.slice_axis_mut(axis, Slice::new(0, Some(1), 1)))
                .for_each(|v| *v = margin_fill);
            Zip::from(
                self.density
                    .slice_axis_mut(axis, Slice::new(-1, Some(-1), 1)),
            )
            .for_each(|v| *v = margin_fill);
        }
    }

    /// Compute the flow velocities, based on the current density in each region.
    ///
    /// Assumes the area outside the grid is overfilled by a factor of 8, to encourage cells to
    /// leave the border of the chip.
    pub fn compute_velocities(&mut self) {
        let _span = info_span!("Computing velocities").entered();
        // Implements:
        //   v_0(x, y, z) = - (d(x+1) - d(x - 1)) / (2 * d(x))
        let mut velocities = [&mut self.vel_x, &mut self.vel_y, &mut self.vel_z];

        for axis in 0..3 {
            let vel_grid = &mut velocities[axis];
            let axis = Axis(axis);
            Zip::from(vel_grid.slice_axis_mut(axis, Slice::from(1isize..-1)))
                .and(self.density.slice_axis(axis, Slice::from(1isize..-1)))
                .and(self.density.slice_axis(axis, Slice::from(2isize..)))
                .and(self.density.slice_axis(axis, Slice::from(..-2isize)))
                .for_each(|v, z, p, n| *v = (n - p) / (-2.0 * z));
        }
    }

    /// Move cells according to the computed velocity fields.
    pub fn move_cells(&self, net: &mut NetlistHypergraph, dt: f32) {
        let axies = [&self.vel_x, &self.vel_y, &self.vel_z];

        for cell in net.cells.iter_mut() {
            if cell.pos_locked {
                continue;
            }

            let p = cell.center_pos() / (self.region_size as f32);
            let i = (p.x as usize + 1, p.y as usize + 1, p.z as usize + 1);
            let f0 = (p.x.fract(), p.y.fract(), p.z.fract());
            let f1 = (1.0 - f0.0, 1.0 - f0.1, 1.0 - f0.2);
            let c000 = (i.0 + 0, i.1 + 0, i.2 + 0);
            let c001 = (i.0 + 0, i.1 + 0, i.2 + 1);
            let c010 = (i.0 + 0, i.1 + 1, i.2 + 0);
            let c011 = (i.0 + 0, i.1 + 1, i.2 + 1);
            let c100 = (i.0 + 1, i.1 + 0, i.2 + 0);
            let c101 = (i.0 + 1, i.1 + 0, i.2 + 1);
            let c110 = (i.0 + 1, i.1 + 1, i.2 + 0);
            let c111 = (i.0 + 1, i.1 + 1, i.2 + 1);

            for (axis, vel) in axies.iter().enumerate() {
                let v000 = vel[c000];
                let v001 = vel[c001];
                let v010 = vel[c010];
                let v011 = vel[c011];
                let v100 = vel[c100];
                let v101 = vel[c101];
                let v110 = vel[c110];
                let v111 = vel[c111];

                let x00 = (v000 * f1.0) + (v001 * f0.0);
                let x01 = (v010 * f1.0) + (v011 * f0.0);
                let x10 = (v100 * f1.0) + (v101 * f0.0);
                let x11 = (v110 * f1.0) + (v111 * f0.0);

                let y0 = (x00 * f1.1) + (x01 * f0.1);
                let y1 = (x10 * f1.1) + (x11 * f0.1);

                let v = (y0 * f1.2) + (y1 * f0.2);

                match axis {
                    0 => cell.x += v * dt,
                    1 => cell.y += v * dt,
                    2 => cell.z += v * dt,
                    _ => unreachable!("Only 3 axies"),
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

fn advance_coord(cell: &mut f32, end: f32, region: usize, region_size: usize) -> f32 {
    let next_cell = ((region + 1) * region_size) as f32;
    let span = if end < next_cell { end } else { next_cell } - *cell;

    *cell = next_cell;

    assert!(span >= 0.0);

    span
}