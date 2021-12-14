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

    assert_eq!(router.is_cell_occupied(Position::new(0, 0))?, RouteId(1));
    assert_eq!(router.is_cell_occupied(Position::new(0, 1))?, RouteId(1));
    assert_eq!(router.is_cell_occupied(Position::new(0, 2))?, RouteId(1));

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

    assert_eq!(router.is_cell_occupied(Position::new(1, 0))?, RouteId(1));
    assert_eq!(router.is_cell_occupied(Position::new(1, 1))?, RouteId(1));
    assert_eq!(router.is_cell_occupied(Position::new(1, 2))?, RouteId(1));
    assert_eq!(router.is_cell_occupied(Position::new(0, 1))?, RouteId(1));
    assert_eq!(router.is_cell_occupied(Position::new(2, 1))?, RouteId(1));

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
    let mut router = init(3, 3);

    router.route(Position::new(0, 0), Position::new(2, 2), RouteId(1))?;

    // Detect which of the possible paths were taken
    let c_00 = router.is_cell_occupied(Position::new(0, 0))?;
    let c_01 = router.is_cell_occupied(Position::new(0, 1))?;
    let c_02 = router.is_cell_occupied(Position::new(0, 2))?;
    let c_10 = router.is_cell_occupied(Position::new(1, 0))?;
    let c_11 = router.is_cell_occupied(Position::new(1, 1))?;
    let c_12 = router.is_cell_occupied(Position::new(1, 2))?;
    let c_20 = router.is_cell_occupied(Position::new(2, 0))?;
    let c_21 = router.is_cell_occupied(Position::new(2, 1))?;
    let c_22 = router.is_cell_occupied(Position::new(2, 2))?;

    // Check endpoints
    assert_eq!(c_00, RouteId(1));
    assert_eq!(c_22, RouteId(1));

    if c_02 == RouteId(1) {
        // 1 1 1
        // x x 1
        // x x 1
        assert_eq!(c_10, RouteId(1));
        assert_eq!(c_21, RouteId(1));
    } else if c_11 == RouteId(1) {
        if c_10 == RouteId(1) {
            // 1 1 x
            // x 1 x
            // x 1 1
            assert_eq!(c_12, RouteId(1));
        } else if c_01 == RouteId(1) {
            // 1 x x
            // 1 1 1
            // x x 1
            assert_eq!(c_21, RouteId(1));
        }
    } else if c_02 == RouteId(1) {
        // 1 x x
        // 1 x x
        // 1 1 1
        assert_eq!(c_01, RouteId(1));
        assert_eq!(c_12, RouteId(1));
    } else {
        panic!("No valid route detected");
    }

    Ok(())
}
