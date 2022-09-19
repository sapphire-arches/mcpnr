//! Collection of analytical solvers
use anyhow::{anyhow, Context, Result};
use nalgebra::{DMatrix, DVector, Vector3};

use crate::core::{NetlistHypergraph, Signal};

// TODO: mod anchor_cell, see comments in anchor_net
mod anchor_net;
mod clique;
mod moveable_star;
mod threshold_crossover;

pub use anchor_net::AnchoredByNet;
pub use clique::Clique;
pub use moveable_star::MoveableStar;
pub use threshold_crossover::ThresholdCrossover;

/// Problem statement for quadratic analytical placement, separated by axis. This is assuming the
/// weighted quadratic error,
///
/// With a quadratic formulation for the full system of equations:
/// $$
///  min_{x} (x^t A x + b x + c)
/// $$
///
/// We ignore $c$ because it does not move the minima, and can analytically find the minimum by
/// solving
///
/// $$
///    A x = -b
/// $$
pub struct AnalyticWirelengthProblem {
    hessian: DMatrix<f32>,
    x_vector: DVector<f32>,
    y_vector: DVector<f32>,
    z_vector: DVector<f32>,
}

impl AnalyticWirelengthProblem {
    /// Create a new problem instance of the given size
    pub fn new(size: usize) -> Self {
        Self {
            hessian: DMatrix::zeros(size, size),
            x_vector: DVector::zeros(size),
            y_vector: DVector::zeros(size),
            z_vector: DVector::zeros(size),
        }
    }

    /// Adds a cost term for 2 mobile entities in the quadratic formulation, using the weighting
    /// equation
    ///
    /// $$
    ///    w_{ij} (x_i - x_j)^2 = w_{ij} (x_i^2 - x_i x_j - x_j x_i + x_j^2)
    /// $$
    ///
    /// In the context of the minimization objective this contributes:
    /// $$
    ///  w_{ij} to A_{i,i} and A_{j,j}
    ///  -w_{ij} to A_{i,j} and A_{j,i}
    /// $$
    pub fn cell_mobile_mobile(&mut self, i: usize, j: usize, weight: f32) {
        self.hessian[(i, i)] += weight;
        self.hessian[(j, j)] += weight;
        self.hessian[(i, j)] -= weight;
        self.hessian[(j, i)] -= weight;
    }

    /// A connection from a fixed position (e.g. a pinned cell or an anchor) to a mobile cell, in
    /// the weighted quadratic error formulation:
    ///
    /// $$
    ///  w_{ij} (x_i - x_j)^2 = w_{ij} (x_i^2 - 2 x_i x_j + x_j^2)
    /// $$
    ///
    /// Because x_j is assumed to be constant, this contributes:
    ///
    /// $$
    ///  w_{ij} to A_{i,i}
    ///  w_{ij} x_j to b_i
    /// $$
    pub fn cell_fixed_mobile(&mut self, mobile_index: usize, weight: f32, fixed_pos: Vector3<f32>) {
        self.hessian[(mobile_index, mobile_index)] += weight;

        self.x_vector[mobile_index] += weight * fixed_pos.x;
        self.y_vector[mobile_index] += weight * fixed_pos.y;
        self.z_vector[mobile_index] += weight * fixed_pos.z;
    }

    /// Solve the problem
    pub fn solve(mut self) -> Result<(DVector<f32>, DVector<f32>, DVector<f32>)> {
        let _span = tracing::info_span!("problem_solve", size = self.hessian.shape().0)
            .entered();

        let decomp = tracing::info_span!("invert_hessian").in_scope(|| {
            self.hessian
                .cholesky()
                .ok_or_else(|| anyhow!("The hessian has become non-hermitian"))
        })?;

        tracing::info_span!("solve_x").in_scope(|| {
            decomp.solve_mut(&mut self.x_vector);
        });
        tracing::info_span!("solve_y").in_scope(|| {
            decomp.solve_mut(&mut self.y_vector);
        });
        tracing::info_span!("solve_z").in_scope(|| {
            decomp.solve_mut(&mut self.z_vector);
        });

        return Ok((self.x_vector, self.y_vector, self.z_vector));
    }
}

/// Index of a star
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StarIndex(u32);

/// Allocates star indicies
pub struct StarAllocator {
    next_index: StarIndex,
}

impl StarAllocator {
    /// Create a new star allocator, allocating from index zero
    pub fn new() -> Self {
        Self {
            next_index: StarIndex(0),
        }
    }

    /// Reset the star allocator back to zero
    pub fn reset(&mut self) {
        self.next_index.0 = 0
    }

    /// Allocate a star index
    pub fn next(&mut self) -> StarIndex {
        let idx = self.next_index;
        self.next_index.0 += 1;
        idx
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetStrategy {
    /// All the cells are fixed, nothing to do generally
    AllFixed,
    /// Use a clique model, which connects all pins to all other pins in the net.
    CliqueModel,
    /// Use a star model, which connects all pins to a mobile star
    StarModel { star_idx: StarIndex },
    /// Anchor model, which anchors all the pins in the net to their current center of gravity
    Anchor,
}

/// Used by placers to determine how to decompose multi-pin nets.
pub trait DecompositionStrategy {
    /// Reset the strategy so it can be used to analyze a new [`NetlistHypergraph`].
    fn reset(&mut self);
    /// Analyze a single signal. Will be called exactly once for each signal in the netlist
    fn analyze(&mut self, net: &NetlistHypergraph, signal: &Signal) -> NetStrategy;
    /// Get the number of extra entries required in the hessian matricies, based on the current
    /// analysis results.
    fn extra_entries(&self) -> usize;

    /// Default execution implementation
    fn execute(&mut self, net: &mut NetlistHypergraph) -> Result<()> {
        let _span = tracing::info_span!("analytical_strategy").entered();

        // 2 passes are required because we need to know the problem size up front, and that's only
        //   known by running analysis to allocate the extra entries.
        tracing::info_span!("prepass").in_scope(|| {
            self.reset();
            net.signals.iter().for_each(|signal| {
                self.analyze(net, signal);
            });
        });

        // Construct the problem
        let mut problem =
            AnalyticWirelengthProblem::new(net.mobile_cell_count + self.extra_entries());

        // placeholder weight
        let weight: f32 = 1.0;

        // Second pass, actually does most of the work
        tracing::info_span!("full_pass").in_scope(|| {
            self.reset();
            let strategies = net
                .signals
                .iter()
                .map(|signal| (signal, self.analyze(net, signal)));

            for (signal, strategy) in strategies {
                match strategy {
                    NetStrategy::AllFixed => {
                        // Do nothing, the analysis claims all nets are fixed
                    }
                    NetStrategy::CliqueModel => {
                        let weight = weight / ((signal.connected_cells.len() - 1) as f32);
                        for (idx, &i) in signal.connected_cells.iter().enumerate() {
                            let cell_i = &net.cells[i];
                            for &j in signal.connected_cells.iter().skip(idx + 1) {
                                let cell_j = &net.cells[j];

                                match (cell_i.pos_locked, cell_j.pos_locked) {
                                    (true, true) => {
                                        // Both cells fixed, nothing to do
                                    }
                                    (true, false) => {
                                        problem.cell_fixed_mobile(j, weight, cell_i.center_pos());
                                    }
                                    (false, true) => {
                                        problem.cell_fixed_mobile(i, weight, cell_j.center_pos());
                                    }
                                    (false, false) => {
                                        problem.cell_mobile_mobile(i, j, weight);
                                    }
                                }
                            }
                        }
                    }
                    NetStrategy::StarModel { star_idx } => {
                        let weight = weight / (signal.moveable_cells as f32);
                        for &i in signal.connected_cells.iter() {
                            let cell_i = &net.cells[i];

                            if cell_i.pos_locked {
                                problem.cell_fixed_mobile(
                                    star_idx.0 as usize,
                                    weight,
                                    cell_i.center_pos(),
                                )
                            } else {
                                problem.cell_mobile_mobile(
                                    net.mobile_cell_count + star_idx.0 as usize,
                                    i,
                                    weight,
                                );
                            }
                        }
                    }
                    NetStrategy::Anchor => {
                        let cog: Vector3<f32> = signal
                            .connected_cells
                            .iter()
                            .map(|i| net.cells[*i].center_pos())
                            .fold(Vector3::zeros(), |a, b| a + b)
                            / (signal.connected_cells.len() as f32);

                        let weight = weight / (signal.moveable_cells as f32);

                        for i in signal.iter_mobile(net) {
                            problem.cell_fixed_mobile(i, weight, cog);
                        }
                    }
                }
            }
        });

        // Actually solve the problem, and copy results back to the hypergraph
        let (x, y, z) = problem.solve().context("Final solve")?;

        tracing::info_span!("writeback").in_scope(|| {
            for (i, cell) in net.cells.iter_mut().take(net.mobile_cell_count).enumerate() {
                debug_assert!(!cell.pos_locked);

                cell.x = x[i] - cell.sx / 2.0;
                cell.y = y[i] - cell.sy / 2.0;
                cell.z = z[i] - cell.sz / 2.0;
            }
        });

        Ok(())
    }
}
