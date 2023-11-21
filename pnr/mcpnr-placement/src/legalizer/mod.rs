use crate::{
    config::GeometryConfig,
    placement_cell::{LegalizedCell, PlacementCell},
};

pub mod tetris;

/// Abstract interface over legalizers. Takes in a collection of [PlacementCell]s and converts them
/// to [LegalizedCell]s.
pub(crate) trait Legalizer {
    /// Legalize the provided cells.
    fn legalize(&self, config: &GeometryConfig, cells: &Vec<PlacementCell>) -> Vec<LegalizedCell>;
}
