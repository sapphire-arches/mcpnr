use log::info;

use super::*;

fn init(size_x: u32, size_y: u32, size_z: u32) -> DetailRouter {
    let _ = env_logger::builder().is_test(true).try_init();
    DetailRouter::new(size_x, size_y, size_z)
}

#[test]
fn accepts_valid_connectivities() -> Result<()> {
    let mut router = init(5, 5, 5);

    let base_pos = Position::new(2, 2, 2);

    // Planar directions and all step up/downs should work with an empty grid
    info!("Do all clear checks");
    for d in PLANAR_DIRECTIONS {
        info!("Check direction {:?}", d);
        assert!(router.check_connectivity(base_pos, base_pos.offset(d), RouteId(0)));
        assert!(router.check_connectivity(
            base_pos,
            base_pos.offset(d).offset(Direction::Down),
            RouteId(0)
        ));
        assert!(router.check_connectivity(
            base_pos,
            base_pos.offset(d).offset(Direction::Up),
            RouteId(0)
        ));
    }

    // Block above the base position
    *router.get_cell_mut(base_pos.offset(Direction::Up))? = GridCell::Blocked;

    // Planar directions and step down should still work, step down should fail
    info!("Do step up blocked checks");
    for d in PLANAR_DIRECTIONS {
        info!("Check direction {:?}", d);
        assert!(router.check_connectivity(base_pos, base_pos.offset(d), RouteId(0)));
        assert!(router.check_connectivity(
            base_pos,
            base_pos.offset(d).offset(Direction::Down),
            RouteId(0)
        ));
        assert!(!router.check_connectivity(
            base_pos,
            base_pos.offset(d).offset(Direction::Up),
            RouteId(0)
        ));
    }

    // Clear block above base position and block around the plane
    *router.get_cell_mut(base_pos.offset(Direction::Up))? = GridCell::Free;
    for d in PLANAR_DIRECTIONS {
        *router.get_cell_mut(base_pos.offset(d))? = GridCell::Blocked;
    }

    // All connectivity should fail as critical paths to the destionation are blocked
    info!("Do blocked checks");
    for d in PLANAR_DIRECTIONS {
        info!("Check direction {:?}", d);
        assert!(!router.check_connectivity(base_pos, base_pos.offset(d), RouteId(0)));
        assert!(!router.check_connectivity(
            base_pos,
            base_pos.offset(d).offset(Direction::Down),
            RouteId(0)
        ));
        assert!(!router.check_connectivity(
            base_pos,
            base_pos.offset(d).offset(Direction::Up),
            RouteId(0)
        ));
    }

    Ok(())
}
