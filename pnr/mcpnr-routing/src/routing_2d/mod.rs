use anyhow::Result;
use std::fmt::Display;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    pub x: u32,
    pub y: u32,
}

impl Position {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

pub struct Router2D {}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteId(u32);

impl Router2D {
    pub fn new(size_x: u32, size_y: u32) -> Self {
        todo!()
    }

    pub fn route(&mut self, start: Position, end: Position, id: RouteId) -> Result<()> {
        todo!()
    }

    pub fn rip_up(&mut self, id: RouteId) -> Result<()> {
        todo!()
    }

    #[inline]
    pub fn is_cell_occupied(&self, pos: Position) -> Result<RouteId> {
        todo!()
    }

    #[inline]
    pub fn mark_cell_occupied(&mut self, pos: Position, id: RouteId) -> Result<()> {
        todo!()
    }
}

#[derive(Debug, PartialEq)]
pub enum RoutingError {
    Unroutable,
    OutOfBounds { pos: Position, bounds: (u32, u32) },
}

impl std::error::Error for RoutingError {}

impl Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unroutable => write!(f, "path was unroutable"),
            Self::OutOfBounds {
                pos: Position { ref x, ref y },
                bounds: (ref bx, ref by),
            } => write!(
                f,
                "access out of bounds: ({}, {}) exceeds ({}, {})",
                x, y, bx, by
            ),
        }
    }
}
