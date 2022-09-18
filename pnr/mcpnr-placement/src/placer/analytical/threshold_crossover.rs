use crate::core::{NetlistHypergraph, Signal};

use super::{DecompositionStrategy, NetStrategy};

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
