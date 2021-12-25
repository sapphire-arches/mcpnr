use crate::RouteId;
use anyhow::{anyhow, bail, ensure, Context, Result};
use log::{debug, info};
use mcpnr_common::block_storage::{Direction, Position, ALL_DIRECTIONS};
use std::{collections::BinaryHeap, fmt::Display};

use self::wire_segment::WireCoord;

#[cfg(test)]
mod tests;

pub mod wire_segment;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GridCell {
    /// Completely free
    Free,
    /// Blocked by something (e.g. part of the guts of a cell
    Blocked,
    /// Occupied by a net with the given RouteId, driver is in the given Direction
    Occupied(Direction, RouteId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridCellPosition {
    pub x: WireCoord,
    /// This is tier * LAYERS_PER_TIER + layer.to_compact_idx
    pub y: i32,
    pub z: WireCoord,
}

impl GridCellPosition {
    pub fn new(x: WireCoord, y: i32, z: WireCoord) -> Self {
        Self { x, y, z }
    }

    pub fn in_bounding_box(&self, min: &Self, max: &Self) -> bool {
        let x = min.x <= self.x && self.x < max.x;
        let y = min.y <= self.y && self.y < max.y;
        let z = min.z <= self.z && self.z < max.z;

        x && y && z
    }

    pub fn offset(self, d: Direction) -> Self {
        match d {
            Direction::North => GridCellPosition::new(self.x, self.y, self.z - 1),
            Direction::South => GridCellPosition::new(self.x, self.y, self.z + 1),
            Direction::East => GridCellPosition::new(self.x + 1, self.y, self.z),
            Direction::West => GridCellPosition::new(self.x - 1, self.y, self.z),
            Direction::Up => GridCellPosition::new(self.x, self.y + 1, self.z),
            Direction::Down => GridCellPosition::new(self.x, self.y - 1, self.z),
        }
    }
}

impl TryFrom<Position> for GridCellPosition {
    type Error = anyhow::Error;

    fn try_from(p: Position) -> Result<Self> {
        let tier = p.y / 16;
        let layer = Layer::from_y_idx(p.y % 16)?;

        Ok(GridCellPosition {
            x: WireCoord::from_block_coord(p.x),
            y: (tier * LAYERS_PER_TIER as i32) + layer.to_compact_idx(),
            z: WireCoord::from_block_coord(p.z),
        })
    }
}

impl Display for GridCellPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tier = self.y / LAYERS_PER_TIER as i32;
        let layer = Layer::from_compact_idx(self.y % LAYERS_PER_TIER as i32);

        match layer {
            Ok(layer) => write!(
                f,
                "({}, {}) in {:?} of tier {}",
                self.x.0, self.z.0, layer, tier
            ),
            Err(_) => write!(
                f,
                "({}, {}) in (UNSUPPPORTED LAYER IDX {}) of tier {}",
                self.x.0,
                self.z.0,
                self.y % LAYERS_PER_TIER as i32,
                tier
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    // [0, 4)
    LI,
    // [4, 7)
    M0,
    // [7, 10)
    M1,
    // [10, 13)
    M2,
    // [13, 16)
    M3,
}

impl Layer {
    #[inline]
    pub fn next(self) -> Layer {
        match self {
            Layer::LI => Layer::M0,
            Layer::M0 => Layer::M1,
            Layer::M1 => Layer::M2,
            Layer::M2 => Layer::M3,
            Layer::M3 => Layer::LI,
        }
    }

    pub fn from_y_idx(y: i32) -> Result<Layer> {
        ensure!(
            0 <= y && y < 16,
            "Y {} out of range, did you forget to mod by 16?",
            y
        );
        if y < 4 {
            Ok(Layer::LI)
        } else {
            Ok(ALL_LAYERS[1 + ((y - 4) / 3) as usize])
        }
    }

    pub fn to_y_idx(self) -> u32 {
        match self {
            Layer::LI => 0,
            Layer::M0 => 4,
            Layer::M1 => 7,
            Layer::M2 => 10,
            Layer::M3 => 13,
        }
    }

    pub fn to_compact_idx(self) -> i32 {
        match self {
            Layer::LI => 0,
            Layer::M0 => 1,
            Layer::M1 => 2,
            Layer::M2 => 3,
            Layer::M3 => 4,
        }
    }

    pub fn from_compact_idx(compact: i32) -> Result<Self> {
        match compact {
            0 => Ok(Layer::LI),
            1 => Ok(Layer::M0),
            2 => Ok(Layer::M1),
            3 => Ok(Layer::M2),
            4 => Ok(Layer::M3),
            _ => Err(anyhow!("Unsupported compact idx in conversion {}", compact)),
        }
    }
}

pub const ALL_LAYERS: [Layer; 5] = [Layer::LI, Layer::M0, Layer::M1, Layer::M2, Layer::M3];

pub const LAYERS_PER_TIER: u32 = ALL_LAYERS.len() as u32;

#[derive(Debug, PartialEq, Eq)]
enum StepDirection {
    StepUp,
    StepDown,
    NoStep,
}

pub struct DetailRouter {
    size_x: i32,
    size_y: i32,
    size_z: i32,

    zsi: usize,
    ysi: usize,

    grid: Vec<GridCell>,
    score_grid: Vec<u32>,

    current_bounds_min: GridCellPosition,
    current_bounds_max: GridCellPosition,
}

impl DetailRouter {
    pub fn new(size_x: u32, size_y: u32, size_z: u32) -> Self {
        let capacity = (size_x * size_y * size_z) as usize;

        let mut grid = Vec::with_capacity(capacity);
        let mut score_grid = Vec::with_capacity(capacity);

        grid.resize(capacity, GridCell::Free);
        score_grid.resize(capacity, 0);

        let size_x = size_x as i32;
        let size_y = size_y as i32;
        let size_z = size_z as i32;

        Self {
            size_x,
            size_y,
            size_z,
            zsi: size_x as usize,
            ysi: (size_x * size_z) as usize,
            grid,
            score_grid,

            current_bounds_min: GridCellPosition::new(WireCoord(0), 0, WireCoord(0)),
            current_bounds_max: GridCellPosition::new(WireCoord(0), 0, WireCoord(0)),
        }
    }

    pub fn route(
        &mut self,
        start: GridCellPosition,
        end: GridCellPosition,
        id: RouteId,
    ) -> Result<()> {
        // TODO: implement A* by adding an estimate to this
        #[derive(PartialEq, Eq)]
        struct RouteQueueItem {
            cost: u32,
            // TODO: Use routing grid indicies instead of positions
            pos: GridCellPosition,

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

        log::info!("Begin routing net {:?} from {} to {}", id, start, end);

        let start_direction = if let GridCell::Occupied(d, gid) = self.get_cell(start)? {
            ensure!(
                *gid == id,
                "Pin start is occupied by a net with non-matching {:?} (should be {:?})",
                gid,
                id
            );
            *d
        } else {
            bail!(
                "Pin start is not occupied, it is instead {:?}",
                self.get_cell(start)?
            );
        };

        let end_direction = if let GridCell::Occupied(d, gid) = self.get_cell(end)? {
            ensure!(
                *gid == id,
                "Pin end is occupied by a net with non-matching {:?} (should be {:?})",
                gid,
                id
            );
            *d
        } else {
            bail!(
                "Pin end is not occupied, it is instead {:?}",
                self.get_cell(end)?
            );
        };

        // TODO: use some sort of inline marker to avoid needing to clear the full grid on every
        // pass
        // TODO: Use temporary just-right-sized routing grid instead of the full one
        for score in self.score_grid.iter_mut() {
            *score = std::u32::MAX;
        }

        self.current_bounds_min = GridCellPosition::new(
            std::cmp::max(std::cmp::min(start.x, end.x) - 2, WireCoord(0)),
            std::cmp::max(std::cmp::min(start.y, end.y) - 2, 0),
            std::cmp::max(std::cmp::min(start.z, end.z) - 2, WireCoord(0)),
        );
        self.current_bounds_max = GridCellPosition::new(
            std::cmp::max(start.x, end.x) + 2,
            std::cmp::max(start.y, end.y) + 2,
            std::cmp::max(start.z, end.z) + 2,
        );

        let start = start.offset(start_direction.mirror());
        let end = end.offset(end_direction);

        match self.get_cell(start)? {
            GridCell::Free => {}
            GridCell::Blocked => {
                return Err(RoutingError::Unroutable)
                    .context("Start pin points directly at an unroutable cell")
            }
            GridCell::Occupied(_, i) => {
                if *i != id {
                    return Err(RoutingError::Unroutable).context(anyhow!(
                        "Start pin points directly at a cell already occupied by route {:?}",
                        id
                    ));
                }
            }
        };

        match self.get_cell(start)? {
            GridCell::Free => {}
            GridCell::Blocked => {
                return Err(RoutingError::Unroutable)
                    .context("End pin points directly at an unroutable cell")
            }
            GridCell::Occupied(_, i) => {
                if *i != id {
                    return Err(RoutingError::Unroutable).context(anyhow!(
                        "End pin points directly at a cell already occupied by route {:?}",
                        id
                    ));
                }
            }
        };

        let mut routing_queue = BinaryHeap::new();

        // Routing should start at the cell specified by the direction of the pin
        routing_queue.push(RouteQueueItem {
            cost: 0,
            pos: start,
            direction_entry: start_direction.mirror(),
        });

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
                return self.do_backtrack(end, end_direction, start, start_direction, id);
            } else {
                self.for_each_neighbor(
                    item.pos,
                    item.direction_entry,
                    id,
                    |neighbor, direction, is_step| {
                        // Skip neighbors that leave the bounds of what we care about
                        if !self.is_in_bounds(neighbor) {
                            debug!("Skipping {} because it leaves bounding box", neighbor);
                            return Ok(());
                        }
                        let idx = self
                            .pos_to_idx(neighbor)
                            .context("Failed to get index of new neighbor")?;
                        let grid = self.grid[idx];
                        let cost = item.cost
                            + match grid {
                                GridCell::Free => 100,
                                GridCell::Blocked => 10_000_000,
                                GridCell::Occupied(_, nid) => {
                                    if id == nid {
                                        50
                                    } else {
                                        // Skip this cell because we can't route through it, but don't error
                                        debug!(
                                            "Skipping {} because it's blocked by {:?}",
                                            neighbor, grid
                                        );
                                        return Ok(());
                                    }
                                }
                            }
                            + match direction {
                                Direction::Up | Direction::Down => 1000,
                                _ => 0,
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
                    },
                )
                .context("Forward search neighbors")?
            }
        }

        debug!("Failed to route net {:?} from {:?} to {:?}", id, start, end);
        self.debug_dump();
        Err(RoutingError::Unroutable)?
    }

    fn do_backtrack(
        &mut self,
        end: GridCellPosition,
        end_direction: Direction,
        start: GridCellPosition,
        start_direction: Direction,
        id: RouteId,
    ) -> Result<()> {
        debug!("Begin backtrack");
        // Start the backtrack offset by the
        let mut backtrack_pos = end;
        let mut min_pos = backtrack_pos;
        let mut min_direction = end_direction;
        let mut min_step = StepDirection::NoStep;
        let mut last_backtrack_pos = GridCellPosition::new(WireCoord(0), 0, WireCoord(0));

        while backtrack_pos != start {
            if backtrack_pos == last_backtrack_pos {
                info!(
                    "Bounds: {} {} for {:?}",
                    self.current_bounds_min, self.current_bounds_max, id
                );
                self.debug_dump();

                panic!("Backtrack did not make progress");
            }

            let backtrack_pos_idx = self
                .pos_to_idx(backtrack_pos)
                .context("Failed to get index for backtrack start")?;
            debug!(
                "Mark occupied {:?} (facing {:?})",
                backtrack_pos, min_direction
            );
            // TODO: handle vias

            let mut min = self.score_grid[backtrack_pos_idx];
            self.for_each_neighbor(
                backtrack_pos,
                min_direction,
                id,
                |neighbor, direction, step| {
                    debug!("  Consider neighbor {}", neighbor);
                    if !self.is_in_bounds(neighbor) {
                        debug!(
                            "  Discard neighbor {} because it is out of bounds",
                            neighbor
                        );
                        return Ok(());
                    }
                    let idx = self
                        .pos_to_idx(neighbor)
                        .context("  Failed to get neighbor index during backtrack")?;
                    let score = self.score_grid[idx];
                    debug!("  Consider neighbor {:?} ({} vs {})", neighbor, score, min);
                    if score < min {
                        min = score;
                        min_pos = neighbor;
                        min_direction = direction;
                        min_step = step;
                    }

                    Ok(())
                },
            )
            .context("During backtrack neighbor search")?;
            self.grid
                .get_mut(backtrack_pos_idx)
                .map(|v| *v = GridCell::Occupied(min_direction, id));

            last_backtrack_pos = backtrack_pos;
            backtrack_pos = min_pos;
        }

        let backtrack_pos_idx = self
            .pos_to_idx(backtrack_pos)
            .context("Failed to get index of final step in backtrack")?;
        self.grid[backtrack_pos_idx] = GridCell::Occupied(start_direction, id);

        self.debug_dump();

        return Ok(());
    }

    #[inline]
    pub fn get_cell(&self, pos: GridCellPosition) -> Result<&GridCell> {
        // Unwrap is ok because pos_to_idx does bounds checking
        Ok(self.grid.get(self.pos_to_idx(pos)?).unwrap())
    }

    #[inline]
    pub fn get_cell_mut(&mut self, pos: GridCellPosition) -> Result<&mut GridCell> {
        let idx = self.pos_to_idx(pos)?;
        Ok(self.grid.get_mut(idx).unwrap())
    }

    #[inline]
    fn is_in_bounds(&self, pos: GridCellPosition) -> bool {
        pos.in_bounding_box(&self.current_bounds_min, &self.current_bounds_max)
    }

    #[inline(always)]
    fn pos_to_idx(&self, pos: GridCellPosition) -> Result<usize> {
        if pos.x.0 < 0
            || pos.y < 0
            || pos.z.0 < 0
            || pos.x.0 >= self.size_x
            || pos.y >= self.size_y
            || pos.z.0 >= self.size_z
        {
            Err(RoutingError::OutOfBounds {
                pos,
                bounds: (self.size_x, self.size_y, self.size_z),
            })?
        } else {
            let x = pos.x.0 as usize;
            let y = pos.y as usize;
            let z = pos.z.0 as usize;
            Ok(x + z * self.zsi + y * self.ysi)
        }
    }

    fn is_blocked(&self, pos: GridCellPosition, id: RouteId, illegal_direction: Direction) -> bool {
        match self.get_cell(pos) {
            Ok(cell) => match cell {
                GridCell::Free => false,
                GridCell::Blocked => true,
                GridCell::Occupied(d, s) => *d == illegal_direction || s != &id,
            },
            Err(_) => true,
        }
    }

    fn for_each_neighbor(
        &self,
        pos: GridCellPosition,
        entry_direction: Direction,
        id: RouteId,
        mut f: impl FnMut(GridCellPosition, Direction, StepDirection) -> Result<()>,
    ) -> Result<()> {
        let illegal_direction = entry_direction.mirror();
        for d in ALL_DIRECTIONS {
            let neighbor = pos.offset(d);
            if d == illegal_direction {
                // Can't double back
                debug!(
                    "Skipping neighbors like {} because it would require a direction mirror",
                    neighbor
                );
                continue;
            }
            if self.is_blocked(neighbor, id, illegal_direction) {
                // No possible move in this direction
                debug!(
                    "Skipping neighbors like {} because they are blocked",
                    neighbor
                );
                continue;
            }
            f(neighbor, d, StepDirection::NoStep).context("in-plane direction")?;
        }

        Ok(())
    }

    pub fn rip_up(&mut self, id: RouteId) -> Result<()> {
        // TODO: make this API more efficient?
        for (_, cell) in self.grid.iter_mut().enumerate() {
            match cell {
                GridCell::Occupied(_, i) if *i == id => *cell = GridCell::Free,
                _ => {}
            }
        }

        Ok(())
    }

    fn debug_dump(&self) {
        for y in 0..self.current_bounds_max.y {
            let min_x = std::cmp::max(self.current_bounds_min.x - 2, 0.into());
            let min_z = std::cmp::max(self.current_bounds_min.z - 2, 0.into());
            {
                let mut bufz = String::new();

                for z in min_z.0..self.current_bounds_max.z.0 {
                    bufz.push_str(&format!("{:4} ", z))
                }
                for z in min_z.0..self.current_bounds_max.z.0 {
                    bufz.push_str(&format!("{:3} ", z))
                }
                debug!(" -- y {} {}", y, bufz);
            }

            for x in min_x.0..self.current_bounds_max.x.0 {
                let mut buf_s = String::new();
                let mut buf_c = String::new();
                for z in min_z.0..self.current_bounds_max.z.0 {
                    let pos = GridCellPosition::new(WireCoord(x), y, WireCoord(z));
                    let idx = self.pos_to_idx(pos).unwrap();
                    let score = self.score_grid[idx];
                    if score == std::u32::MAX {
                        buf_s.push_str("x__x ");
                    } else {
                        buf_s.push_str(&format!("{:4} ", score));
                    }
                    match self.grid[idx] {
                        GridCell::Free => buf_c.push_str("FFF "),
                        GridCell::Blocked => buf_c.push_str("BBB "),
                        GridCell::Occupied(d, RouteId(i)) => {
                            let dc = match d {
                                Direction::North => "N",
                                Direction::South => "S",
                                Direction::East => "E",
                                Direction::West => "W",
                                Direction::Up => "U",
                                Direction::Down => "D",
                            };
                            buf_c.push_str(&format!("{}{:2} ", dc, i))
                        }
                    }
                }
                debug!("(x: {:2}) {} {}", x, buf_s, buf_c);
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum RoutingError {
    Unroutable,
    OutOfBounds {
        pos: GridCellPosition,
        bounds: (i32, i32, i32),
    },
}

impl std::error::Error for RoutingError {}

impl Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unroutable => write!(f, "path was unroutable"),
            Self::OutOfBounds {
                pos:
                    GridCellPosition {
                        ref x,
                        ref y,
                        ref z,
                    },
                bounds: (ref bx, ref by, ref bz),
            } => write!(
                f,
                "access out of bounds: ({}, {}, {}) exceeds ({}, {}, {})",
                x.0, y, z.0, bx, by, bz
            ),
        }
    }
}
