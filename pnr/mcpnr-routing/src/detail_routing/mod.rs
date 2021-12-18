use crate::RouteId;
use anyhow::{Context, Result};
use log::debug;
use std::{collections::BinaryHeap, fmt::Display};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

impl Position {
    pub fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GridCell {
    /// Completely free
    Free,
    /// Blocked by something (e.g. part of the guts of a cell
    Blocked,
    /// Occupied by a net with the given RouteID
    Occupied(RouteId),
}

#[derive(Debug, PartialEq, Eq)]
enum Direction {
    /// Z-
    North,
    /// Z+,
    South,
    /// X+
    East,
    /// X-
    West,
    /// +Y
    Up,
    /// -Y
    Down,
}

pub struct DetailRouter {
    size_x: u32,
    size_y: u32,
    size_z: u32,

    zsi: usize,
    ysi: usize,

    grid: Vec<GridCell>,
    score_grid: Vec<u32>,
}

impl DetailRouter {
    pub fn new(size_x: u32, size_y: u32, size_z: u32) -> Self {
        let capacity = (size_x * size_y * size_z) as usize;
        let mut grid = Vec::with_capacity(capacity);
        let mut score_grid = Vec::with_capacity(capacity);

        grid.resize(capacity, GridCell::Free);
        score_grid.resize(capacity, 0);

        Self {
            size_x,
            size_y,
            size_z,
            zsi: size_x as usize,
            ysi: (size_x * size_z) as usize,
            grid,
            score_grid,
        }
    }

    pub fn route(&mut self, start: Position, end: Position, id: RouteId) -> Result<()> {
        // TODO: implement A* by adding an estimate to this
        #[derive(PartialEq, Eq)]
        struct RouteQueueItem {
            cost: u32,
            // TODO: Use routing grid indicies instead of positions
            pos: Position,

            direction_entry: Direction,
        }

        impl PartialOrd for RouteQueueItem {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for RouteQueueItem {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // We intentionally reverse the usual order of comparison for scores because we
                // lower scores to be more important in the priority queue
                other
                    .cost
                    .cmp(&self.cost)
                    .then(self.pos.x.cmp(&other.pos.x))
                    .then(self.pos.y.cmp(&other.pos.y))
            }
        }

        log::info!("Begin routing from {} to {}", start, end);

        // TODO: use some sort of inline marker to avoid needing to clear the full grid on every
        // pass
        // TODO: Use temporary just-right-sized routing grid instead of the full one
        for score in self.score_grid.iter_mut() {
            *score = std::u32::MAX;
        }

        // TODO: use a priority queue
        let mut routing_queue = BinaryHeap::new();

        routing_queue.push(RouteQueueItem {
            cost: 0,
            pos: start,
            // TODO: get entry direction from pin
            direction_entry: Direction::North,
        });

        let bounds_l = Position::new(
            std::cmp::min(start.x, end.x).saturating_sub(2),
            std::cmp::min(start.y, end.y).saturating_sub(2),
            std::cmp::min(start.z, end.z).saturating_sub(2),
        );
        let bounds_h = Position::new(
            std::cmp::max(start.x, end.x) + 2,
            std::cmp::max(start.y, end.y) + 2,
            std::cmp::max(start.z, end.z) + 2,
        );

        while let Some(item) = routing_queue.pop() {
            debug!("Process queue item {} (cost: {})", item.pos, item.cost);
            let idx = self
                .pos_to_idx(item.pos)
                .context("Failed to get index for popped item")?;
            // assert!(item.cost < self.score_grid[idx]);
            if item.cost >= self.score_grid[idx] {
                continue;
            }
            self.score_grid[idx] = item.cost;

            if item.pos == end {
                debug!("Begin backtrack");
                let mut backtrack_pos = item.pos;

                while backtrack_pos != start {
                    let backtrack_pos_idx = self
                        .pos_to_idx(backtrack_pos)
                        .context("Failed to get index for backtrack start")?;
                    debug!("Mark occupied {:?}", backtrack_pos);
                    self.grid[backtrack_pos_idx] = GridCell::Occupied(id);

                    let mut min = self.score_grid[backtrack_pos_idx];
                    let mut min_pos = backtrack_pos;
                    self.for_each_neighbor(backtrack_pos, |neighbor, _| {
                        let idx = self
                            .pos_to_idx(neighbor)
                            .context("Failed to get neighbor index during backtrack")?;
                        let score = self.score_grid[idx];
                        debug!("Consider neighbor {:?} ({} vs {})", neighbor, score, min);
                        if score < min {
                            min = score;
                            min_pos = neighbor;
                        }

                        Ok(())
                    })?;

                    backtrack_pos = min_pos;
                }

                let backtrack_pos_idx = self
                    .pos_to_idx(backtrack_pos)
                    .context("Failed to get index of final step in backtrack")?;
                self.grid[backtrack_pos_idx] = GridCell::Occupied(id);

                return Ok(());
            } else {
                self.for_each_neighbor(item.pos, |neighbor, direction| {
                    // Skip neighbors that leave the bounds of what we care about
                    if !(bounds_l.x <= neighbor.x
                        && neighbor.x <= bounds_h.x
                        && bounds_l.y <= neighbor.y
                        && neighbor.y <= bounds_h.y
                        && bounds_l.z <= neighbor.z
                        && neighbor.z <= bounds_h.z)
                    {
                        return Ok(());
                    }
                    let idx = self
                        .pos_to_idx(neighbor)
                        .context("Failed to get index of new neighbor")?;
                    let grid = self.grid[idx];
                    let cost = item.cost
                        + if grid == GridCell::Free {
                            100
                        } else if grid == GridCell::Occupied(id) {
                            50
                        } else {
                            // Skip this cell because we can't route through it, but don't error
                            return Ok(());
                        }
                        - if direction == item.direction_entry {
                            49
                        } else {
                            0
                        };
                    if cost < self.score_grid[idx] {
                        debug!("Pushing item for {} (cost: {})", neighbor, cost);
                        routing_queue.push(RouteQueueItem {
                            cost,
                            pos: neighbor,
                            direction_entry: direction,
                        })
                    }

                    Ok(())
                })?
            }
        }

        Err(RoutingError::Unroutable)?
    }

    #[inline]
    pub fn get_cell(&self, pos: Position) -> Result<&GridCell> {
        // Unwrap is ok because pos_to_idx does bounds checking
        Ok(self.grid.get(self.pos_to_idx(pos)?).unwrap())
    }

    #[inline]
    pub fn get_cell_mut(&mut self, pos: Position) -> Result<&mut GridCell> {
        let idx = self.pos_to_idx(pos)?;
        Ok(self.grid.get_mut(idx).unwrap())
    }

    #[inline(always)]
    fn pos_to_idx(&self, pos: Position) -> Result<usize> {
        if pos.x >= self.size_x || pos.y >= self.size_y || pos.z >= self.size_z {
            Err(RoutingError::OutOfBounds {
                pos,
                bounds: (self.size_x, self.size_y, self.size_z),
            })?
        } else {
            let x = pos.x as usize;
            let y = pos.y as usize;
            let z = pos.z as usize;
            Ok(x + z * self.zsi + y * self.ysi)
        }
    }

    fn for_each_neighbor(
        &self,
        pos: Position,
        mut f: impl FnMut(Position, Direction) -> Result<()>,
    ) -> Result<()> {
        let plus_y_ok = pos.y < self.size_y - 1;
        let minu_y_ok = pos.y >= 1;

        if pos.x > 0 {
            f(Position::new(pos.x - 1, pos.y, pos.z), Direction::West)?;
            if plus_y_ok {
                f(Position::new(pos.x - 1, pos.y + 1, pos.z), Direction::West)?;
            }
            if minu_y_ok {
                f(Position::new(pos.x - 1, pos.y - 1, pos.z), Direction::West)?;
            }
        }
        if pos.x + 1 < self.size_x {
            f(Position::new(pos.x + 1, pos.y, pos.z), Direction::East)?;
            if plus_y_ok {
                f(Position::new(pos.x + 1, pos.y + 1, pos.z), Direction::East)?;
            }
            if minu_y_ok {
                f(Position::new(pos.x + 1, pos.y - 1, pos.z), Direction::East)?;
            }
        }
        if pos.z > 0 {
            f(Position::new(pos.x, pos.y, pos.z - 1), Direction::North)?;
            if plus_y_ok {
                f(Position::new(pos.x, pos.y + 1, pos.z - 1), Direction::North)?;
            }
            if minu_y_ok {
                f(Position::new(pos.x, pos.y - 1, pos.z - 1), Direction::North)?;
            }
        }
        if pos.z + 1 < self.size_z {
            f(Position::new(pos.x, pos.y, pos.z + 1), Direction::South)?;
            if plus_y_ok {
                f(Position::new(pos.x, pos.y + 1, pos.z + 1), Direction::South)?;
            }
            if minu_y_ok {
                f(Position::new(pos.x, pos.y - 1, pos.z + 1), Direction::South)?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum RoutingError {
    Unroutable,
    OutOfBounds {
        pos: Position,
        bounds: (u32, u32, u32),
    },
}

impl std::error::Error for RoutingError {}

impl Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unroutable => write!(f, "path was unroutable"),
            Self::OutOfBounds {
                pos:
                    Position {
                        ref x,
                        ref y,
                        ref z,
                    },
                bounds: (ref bx, ref by, ref bz),
            } => write!(
                f,
                "access out of bounds: ({}, {}, {}) exceeds ({}, {}, {})",
                x, y, z, bx, by, bz
            ),
        }
    }
}
