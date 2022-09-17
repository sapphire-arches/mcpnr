use super::*;
use crate::placement_cell::PlacementCell;
use std::collections::{hash_map::Entry, HashMap};

fn make_netlist<'a>(
    mobile_cells: impl Iterator<Item = &'a (&'static str, (u32, u32, u32), (u32, u32, u32))>,
    fixed_cells: impl Iterator<Item = &'a (&'static str, (u32, u32, u32), (u32, u32, u32))>,
    signal_specs: impl Iterator<Item = &'a &'a [&'static str]>,
) -> NetlistHypergraph {
    let mut cells = Vec::new();
    let mut cell_indicies: HashMap<&'static str, usize> = Default::default();

    for (name, (x, y, z), (sx, sy, sz)) in mobile_cells {
        let cell_idx = cells.len();
        cells.push(PlacementCell {
            x: *x as f32,
            y: *y as f32,
            z: *z as f32,
            sx: *sx as f32,
            sy: *sy as f32,
            sz: *sz as f32,
            pos_locked: false,
        });

        match cell_indicies.entry(name) {
            Entry::Occupied(_) => panic!("Duplicate cell {} specified in test", name),
            Entry::Vacant(v) => v.insert(cell_idx),
        };
    }

    for (name, (x, y, z), (sx, sy, sz)) in fixed_cells {
        let cell_idx = cells.len();
        cells.push(PlacementCell {
            x: *x as f32,
            y: *y as f32,
            z: *z as f32,
            sx: *sx as f32,
            sy: *sy as f32,
            sz: *sz as f32,
            pos_locked: true,
        });

        match cell_indicies.entry(name) {
            Entry::Occupied(_) => panic!("Duplicate fixed cell {} specified in test", name),
            Entry::Vacant(v) => v.insert(cell_idx),
        };
    }

    let signals = signal_specs
        .map(|spec| {
            let connected_cells: Vec<_> =
                spec.into_iter().map(|name| cell_indicies[name]).collect();

            Signal {
                moveable_cells: connected_cells
                    .iter()
                    .filter(|idx| !cells[**idx].pos_locked)
                    .count(),
                connected_cells,
            }
        })
        .collect();

    NetlistHypergraph::test_new(cells, signals)
}

macro_rules! netlist {
    (
        cells : [
           $($name:ident =>
                ($x:expr, $y:expr, $z:expr),
                ($sx:expr, $sy:expr, $sz:expr);)*
        ],
        fixed_cells : [
           $($f_name:ident =>
                ($f_x:expr, $f_y:expr, $f_z:expr),
                ($f_sx:expr, $f_sy:expr, $f_sz:expr);)*
        ],
        signals : [
            $([$($cell:ident),*]),*
        ]
    ) => {{
        let cells: &[(&'static str, (u32, u32, u32), (u32, u32, u32))] = &[
            $(
                (stringify!($name), ($x, $y, $z), ($sx, $sy, $sz))
            ),*
        ];
        let fixed_cells: &[(&'static str, (u32, u32, u32), (u32, u32, u32))] = &[
            $(
                (stringify!($f_name), ($f_x, $f_y, $f_z), ($f_sx, $f_sy, $f_sz))
            ),*
        ];

        let signals: &[&[&'static str]] = &[
            $( &[
                    $(stringify!($cell)),*
            ] ),*
        ];

        make_netlist(cells.into_iter(), fixed_cells.into_iter(), signals.into_iter())
    }};
}

#[test]
fn simple_2fixed_1mobile() {
    tracing_subscriber::fmt::init();

    let mut net = netlist![
        cells: [
            mobile_0 => (0, 0, 0), (1, 1, 1);
        ],
        fixed_cells: [
            fixed_0 => (0, 0, 0), (1, 1, 1);
            fixed_1 => (2, 2, 2), (1, 1, 1);
        ],
        signals: [
            [mobile_0, fixed_0],
            [mobile_0, fixed_1]
        ]
    ];

    let mut strategy = Clique::new();
    strategy.execute(&mut net).expect("Strategy success");

    assert_eq!(net.cells[0].x, 1.0);
    assert_eq!(net.cells[0].y, 1.0);
    assert_eq!(net.cells[0].z, 1.0);
}
