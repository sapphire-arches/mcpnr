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

fn print_router_grid(router: &Router2D) -> Result<()> {
    for y in 0..router.size_y {
        for x in 0..router.size_x {
            let pos = Position::new(x, y);
            let idx = router.pos_to_idx(pos)?;

            match router.grid[idx] {
                GridCell::Occupied(RouteId(i)) => print!("{} ", i),
                GridCell::Free => print!("x "),
            }
        }
        println!()
    }
    Ok(())
}

#[test]
fn it_can_route_straight_lines() -> Result<()> {
    let mut router = init(1, 3);
    router.route(Position::new(0, 0), Position::new(0, 2), RouteId(1))?;

    assert_eq!(
        router.is_cell_occupied(Position::new(0, 0))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 1))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 2))?,
        Some(RouteId(1))
    );

    Ok(())
}

#[test]
fn crossing_dissimilar_nets_fails() -> Result<()> {
    let mut router = init(3, 3);

    router.route(Position::new(1, 0), Position::new(1, 2), RouteId(1))?;
    print_router_grid(&router)?;

    let err = router.route(Position::new(0, 1), Position::new(2, 1), RouteId(2));
    print_router_grid(&router)?;

    let err = err.expect_err("Routing unexpectedly succeeded");

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

    assert_eq!(
        router.is_cell_occupied(Position::new(1, 0))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(1, 1))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(1, 2))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 1))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(2, 1))?,
        Some(RouteId(1))
    );

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
            Some(RoutingError::Unroutable) => true,
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
    print_router_grid(&router)?;

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
    assert_eq!(c_00, Some(RouteId(1)));
    assert_eq!(c_22, Some(RouteId(1)));

    if let Some(c_20) = c_20 {
        assert_eq!(c_20, RouteId(1));
        // 1 1 1
        // x x 1
        // x x 1
        assert_eq!(c_10, Some(RouteId(1)));
        assert_eq!(c_21, Some(RouteId(1)));
    } else if let Some(c_11) = c_11 {
        assert_eq!(c_11, RouteId(1));
        if let Some(c_10) = c_10 {
            assert_eq!(c_10, RouteId(1));
            // 1 1 x
            // x 1 x
            // x 1 1
            assert_eq!(c_12, Some(RouteId(1)));
        } else if let Some(c_01) = c_01 {
            assert_eq!(c_01, RouteId(1));
            // 1 x x
            // 1 1 1
            // x x 1
            assert_eq!(c_21, Some(RouteId(1)));
        }
    } else if let Some(c_02) = c_02 {
        assert_eq!(c_02, RouteId(1));
        // 1 x x
        // 1 x x
        // 1 1 1
        assert_eq!(c_01, Some(RouteId(1)));
        assert_eq!(c_12, Some(RouteId(1)));
    } else {
        panic!("No valid route detected");
    }

    Ok(())
}

#[test]
fn it_chooses_the_shortest_path() -> Result<()> {
    let mut router = init(3, 4);

    // s x x
    // x 2 x
    // x 2 x
    // x e x

    router.mark_cell_occupied(Position::new(1, 1), RouteId(2))?;
    router.mark_cell_occupied(Position::new(1, 2), RouteId(2))?;

    router.route(Position::new(0, 0), Position::new(1, 3), RouteId(1))?;

    assert_eq!(
        router.is_cell_occupied(Position::new(0, 0))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 1))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 2))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 3))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(1, 3))?,
        Some(RouteId(1))
    );

    Ok(())
}

#[test]
fn it_can_rip_up_tracks() -> Result<()> {
    let mut router = init(1, 3);
    router.route(Position::new(0, 0), Position::new(0, 2), RouteId(1))?;

    assert_eq!(
        router.is_cell_occupied(Position::new(0, 0))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 1))?,
        Some(RouteId(1))
    );
    assert_eq!(
        router.is_cell_occupied(Position::new(0, 2))?,
        Some(RouteId(1))
    );

    router.rip_up(RouteId(1))?;

    assert_eq!(router.is_cell_occupied(Position::new(0, 0))?, None);
    assert_eq!(router.is_cell_occupied(Position::new(0, 1))?, None);
    assert_eq!(router.is_cell_occupied(Position::new(0, 2))?, None);

    Ok(())
}
