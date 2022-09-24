use super::DiffusionPlacer;
use crate::netlist;
use approx::assert_relative_eq;

fn test_diffuser() -> DiffusionPlacer {
    DiffusionPlacer::new(16, 16, 16, 2)
}

#[test]
fn splat_aligned() {
    let netlist = netlist!(
        cells: [
        ],
        fixed_cells: [
            fixed_0 => (0, 0, 0), (1, 1, 1);
            fixed_1 => (2, 2, 2), (1, 1, 1);
        ],
        signals: [
        ]
    );

    let mut diffuser = test_diffuser();

    diffuser.splat(&netlist);

    assert_relative_eq!(diffuser.density[(0, 0, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 0, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(0, 1, 0)], 0.0);
    assert_relative_eq!(diffuser.density[(0, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 0, 0)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 0, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 1, 0)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
}

#[test]
fn splat_unaligned() {
    let netlist = netlist!(
        cells: [
        ],
        fixed_cells: [
            fixed_0 => (1, 1, 1), (1, 1, 1);
            fixed_1 => (3, 3, 3), (1, 1, 1);
        ],
        signals: [
        ]
    );

    let mut diffuser = test_diffuser();

    diffuser.splat(&netlist);

    assert_relative_eq!(diffuser.density[(0, 0, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 0, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(0, 1, 0)], 0.0);
    assert_relative_eq!(diffuser.density[(0, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 0, 0)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 0, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 1, 0)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
}

#[test]
fn splat_multiple_regions() {
    let netlist = netlist!(
        cells: [
        ],
        fixed_cells: [
            fixed_0 => (1, 1, 1), (2, 2, 2);
        ],
        signals: [
        ]
    );

    let mut diffuser = test_diffuser();

    diffuser.splat(&netlist);

    assert_relative_eq!(diffuser.density[(0, 0, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 0, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 1, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 0, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 0, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 1, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
}

#[test]
fn splat_solid() {
    let netlist = netlist!(
        cells: [
        ],
        fixed_cells: [
            fixed_0 => (1, 1, 1), (4, 4, 4);
        ],
        signals: [
        ]
    );

    let mut diffuser = test_diffuser();

    diffuser.splat(&netlist);

    // 3 layers affected, as so:
    // y ------------------>
    // x ---->  ---->  ---->
    // z 1 2 1  2 4 2  1 2 1
    // | 2 4 2  4 8 4  2 4 2
    // v 1 2 1  2 4 2  1 2 1
    assert_relative_eq!(diffuser.density[(0, 0, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 0, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(0, 0, 2)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 1, 0)], 2.0);
    assert_relative_eq!(diffuser.density[(0, 1, 1)], 4.0);
    assert_relative_eq!(diffuser.density[(0, 1, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(0, 2, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(0, 2, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(0, 2, 2)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 0, 0)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 0, 1)], 4.0);
    assert_relative_eq!(diffuser.density[(1, 0, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 1, 0)], 4.0);
    assert_relative_eq!(diffuser.density[(1, 1, 1)], 8.0);
    assert_relative_eq!(diffuser.density[(1, 1, 2)], 4.0);
    assert_relative_eq!(diffuser.density[(1, 2, 0)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 2, 1)], 4.0);
    assert_relative_eq!(diffuser.density[(1, 2, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 0, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 0, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 0, 2)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 1, 0)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 1, 1)], 4.0);
    assert_relative_eq!(diffuser.density[(2, 1, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 2, 0)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 2, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 2, 2)], 1.0);
}
