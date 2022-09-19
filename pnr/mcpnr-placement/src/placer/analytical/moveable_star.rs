use crate::core::{NetlistHypergraph, Signal};

use super::{DecompositionStrategy, NetStrategy, StarAllocator};

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

#[cfg(test)]
mod test {
    use super::MoveableStar;

    use crate::{approx_eq, netlist, placer::analytical::DecompositionStrategy};

    #[test]
    fn three_star() {
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

        let mut strategy = MoveableStar::new();
        strategy.execute(&mut net).expect("Strategy success");

        for i in 0..3 {
            eprintln!("Check index {i}");
            approx_eq!(net.cells.x[i], 0.5);
            approx_eq!(net.cells.y[i], 0.5);
            approx_eq!(net.cells.z[i], 0.5);
        }
    }
}
