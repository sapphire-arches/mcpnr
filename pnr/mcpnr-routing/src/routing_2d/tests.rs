use anyhow::Error;

use super::*;

fn init(size_x: u32, size_y: u32) -> Router2D {
    Router2D::new(size_x, size_y)
}

fn check_chain_for_error(
    err: Error,
    predicate: impl FnMut(&&(dyn std::error::Error + 'static)) -> bool,
) -> bool {
    err.chain().find(predicate).is_some()
}

#[test]
fn it_can_route_straight_lines() -> Result<()> {
    let mut router = init(1, 3);
    router.route(Position::new(0, 0), Position::new(0, 2), RouteId(1))?;
    Ok(())
}

#[test]
fn crossing_dissimilar_nets_fails() -> Result<()> {
    let mut router = init(3, 3);

    router.route(Position::new(1, 0), Position::new(1, 2), RouteId(1))?;

    let err = router
        .route(Position::new(0, 1), Position::new(2, 1), RouteId(2))
        .expect_err("Routing unexpectedly succeeded");

    assert!(check_chain_for_error(err, |e| {
        match e.downcast_ref() {
            Some(RoutingError::Unroutable) => true,
            _ => false,
        }
    }));

    Ok(())
}

#[test]
fn crossing_similar_net_succeeds() -> Result<()> {
    let mut router = init(3, 3);
    router.route(Position::new(1, 0), Position::new(1, 2), RouteId(1))?;
    router.route(Position::new(0, 1), Position::new(2, 1), RouteId(1))?;

    Ok(())
}

#[test]
fn routing_to_out_of_bounds_point_fails() -> Result<()> {
    let mut router = init(1, 1);
    let err = router
        .route(Position::new(0, 0), Position::new(0, 1), RouteId(1))
        .expect_err("Routing unexpectedly succeeded");

    assert!(check_chain_for_error(err, |e| {
        match e.downcast_ref() {
            Some(RoutingError::OutOfBounds {
                pos: Position { x: 0, y: 1 },
                bounds: (1, 1),
            }) => true,
            _ => false,
        }
    }));

    Ok(())
}

#[test]
fn it_can_avoid_obstacles() -> Result<()> {
    let mut router = init(3, 3);

    // setup world like:
    //   x 0 1 2
    // y   -----
    // 0 | x x x
    // 1 | x 0 x
    // 2 | x 0 x
    router.mark_cell_occupied(Position::new(1, 1), RouteId(0))?;
    router.mark_cell_occupied(Position::new(1, 2), RouteId(0))?;

    // Do route with RouteId(1)
    router.route(Position::new(0, 1), Position::new(2, 2), RouteId(1))?;

    Ok(())
}

#[test]
fn it_can_choose_among_identical_paths() -> Result<()> {
    let mut router = init(10, 10);

    router.route(Position::new(0, 0), Position::new(9, 9), RouteId(1))
}
