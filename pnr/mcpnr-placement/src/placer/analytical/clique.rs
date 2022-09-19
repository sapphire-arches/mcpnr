use crate::core::{NetlistHypergraph, Signal};

use super::{DecompositionStrategy, NetStrategy};

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

#[cfg(test)]
mod test {
    use super::Clique;

    use crate::{approx_eq, netlist, placer::analytical::DecompositionStrategy};

    #[test]
    fn simple_2fixed_1mobile() {
        let _ = tracing_subscriber::fmt::try_init();

        let mut net = netlist![
            cells: [
                mobile_0 => (1, 1, 1);
            ],
            fixed_cells: [
                fixed_0 => (0, 0, 0), (1, 1, 1);
                fixed_1 => (2, 2, 2), (1, 1, 1);
            ],
            signals: [
                [mobile_0, fixed_0],
                [mobile_0, fixed_1]
            ]
        ];

        let mut strategy = Clique::new();
        strategy.execute(&mut net).expect("Strategy success");

        approx_eq!(net.cells.x[0], 1.0);
        approx_eq!(net.cells.y[0], 1.0);
        approx_eq!(net.cells.z[0], 1.0);
    }

    #[test]
    fn simple_2fixed_2mobile() {
        let _ = tracing_subscriber::fmt::try_init();

        let mut net = netlist![
            cells: [
                mobile_0 => (1, 1, 1);
                mobile_1 => (1, 1, 1);
            ],
            fixed_cells: [
                fixed_0 => (0, 0, 0), (1, 1, 1);
                fixed_1 => (3, 3, 3), (1, 1, 1);
            ],
            signals: [
                [fixed_0, mobile_0],
                [mobile_0, mobile_1],
                [mobile_1, fixed_1]
            ]
        ];

        let mut strategy = Clique::new();
        strategy.execute(&mut net).expect("Strategy success");

        approx_eq!(net.cells.x[0], 1.0);
        approx_eq!(net.cells.y[0], 1.0);
        approx_eq!(net.cells.z[0], 1.0);

        approx_eq!(net.cells.x[1], 2.0);
        approx_eq!(net.cells.y[1], 2.0);
        approx_eq!(net.cells.z[1], 2.0);
    }

    #[test]
    fn three_clique() {
        let _ = tracing_subscriber::fmt::try_init();

        let mut net = netlist![
            cells: [
                mobile_0 => (1, 1, 1);
                mobile_1 => (1, 1, 1);
                mobile_2 => (1, 1, 1);
            ],
            fixed_cells: [
                fixed_0 => (0, 0, 0), (1, 1, 1);
                fixed_1 => (1, 1, 1), (1, 1, 1);
            ],
            signals: [
                [fixed_0, mobile_0],
                [fixed_1, mobile_0],
                [mobile_0, mobile_1, mobile_2]
            ]
        ];

        let mut strategy = Clique::new();
        strategy.execute(&mut net).expect("Strategy success");

        for i in 0..3 {
            eprintln!("Check index {i}");
            approx_eq!(net.cells.x[i], 0.5);
            approx_eq!(net.cells.y[i], 0.5);
            approx_eq!(net.cells.z[i], 0.5);
        }
    }
}
