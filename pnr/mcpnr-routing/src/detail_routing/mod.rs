use crate::RouteId;
use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use std::{collections::BinaryHeap, fmt::Display};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

impl Position {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub fn in_bounding_box(&self, min: &Self, max: &Self) -> bool {
        let x = min.x <= self.x && self.x < max.x;
        let y = min.y <= self.y && self.y < max.y;
        let z = min.z <= self.z && self.z < max.z;

        x && y && z
    }

    pub fn offset(&self, d: Direction) -> Self {
        d.offset_position_by(self)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GridCell {
    /// Completely free
    Free,
    /// Blocked by something (e.g. part of the guts of a cell
    Blocked,
    /// Occupied by a net with the given RouteId
    Occupied(RouteId),
    /// Claimed by a net with the given RouteId (not directly on the route, but required to remain
    /// clear of other net routes to avoid DRC errors
    Claimed(RouteId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
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

impl Direction {
    fn offset_position_by(self, p: &Position) -> Position {
        match self {
            Direction::North => Position::new(p.x, p.y, p.z - 1),
            Direction::South => Position::new(p.x, p.y, p.z + 1),
            Direction::East => Position::new(p.x + 1, p.y, p.z),
            Direction::West => Position::new(p.x - 1, p.y, p.z),
            Direction::Up => Position::new(p.x, p.y + 1, p.z),
            Direction::Down => Position::new(p.x, p.y - 1, p.z),
        }
    }
}

const PLANAR_DIRECTIONS: [Direction; 4] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
];

#[allow(unused)]
const ALL_DIRECTIONS: [Direction; 6] = [
    Direction::North,
    Direction::South,
    Direction::East,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

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

    current_bounds_min: Position,
    current_bounds_max: Position,
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

            current_bounds_min: Position::new(0, 0, 0),
            current_bounds_max: Position::new(0, 0, 0),
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

        self.current_bounds_min = Position::new(
            std::cmp::max(std::cmp::min(start.x, end.x) - 2, 0),
            std::cmp::max(std::cmp::min(start.y, end.y) - 2, 1),
            std::cmp::max(std::cmp::min(start.z, end.z) - 2, 0),
        );
        self.current_bounds_max = Position::new(
            std::cmp::max(start.x, end.x) + 2,
            std::cmp::max(start.y, end.y) + 2,
            std::cmp::max(start.z, end.z) + 2,
        );

        // Immediately claim the pins
        match self.get_cell_mut(start.offset(Direction::Down))? {
            v @ GridCell::Free => *v = GridCell::Claimed(id),
            v @ GridCell::Blocked => *v = GridCell::Claimed(id),
            GridCell::Occupied(net) => {
                return Err(RoutingError::Unroutable).with_context(|| {
                    anyhow!(
                        "Support for start at {} is occupied by route {:?}",
                        start,
                        net
                    )
                })
            }
            GridCell::Claimed(claimer) if claimer == &id => {}
            GridCell::Claimed(claimer) => {
                return Err(RoutingError::Unroutable).with_context(|| {
                    anyhow!(
                        "Support for start at {} is occupied by route {:?}",
                        start,
                        claimer
                    )
                })
            }
        }

        match self.get_cell_mut(end.offset(Direction::Down))? {
            v @ GridCell::Free => *v = GridCell::Claimed(id),
            v @ GridCell::Blocked => *v = GridCell::Claimed(id),
            GridCell::Occupied(net) => {
                return Err(RoutingError::Unroutable).with_context(|| {
                    anyhow!("Support for end at {} is occupied by route {:?}", end, net)
                })
            }
            GridCell::Claimed(claimer) if claimer == &id => {}
            GridCell::Claimed(claimer) => {
                return Err(RoutingError::Unroutable).with_context(|| {
                    anyhow!(
                        "Support for end at {} is occupied by route {:?}",
                        end,
                        claimer
                    )
                })
            }
        }

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
                let mut min_pos = backtrack_pos;
                let mut min_direction = Direction::North;
                let mut min_is_step = StepDirection::NoStep;
                let mut last_backtrack_pos = Position::new(0, 0, 0);

                while backtrack_pos != start {
                    let backtrack_pos_idx = self
                        .pos_to_idx(backtrack_pos)
                        .context("Failed to get index for backtrack start")?;
                    debug!("Mark occupied {:?}", backtrack_pos);
                    self.mark_occupied(backtrack_pos_idx, id);

                    if backtrack_pos == last_backtrack_pos {
                        info!(
                            "Bounds: {} {} for {:?}",
                            self.current_bounds_min, self.current_bounds_max, id
                        );
                        for y in 0..self.current_bounds_max.y {
                            {
                                let mut bufz = String::new();

                                for z in 0..self.current_bounds_max.z {
                                    bufz.push_str(&format!("{:4} ", z))
                                }
                                for z in 0..self.current_bounds_max.z {
                                    bufz.push_str(&format!("{:3} ", z))
                                }
                                info!(" -- y {} {}", y, bufz);
                            }

                            for x in 0..self.current_bounds_max.x {
                                let mut buf_s = String::new();
                                let mut buf_c = String::new();
                                for z in 0..self.current_bounds_max.z {
                                    let pos = Position::new(x, y, z);
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
                                        GridCell::Occupied(RouteId(i)) => {
                                            buf_c.push_str(&format!("O{:2} ", i))
                                        }
                                        GridCell::Claimed(RouteId(i)) => {
                                            buf_c.push_str(&format!("C{:2} ", i))
                                        }
                                    }
                                }
                                info!("(x: {:2}) {} {}", x, buf_s, buf_c);
                            }
                        }

                        panic!()
                    }

                    let mut min = self.score_grid[backtrack_pos_idx];
                    self.for_each_neighbor(backtrack_pos, |neighbor, direction, is_step| {
                        debug!("  Consider neighbor {}", neighbor);
                        if !self.is_in_bounds(neighbor) {
                            debug!(
                                "  Discard neighbor {} because it is out of bounds",
                                neighbor
                            );
                            return Ok(());
                        }
                        if !self.check_connectivity(neighbor, backtrack_pos, id) {
                            debug!("  Discard neighbor {} because it is unroutable", neighbor);
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
                            min_is_step = is_step;
                        }

                        Ok(())
                    })?;

                    last_backtrack_pos = backtrack_pos;
                    backtrack_pos = min_pos;
                }

                let backtrack_pos_idx = self
                    .pos_to_idx(backtrack_pos)
                    .context("Failed to get index of final step in backtrack")?;
                self.grid[backtrack_pos_idx] = GridCell::Occupied(id);

                return Ok(());
            } else {
                self.for_each_neighbor(item.pos, |neighbor, direction, is_step| {
                    // Skip neighbors that leave the bounds of what we care about
                    if !self.is_in_bounds(neighbor) {
                        debug!("Skipping {} because it leaves bounding box", neighbor);
                        return Ok(());
                    }
                    // Skip neighbors with invalid connectivity
                    if !self.check_connectivity(item.pos, neighbor, id) {
                        // Skip this cell because we can't route through it, but don't error
                        debug!(
                            "Skipping {} because we can't route to it from {}",
                            neighbor, item.pos
                        );
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
                            debug!("Skipping {} because we can't route through it", neighbor);
                            return Ok(());
                        }
                        - if direction == item.direction_entry {
                            49
                        } else {
                            0
                        }
                        + match is_step {
                            StepDirection::StepUp => 10,
                            StepDirection::StepDown => 10,
                            StepDirection::NoStep => 0,
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

    #[inline]
    fn is_in_bounds(&self, pos: Position) -> bool {
        pos.in_bounding_box(&self.current_bounds_min, &self.current_bounds_max)
    }

    fn mark_occupied(&mut self, base_idx: usize, route: RouteId) {
        fn safe_set(cell: &mut GridCell, route: GridCell) {
            if *cell == GridCell::Free {
                *cell = route
            }
        }

        /*
        self.grid
            .get_mut(base_idx + 1)
            .map(|v| safe_set(v, GridCell::Claimed(route)));
        self.grid
            .get_mut(base_idx - 1)
            .map(|v| safe_set(v, GridCell::Claimed(route)));
        self.grid
            .get_mut(base_idx + self.zsi)
            .map(|v| safe_set(v, GridCell::Claimed(route)));
        self.grid
            .get_mut(base_idx - self.zsi)
            .map(|v| safe_set(v, GridCell::Claimed(route)));
        */

        self.grid
            .get_mut(base_idx + self.ysi)
            .map(|v| safe_set(v, GridCell::Claimed(route)));
        self.grid
            .get_mut(base_idx - self.ysi)
            .map(|v| safe_set(v, GridCell::Claimed(route)));

        self.grid
            .get_mut(base_idx)
            .map(|v| *v = GridCell::Occupied(route));
    }

    #[inline(always)]
    fn pos_to_idx(&self, pos: Position) -> Result<usize> {
        if pos.x < 0
            || pos.y < 0
            || pos.z < 0
            || pos.x >= self.size_x
            || pos.y >= self.size_y
            || pos.z >= self.size_z
        {
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

    fn check_connectivity(&self, src: Position, dst: Position, route: RouteId) -> bool {
        let delta_x = dst.x - src.x;
        let delta_y = dst.y - src.y;
        let delta_z = dst.z - src.z;

        if !self.is_in_bounds(src) {
            return false;
        }

        let grid_at_dest = self.get_cell(dst).map(|v| *v).unwrap_or(GridCell::Blocked);
        let dest_is_clear =
            grid_at_dest == GridCell::Free || grid_at_dest == GridCell::Occupied(route);

        // check if destination is too close to another line
        let dest_will_not_interfere = !PLANAR_DIRECTIONS.iter().all(|d| {
            let cell = self
                .get_cell(dst.offset(*d))
                .map(|v| *v)
                .unwrap_or(GridCell::Blocked);

            cell == GridCell::Free
                || cell == GridCell::Occupied(route)
                || cell == GridCell::Claimed(route)
        });

        let grid_below_dest = self
            .get_cell(dst.offset(Direction::Down))
            .map(|v| *v)
            .unwrap_or(GridCell::Blocked);
        let grid_below_dest_is_support =
            grid_below_dest == GridCell::Free || grid_below_dest == GridCell::Claimed(route);

        let grid_above_src = self
            .get_cell(src.offset(Direction::Up))
            .map(|v| *v)
            .unwrap_or(GridCell::Blocked);

        let grid_above_src_is_free =
            grid_above_src == GridCell::Free || grid_above_src == GridCell::Claimed(route);

        let grid_above_dst = self
            .get_cell(dst.offset(Direction::Up))
            .map(|v| *v)
            .unwrap_or(GridCell::Blocked);
        let grid_above_dst_is_free =
            grid_above_dst == GridCell::Free || grid_above_dst == GridCell::Claimed(route);

        debug!(
            "Connectivity analysis from {} to {} says {:?}->{} {:?}->{} {:?}->{} {:?}->{}",
            src,
            dst,
            grid_below_dest,
            grid_below_dest_is_support,
            grid_at_dest,
            dest_is_clear,
            grid_above_src,
            grid_above_src_is_free,
            grid_above_dst,
            grid_above_dst_is_free
        );

        grid_below_dest_is_support
            && dest_is_clear
            && match (delta_x, delta_y, delta_z) {
                // Simple cases: traversing in-plane is OK as long as the cell below the
                // destination is usable and the destionation is clear
                (-1, 0, 0) => true,
                (1, 0, 0) => true,
                (0, 0, -1) => true,
                (0, 0, 1) => true,
                // Step-up cases. Required grid above source as well as the cell below destination
                (-1, 1, 0) => grid_above_src_is_free,
                (1, 1, 0) => grid_above_src_is_free,
                (0, 1, -1) => grid_above_src_is_free,
                (0, 1, 1) => grid_above_src_is_free,
                // Step-down cases. Required grid above source as well as the cell below destination
                (-1, -1, 0) => grid_above_dst_is_free,
                (1, -1, 0) => grid_above_dst_is_free,
                (0, -1, -1) => grid_above_dst_is_free,
                (0, -1, 1) => grid_above_dst_is_free,
                // Deltas are out of range
                _ => false,
            }
    }

    fn for_each_neighbor(
        &self,
        pos: Position,
        mut f: impl FnMut(Position, Direction, StepDirection) -> Result<()>,
    ) -> Result<()> {
        for d in PLANAR_DIRECTIONS {
            f(pos.offset(d), d, StepDirection::NoStep)?;
            f(
                pos.offset(d).offset(Direction::Up),
                d,
                StepDirection::StepUp,
            )?;
            f(
                pos.offset(d).offset(Direction::Down),
                d,
                StepDirection::StepDown,
            )?;
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum RoutingError {
    Unroutable,
    OutOfBounds {
        pos: Position,
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
