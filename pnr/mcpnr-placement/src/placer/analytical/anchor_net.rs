use crate::core::{NetlistHypergraph, Signal};

use super::{DecompositionStrategy, NetStrategy};

/// A strategy that considers every multipin net to be anchored by its CoG. This is in contrast to
/// a potential [`AnchoredByCell`] strategy that would link each cell to an anchor at the CoG of
/// the cell and all the cells connected to it by any net.
pub struct AnchoredByNet {}

impl AnchoredByNet {
    pub fn new() -> Self {
        Self {}
    }
}

impl DecompositionStrategy for AnchoredByNet {
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

#[cfg(test)]
mod test {
    use super::AnchoredByNet;

    use crate::{approx_eq, netlist, placer::analytical::DecompositionStrategy};

    #[test]
    fn three_anchor_by_net() {
        let _ = tracing_subscriber::fmt::try_init();

        let mut net = netlist![
            cells: [
                mobile_0 => (1, 1, 1);
                mobile_1 => (1, 1, 1);
                mobile_2 => (1, 1, 1);
            ],
            fixed_cells: [
                fixed_0 => (0, 0, 0), (1, 1, 1);
                fixed_1 => (2, 2, 2), (1, 1, 1);
            ],
            signals: [
                // We need to bind all nets so all the cells end up at the same location, as the
                // cell-cell link does not actually affect the AnchoredByNet strategy
                [fixed_0, mobile_0],
                [fixed_1, mobile_0],
                [fixed_0, mobile_1],
                [fixed_1, mobile_1],
                [fixed_0, mobile_2],
                [fixed_1, mobile_2],
                [mobile_0, mobile_1, mobile_2]
            ]
        ];

        // move the moveable cells to a position that will cause locking to have a significant effect
        net.cells.x[0] = 9.0;
        net.cells.y[0] = 9.0;
        net.cells.z[0] = 9.0;

        net.cells.x[1] = 8.9;
        net.cells.y[1] = 8.9;
        net.cells.z[1] = 8.9;

        net.cells.x[2] = 9.1;
        net.cells.y[2] = 9.1;
        net.cells.z[2] = 9.1;

        let mut strategy = AnchoredByNet::new();
        strategy.execute(&mut net).expect("Strategy success");

        for i in 0..3 {
            eprintln!("Check index {i}");
            approx_eq!(net.cells.x[i], 2.1428574);
            approx_eq!(net.cells.y[i], 2.1428574);
            approx_eq!(net.cells.z[i], 2.1428574);
        }
    }
}
