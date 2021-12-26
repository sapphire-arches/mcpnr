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
        driver: GridCellPosition,
        driver_direction: Direction,
        sink: GridCellPosition,
        sink_direction: Direction,
        id: RouteId,
    ) -> Result<()> {
        // TODO: implement A* by adding an estimate to this
        #[derive(PartialEq, Eq)]
        struct RouteQueueItem {
            cost: u32,
            // TODO: Use routing grid indicies instead of positions
            pos: GridCellPosition,

            illegal_direction: Direction,
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

        log::info!("Begin routing net {:?} from {} to {}", id, driver, sink);

        // TODO: Use temporary just-right-sized routing grid instead of the full one
        for score in self.score_grid.iter_mut() {
            *score = std::u32::MAX;
        }

        self.current_bounds_min = GridCellPosition::new(
            std::cmp::max(std::cmp::min(driver.x, sink.x) - 2, WireCoord(0)),
            std::cmp::max(std::cmp::min(driver.y, sink.y) - 2, 0),
            std::cmp::max(std::cmp::min(driver.z, sink.z) - 2, WireCoord(0)),
        );
        self.current_bounds_max = GridCellPosition::new(
            std::cmp::min(std::cmp::max(driver.x, sink.x) + 2, self.size_x.into()),
            std::cmp::min(std::cmp::max(driver.y, sink.y) + 2, self.size_y),
            std::cmp::min(std::cmp::max(driver.z, sink.z) + 2, self.size_z.into()),
        );

        // Start the driver one cell away in the direction that will cause entry into the driver
        let driver = driver.offset(driver_direction.mirror());
        // Start the sink one cell away in the direction the pin requests.
        let sink = sink.offset(sink_direction);

        // Immediately mark the driver position as occupied and facing in the appropriate
        // direction. This helps terminate the search early, and someone needs to do it so it may
        // as well be us.
        *self
            .get_cell_mut(driver)
            .context("Driver pin offset mark")? = GridCell::Occupied(driver_direction, id);

        match self.get_cell(driver)? {
            GridCell::Free => {}
            GridCell::Blocked => {
                self.debug_dump();
                return Err(RoutingError::Unroutable)
                    .context("Driver pin points directly at an unroutable cell");
            }
            GridCell::Occupied(_, i) => {
                if *i != id {
                    return Err(RoutingError::Unroutable).context(anyhow!(
                        "Driver pin points directly at a cell already occupied by route {:?}",
                        id
                    ));
                }
            }
        };

        match self.get_cell(driver)? {
            GridCell::Free => {}
            GridCell::Blocked => {
                self.debug_dump();
                return Err(RoutingError::Unroutable)
                    .context("Sink pin points directly at an unroutable cell");
            }
            GridCell::Occupied(_, i) => {
                if *i != id {
                    return Err(RoutingError::Unroutable).context(anyhow!(
                        "Sink pin points directly at a cell already occupied by route {:?}",
                        id
                    ));
                }
            }
        };

        let mut routing_queue = BinaryHeap::new();

        // Start at the sink and iterate until we either bottom out (explored everything and found
        // no route) or we find our way to something already owned by our net.
        //
        // We block movement back to the original sink because that's already marked and would
        // cause an erronious early-out
        routing_queue.push(RouteQueueItem {
            cost: 0,
            pos: sink,
            illegal_direction: sink_direction.mirror(),
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
            let item_grid = self.grid[idx];

            if let GridCell::Occupied(_, occupied_id) = item_grid {
                if occupied_id == id {
                    return self.do_backtrack(sink, item.pos, item.illegal_direction, id);
                }
            }
            self.for_each_neighbor(
                item.pos,
                item.illegal_direction,
                id,
                |neighbor, move_direction| -> Result<()> {
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
                                    25
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
                        + match move_direction {
                            Direction::Up | Direction::Down => 1000,
                            _ => 0,
                        };
                    if cost < self.score_grid[idx] {
                        debug!("Pushing item for {} (cost: {})", neighbor, cost);
                        routing_queue.push(RouteQueueItem {
                            cost,
                            pos: neighbor,
                            illegal_direction: move_direction.mirror(),
                        })
                    }

                    Ok(())
                },
            )
            .context("Forward search neighbors")?
        }

        debug!(
            "Failed to route net {:?} from {:?} to {:?}",
            id, sink, driver
        );
        self.debug_dump();
        Err(RoutingError::Unroutable)?
    }

    fn do_backtrack(
        &mut self,
        sink: GridCellPosition,
        first_net_touch: GridCellPosition,
        start_direction: Direction,
        id: RouteId,
    ) -> Result<()> {
        debug!("Begin backtrack");

        let mut min_direction = start_direction.mirror();
        let mut min_position = first_net_touch;
        let min_pos_idx = self.pos_to_idx(min_position)?;
        let mut min_cost = self.score_grid[min_pos_idx];
        let mut last_min_pos = min_position;

        while min_position != sink {
            self.for_each_neighbor(
                min_position,
                min_direction,
                id,
                |neighbor, move_direction| -> Result<()> {
                    let neighbor_idx = self.pos_to_idx(neighbor)?;
                    if self.score_grid[neighbor_idx] < min_cost {
                        min_cost = self.score_grid[neighbor_idx];

                        min_position = neighbor;
                        // Mirror the direction because the step taken here moves us *towards* the
                        // sink, while we want to record the path *away* from the sink.
                        min_direction = move_direction.mirror();
                    }
                    Ok(())
                },
            )?;

            let min_pos_idx = self.pos_to_idx(min_position)?;
            self.grid[min_pos_idx] = GridCell::Occupied(min_direction, id);

            if last_min_pos == min_position {
                self.debug_dump();
                return Err(RoutingError::Unroutable).context(anyhow!(
                    "Backtrack for net {:?} did not make progress at {}",
                    id,
                    min_position
                ));
                // panic!(
                //     "Backtrack for net {:?} did not make progress at {}",
                //     id, min_position
                // );
            }
            last_min_pos = min_position;
        }

        Ok(())
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

    fn is_blocked(&self, pos: GridCellPosition, id: RouteId) -> bool {
        match self.get_cell(pos) {
            Ok(cell) => match cell {
                GridCell::Free => false,
                GridCell::Blocked => {
                    debug!("Cell {} is directly blocked", pos);
                    true
                }
                GridCell::Occupied(_, s) => {
                    if s != &id {
                        debug!("Cell {} is allready occupied by net {:?}", pos, s);
                        true
                    } else {
                        false
                    }
                }
            },
            Err(_) => true,
        }
    }

    fn for_each_neighbor(
        &self,
        pos: GridCellPosition,
        sink_direction: Direction,
        id: RouteId,
        mut f: impl FnMut(GridCellPosition, Direction) -> Result<()>,
    ) -> Result<()> {
        let illegal_direction = sink_direction;
        for d in ALL_DIRECTIONS {
            let neighbor = pos.offset(d);
            if d == illegal_direction {
                // Can't double back
                debug!(
                    "Skipping neighbors like {} because it would move closer to the sink",
                    neighbor
                );
                continue;
            }
            if self.is_blocked(neighbor, id) {
                // No possible move in this direction
                debug!(
                    "Skipping neighbors like {} because they are blocked",
                    neighbor
                );
                continue;
            }
            f(neighbor, d).context("in-plane direction")?;
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
