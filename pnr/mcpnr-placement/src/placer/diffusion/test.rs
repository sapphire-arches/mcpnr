use super::DiffusionPlacer;
use crate::netlist;
use approx::assert_relative_eq;
use ndarray::s;

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

    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 1, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 2, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 2, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 1, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 2, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 2, 2)], 1.0);
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

    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 1, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 2, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 2, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 1, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 2, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 2, 2)], 1.0);
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

    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 1, 2)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 2, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 2, 2)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 1, 2)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 2, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 2, 2)], 1.0);
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
    assert_relative_eq!(diffuser.density[(1, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 1, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 1, 3)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 2, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 2, 2)], 4.0);
    assert_relative_eq!(diffuser.density[(1, 2, 3)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 3, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(1, 3, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(1, 3, 3)], 1.0);
    assert_relative_eq!(diffuser.density[(2, 1, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 1, 2)], 4.0);
    assert_relative_eq!(diffuser.density[(2, 1, 3)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 2, 1)], 4.0);
    assert_relative_eq!(diffuser.density[(2, 2, 2)], 8.0);
    assert_relative_eq!(diffuser.density[(2, 2, 3)], 4.0);
    assert_relative_eq!(diffuser.density[(2, 3, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(2, 3, 2)], 4.0);
    assert_relative_eq!(diffuser.density[(2, 3, 3)], 2.0);
    assert_relative_eq!(diffuser.density[(3, 1, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(3, 1, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(3, 1, 3)], 1.0);
    assert_relative_eq!(diffuser.density[(3, 2, 1)], 2.0);
    assert_relative_eq!(diffuser.density[(3, 2, 2)], 4.0);
    assert_relative_eq!(diffuser.density[(3, 2, 3)], 2.0);
    assert_relative_eq!(diffuser.density[(3, 3, 1)], 1.0);
    assert_relative_eq!(diffuser.density[(3, 3, 2)], 2.0);
    assert_relative_eq!(diffuser.density[(3, 3, 3)], 1.0);
}

#[test]
fn diffuse_simple() {
    let mut diffuser = test_diffuser();

    diffuser.density[(2, 2, 2)] = 1.0;
    diffuser.step_time(0.01);

    let sum = diffuser
        .density
        .slice(s![1..4usize, 1..4usize, 1..4usize])
        .fold(0.0, |a, b| a + b);

    // Sanity check the total sum, to ensure "conservation of mass"
    assert_relative_eq!(sum, 1.0);

    // 3 layers affected, as so:
    // y ------------------>
    // x ---->  ---->  ---->
    // z 0 0 0  0 b 0  0 0 0
    // | 0 b 0  b a b  0 b 0
    // v 0 0 0  0 b 0  0 0 0              x  y  z
    assert_relative_eq!(diffuser.density[(1, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 1, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 1, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 2, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 2, 2)], 0.005);
    assert_relative_eq!(diffuser.density[(1, 2, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 3, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 3, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(1, 3, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 1, 2)], 0.005);
    assert_relative_eq!(diffuser.density[(2, 1, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 2, 1)], 0.005);
    assert_relative_eq!(diffuser.density[(2, 2, 2)], 0.97);
    assert_relative_eq!(diffuser.density[(2, 2, 3)], 0.005);
    assert_relative_eq!(diffuser.density[(2, 3, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(2, 3, 2)], 0.005);
    assert_relative_eq!(diffuser.density[(2, 3, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 1, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 1, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 1, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 2, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 2, 2)], 0.005);
    assert_relative_eq!(diffuser.density[(3, 2, 3)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 3, 1)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 3, 2)], 0.0);
    assert_relative_eq!(diffuser.density[(3, 3, 3)], 0.0);
}

#[test]
fn vel_simple() {
    let mut diffuser = test_diffuser();

    diffuser.density.iter_mut().for_each(|v| *v = 1.0);
    diffuser.density[(3, 3, 3)] = 2.0;
    diffuser.compute_velocities();

    assert_relative_eq!(diffuser.vel_x[(3, 3, 3)], 0.0);
    assert_relative_eq!(diffuser.vel_x[(2, 3, 3)], 0.5);
    assert_relative_eq!(diffuser.vel_x[(4, 3, 3)], -0.5);

    assert_relative_eq!(diffuser.vel_y[(3, 3, 3)], 0.0);
    assert_relative_eq!(diffuser.vel_y[(3, 2, 3)], 0.5);
    assert_relative_eq!(diffuser.vel_y[(3, 4, 3)], -0.5);

    assert_relative_eq!(diffuser.vel_z[(3, 3, 3)], 0.0);
    assert_relative_eq!(diffuser.vel_z[(3, 3, 2)], 0.5);
    assert_relative_eq!(diffuser.vel_z[(3, 3, 4)], -0.5);
}

#[test]
fn movement_sanity() {
    let mut diffuser = test_diffuser();

    diffuser.vel_x[(2, 2, 2)] = 1.0;
    diffuser.vel_y[(2, 2, 2)] = 1.0;
    diffuser.vel_z[(2, 2, 2)] = 1.0;

    let mut net = netlist!(
        cells: [ mobile_0 => (1, 1, 1); ],
        fixed_cells: [],
        signals: []
    );

    net.cells[0].x = 0.5;
    net.cells[0].y = 0.5;
    net.cells[0].z = 0.5;

    diffuser.move_cells(&mut net, 0.25);

    assert_relative_eq!(net.cells[0].x, 0.53125);
    assert_relative_eq!(net.cells[0].y, 0.53125);
    assert_relative_eq!(net.cells[0].z, 0.53125);
}
