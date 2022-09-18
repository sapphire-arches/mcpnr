use nalgebra::{DMatrix, DVector, Vector3};

use crate::core::{NetlistHypergraph, Signal};
use anyhow::{anyhow, bail, Context, Result};

#[cfg(test)]
mod test;

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
        let decomp = self
            .hessian
            .cholesky()
            .ok_or_else(|| anyhow!("The hessian has become non-hermitian"))?;

        decomp.solve_mut(&mut self.x_vector);
        decomp.solve_mut(&mut self.y_vector);
        decomp.solve_mut(&mut self.z_vector);

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
        // 2 passes are required because we need to know the problem size up front, and that's only
        //   known by running analysis to allocate the extra entries.
        self.reset();
        net.signals.iter().for_each(|signal| {
            self.analyze(net, signal);
        });

        // Construct the problem
        let mut problem =
            AnalyticWirelengthProblem::new(net.mobile_cell_count + self.extra_entries());

        // placeholder weight
        let weight: f32 = 1.0;

        // Second pass, actually does most of the work
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

        // Actually solve the problem, and copy results back to the hypergraph
        let (x, y, z) = problem.solve().context("Final solve")?;

        for (i, cell) in net.cells.iter_mut().take(net.mobile_cell_count).enumerate() {
            debug_assert!(!cell.pos_locked);

            cell.x = x[i] - cell.sx / 2.0;
            cell.y = y[i] - cell.sy / 2.0;
            cell.z = z[i] - cell.sz / 2.0;
        }

        Ok(())
    }
}

/// A strategy that considers every multipin net a moveable star
pub struct MoveableStar {
    allocator: StarAllocator,
}

impl MoveableStar {
    /// Allocate a new moveable star strategy
    pub fn new() -> Self {
        Self {
            allocator: StarAllocator::new(),
        }
    }
}

impl DecompositionStrategy for MoveableStar {
    fn reset(&mut self) {
        self.allocator.reset();
    }

    fn analyze(&mut self, _net: &NetlistHypergraph, signal: &Signal) -> NetStrategy {
        match signal.moveable_cells {
            0 => NetStrategy::AllFixed,
            1 => NetStrategy::CliqueModel,
            _ => NetStrategy::StarModel {
                star_idx: self.allocator.next(),
            },
        }
    }

    fn extra_entries(&self) -> usize {
        self.allocator.next_index.0 as usize
    }
}

/// A strategy that considers every multipin a clique
pub struct Clique {}

impl Clique {
    /// Allocate a new clique strategy
    pub fn new() -> Self {
        Self {}
    }
}

impl DecompositionStrategy for Clique {
    fn reset(&mut self) {
        // Nothing to do
    }

    fn analyze(&mut self, _net: &NetlistHypergraph, signal: &Signal) -> NetStrategy {
        match signal.moveable_cells {
            0 => NetStrategy::AllFixed,
            _ => NetStrategy::CliqueModel,
        }
    }

    fn extra_entries(&self) -> usize {
        0
    }
}

/// A strategy that considers every multipin net to be anchored by its CoG
pub struct Anchored {}

impl DecompositionStrategy for Anchored {
    fn reset(&mut self) {
        // Nothing to do
    }

    fn analyze(&mut self, _net: &NetlistHypergraph, signal: &Signal) -> NetStrategy {
        match signal.moveable_cells {
            0 => NetStrategy::AllFixed,
            1 => NetStrategy::CliqueModel,
            _ => NetStrategy::Anchor,
        }
    }

    fn extra_entries(&self) -> usize {
        0
    }
}

/// A strategy that crosses over between multiple strategies based on a threshold
pub struct ThresholdCrossover<Small, Large> {
    /// The crossover threshold. Any net with this many moveable pins or more will use the Large strategy
    threshold: usize,
    small: Small,
    large: Large,
}

impl<Small, Large> ThresholdCrossover<Small, Large> {
    pub fn new(threshold: usize, small: Small, large: Large) -> Self {
        Self {
            threshold,
            small,
            large,
        }
    }
}

impl<Small: DecompositionStrategy, Large: DecompositionStrategy> DecompositionStrategy
    for ThresholdCrossover<Small, Large>
{
    fn reset(&mut self) {
        self.small.reset();
        self.large.reset();
    }

    fn analyze(&mut self, net: &NetlistHypergraph, signal: &Signal) -> NetStrategy {
        if signal.moveable_cells >= self.threshold {
            self.large.analyze(net, signal)
        } else {
            self.small.analyze(net, signal)
        }
    }

    fn extra_entries(&self) -> usize {
        let small_entries = self.small.extra_entries();
        let large_entries = self.large.extra_entries();

        // To fix this, we need to make Small and Large aware of eachother (probably move the
        // star allocator to a parameter of analyze?)
        assert!(
            small_entries == 0 || large_entries == 0,
            "Indexing in the solver system will break
                if both small and large entries allocate extra entries"
        );

        small_entries + large_entries
    }
}
