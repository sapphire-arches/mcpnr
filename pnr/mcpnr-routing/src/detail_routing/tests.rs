use log::info;

use super::*;

fn init(size_x: u32, size_y: u32, size_z: u32) -> DetailRouter {
    let _ = env_logger::builder().is_test(true).try_init();
    DetailRouter::new(size_x, size_y, size_z)
}

fn assert_connected(
    router: &DetailRouter,
    driver: GridCellPosition,
    sink: GridCellPosition,
    sink_direction: Direction,
    route: RouteId,
) -> Result<Vec<GridCellPosition>> {
    info!("Post-route debug dump");
    router.debug_dump();

    let mut pathway = Vec::new();
    let mut pos = sink.offset(sink_direction);
    while pos != driver {
        ensure!(
            !pathway.contains(&pos),
            "Pathway loop detected: {:?} in {:?}",
            pos,
            pathway
        );

        if let GridCell::Occupied(d, grid_route) = router.get_cell(pos)? {
            ensure!(*grid_route == route, "Grid pointed to a different route");
            pathway.push(pos);
            pos = pos.offset(*d);
        } else {
            bail!("Grid pointed to something other than another occupied cell");
        }
    }

    Ok(pathway)
}

fn test_routing_and_suffixes(
    router: &mut DetailRouter,
    driver: GridCellPosition,
    driver_direction: Direction,
    sink: GridCellPosition,
    sink_direction: Direction,
    route: RouteId,
) -> Result<()> {
    router.route(driver, driver_direction, sink, sink_direction, route)?;

    let pathway = assert_connected(router, driver, sink, sink_direction, route)?;

    info!("Testing removal along pathway {:?}", pathway);

    for i in 2..pathway.len() {
        for j in 1..i {
            *router.get_cell_mut(pathway[j])? = GridCell::Free;
        }

        info!("Pre-route debug dump");
        router.debug_dump();

        router.route(driver, driver_direction, sink, sink_direction, route)?;

        let _ = assert_connected(router, driver, sink, sink_direction, route)?;
    }

    Ok(())
}

#[test]
pub fn it_can_route_straight_lines() -> Result<()> {
    let mut router = init(5, 5, 5);

    let drivers: [(GridCellPosition, Direction, RouteId); 4] = [
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::North,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 4.into()),
            Direction::South,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::West,
            RouteId(0),
        ),
        (
            GridCellPosition::new(4.into(), 0, 0.into()),
            Direction::East,
            RouteId(0),
        ),
    ];

    let sinks: [(GridCellPosition, Direction, RouteId); 4] = [
        (
            GridCellPosition::new(0.into(), 0, 4.into()),
            Direction::North,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::South,
            RouteId(0),
        ),
        (
            GridCellPosition::new(4.into(), 0, 0.into()),
            Direction::West,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::East,
            RouteId(0),
        ),
    ];

    for i in 0..sinks.len() {
        router.rip_up(RouteId(0))?;

        let driver = drivers[i];
        let sink = sinks[i];

        *router.get_cell_mut(driver.0)? = GridCell::Blocked;
        *router.get_cell_mut(sink.0)? = GridCell::Blocked;

        test_routing_and_suffixes(&mut router, driver.0, driver.1, sink.0, sink.1, RouteId(0))?;
    }

    Ok(())
}

#[test]
pub fn it_can_route_across_layers() -> Result<()> {
    let mut router = init(5, 5, 5);

    let drivers: [(GridCellPosition, Direction, RouteId); 4] = [
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::North,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 4.into()),
            Direction::South,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::West,
            RouteId(0),
        ),
        (
            GridCellPosition::new(4.into(), 0, 0.into()),
            Direction::East,
            RouteId(0),
        ),
    ];

    let sinks: [(GridCellPosition, Direction, RouteId); 4] = [
        (
            GridCellPosition::new(0.into(), 0, 4.into()),
            Direction::North,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::South,
            RouteId(0),
        ),
        (
            GridCellPosition::new(4.into(), 0, 0.into()),
            Direction::West,
            RouteId(0),
        ),
        (
            GridCellPosition::new(0.into(), 0, 0.into()),
            Direction::East,
            RouteId(0),
        ),
    ];

    // Add some hills.
    *router.get_cell_mut(GridCellPosition::new(2.into(), 0, 0.into()))? =
        GridCell::Occupied(Direction::Up, RouteId(2));
    *router.get_cell_mut(GridCellPosition::new(2.into(), 0, 4.into()))? =
        GridCell::Occupied(Direction::Up, RouteId(2));
    *router.get_cell_mut(GridCellPosition::new(0.into(), 0, 2.into()))? =
        GridCell::Occupied(Direction::Up, RouteId(2));
    *router.get_cell_mut(GridCellPosition::new(4.into(), 0, 2.into()))? =
        GridCell::Occupied(Direction::Up, RouteId(2));

    for x in 1..=3 {
        for z in 1..=3 {
            *router.get_cell_mut(GridCellPosition::new(x.into(), 0, z.into()))? =
                GridCell::Occupied(Direction::Up, RouteId(2));
        }
    }

    for i in 0..sinks.len() {
        router.rip_up(RouteId(0))?;

        let driver = drivers[i];
        let sink = sinks[i];

        *router.get_cell_mut(driver.0)? = GridCell::Blocked;
        *router.get_cell_mut(sink.0)? = GridCell::Blocked;

        test_routing_and_suffixes(&mut router, driver.0, driver.1, sink.0, sink.1, RouteId(0))?;
    }

    Ok(())
}
