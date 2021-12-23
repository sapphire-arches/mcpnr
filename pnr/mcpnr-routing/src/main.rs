mod detail_routing;
mod netlist;
mod routing_2d;
mod splat;
mod structure_cache;

use anyhow::{anyhow, ensure, Context, Result};
use detail_routing::wire_segment::{splat_wire_segment, LayerPosition, WireTierLayer};
use detail_routing::{DetailRouter, GridCell, GridCellPosition, Layer, RoutingError};
use itertools::Itertools;
use log::{error, info, warn};
use mcpnr_common::block_storage::{
    Block, BlockStorage, Direction, Position, PropertyValue, ALL_DIRECTIONS, PLANAR_DIRECTIONS,
};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use netlist::Netlist;
use splat::Splatter;
use std::collections::HashMap;
use std::path::PathBuf;
use structure_cache::StructureCache;

use crate::detail_routing::wire_segment::WIRE_GRID_SCALE;
use crate::detail_routing::LAYERS_PER_TIER;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteId(pub u32);

#[derive(Clone, Debug)]
struct Config {
    input_file: PathBuf,
    techlib_directory: PathBuf,
    structure_directory: PathBuf,
    output_file: PathBuf,
    tiers: u32,
}

fn parse_args() -> Result<Config> {
    use clap::{App, Arg};
    let matches = App::new("MCPNR Placer")
        .version(env!("CARGO_PKG_VERSION"))
        .author(clap::crate_authors!())
        .about("Placement phase for the MCPNR flow")
        .arg(
            Arg::with_name("TECHLIB")
                .long("techlib")
                .value_name("TECHLIB")
                .required(true),
        )
        .arg(
            Arg::with_name("TIERS")
                .long("tiers")
                .value_name("TIERS")
                .default_value("1"),
        )
        .arg(
            Arg::with_name("INPUT")
                .help("Input design, as the output of a Yosys write_protobuf command")
                .index(1)
                .required(true),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .help("Output file location")
                .index(2)
                .required(true),
        )
        .get_matches();

    let techlib_directory = PathBuf::from(matches.value_of_os("TECHLIB").unwrap());

    Ok(Config {
        input_file: PathBuf::from(matches.value_of_os("INPUT").unwrap()),
        output_file: PathBuf::from(matches.value_of_os("OUTPUT").unwrap()),
        structure_directory: techlib_directory.join("structures"),
        techlib_directory,
        tiers: matches
            .value_of("TIERS")
            .ok_or_else(|| -> ! { unreachable!() })?
            .parse()
            .with_context(|| anyhow!("Parsing tiers argument"))?,
    })
}

fn block_facing(block: &Block) -> Option<Direction> {
    block
        .properties
        .as_ref()
        .and_then(|p| p.get("facing"))
        .and_then(|f| match f {
            PropertyValue::String(f) => match f.as_str() {
                "north" => Some(Direction::North),
                "south" => Some(Direction::South),
                "east" => Some(Direction::East),
                "west" => Some(Direction::West),
                "up" => Some(Direction::Up),
                "down" => Some(Direction::Down),
                _ => None,
            },
            PropertyValue::Byte(_) => None,
        })
}

const GEN_TEST_SQUARES: bool = false;

fn do_splat(
    config: &Config,
    design: &PlacedDesign,
    structure_cache: &StructureCache,
    output_structure: &mut BlockStorage,
) -> Result<()> {
    let splatter = Splatter::new(output_structure, structure_cache);

    splatter
        .draw_border(output_structure)
        .context("Error during border draw")?;

    for cell in design.cells.iter() {
        splatter
            .splat_cell(cell, output_structure)
            .context("Error during cell splat")?;
    }

    if GEN_TEST_SQUARES {
        let b_torch =
            output_structure.add_new_block_type(Block::new("minecraft:redstone_torch".into()));

        // Square of wires
        // Each side has 5 steps LI -> M0, M0 -> M1, M1 -> M1, M1 -> M0, M0 -> LI and corners (so 7
        // total wire cells)
        let wires = [
            (WireTierLayer::new(0, Layer::LI), Direction::South),
            (WireTierLayer::new(0, Layer::M0), Direction::South),
            (WireTierLayer::new(0, Layer::M1), Direction::South),
            (WireTierLayer::new(0, Layer::M1), Direction::South),
            (WireTierLayer::new(0, Layer::M0), Direction::South),
            (WireTierLayer::new(0, Layer::LI), Direction::South),
            (WireTierLayer::new(0, Layer::LI), Direction::East),
            (WireTierLayer::new(0, Layer::M0), Direction::East),
            (WireTierLayer::new(0, Layer::M1), Direction::East),
            (WireTierLayer::new(0, Layer::M1), Direction::East),
            (WireTierLayer::new(0, Layer::M0), Direction::East),
            (WireTierLayer::new(0, Layer::LI), Direction::East),
            (WireTierLayer::new(0, Layer::LI), Direction::North),
            (WireTierLayer::new(0, Layer::M0), Direction::North),
            (WireTierLayer::new(0, Layer::M1), Direction::North),
            (WireTierLayer::new(0, Layer::M1), Direction::North),
            (WireTierLayer::new(0, Layer::M0), Direction::North),
            (WireTierLayer::new(0, Layer::LI), Direction::North),
            (WireTierLayer::new(0, Layer::LI), Direction::West),
            (WireTierLayer::new(0, Layer::M0), Direction::West),
            (WireTierLayer::new(0, Layer::M1), Direction::West),
            (WireTierLayer::new(0, Layer::M1), Direction::West),
            (WireTierLayer::new(0, Layer::M0), Direction::West),
            (WireTierLayer::new(0, Layer::LI), Direction::West),
        ];
        let mut p = LayerPosition::new(11.into(), 0.into());
        for i in 0..wires.len() {
            let s = wires[(i + wires.len() - 1) % wires.len()];
            let e = wires[i];
            info!("{:?} -> {:?} at {:?}", s, e, p);
            let (pn, _) = splat_wire_segment(output_structure, p, s, e)?;
            p = pn;
        }
        let mut p = LayerPosition::new(9.into(), 10.into());
        for i in (0..wires.len()).rev() {
            let e = wires[(i + wires.len() - 1) % wires.len()];
            let s = wires[i];
            info!("{:?} -> {:?} at {:?}", s, e, p);
            let (pn, _) = splat_wire_segment(output_structure, p, s, e)?;
            p = pn;
        }
    }

    Ok(())
}

fn do_route(config: &Config, netlist: &Netlist, output: &mut BlockStorage) -> Result<()> {
    let extents = output.extents().clone();

    if GEN_TEST_SQUARES {
        return Ok(());
    }

    info!("Begin routing");

    let router = {
        let mut router = DetailRouter::new(
            extents[0] + (WIRE_GRID_SCALE as u32 - 1) / WIRE_GRID_SCALE as u32,
            config.tiers * LAYERS_PER_TIER,
            extents[2] + (WIRE_GRID_SCALE as u32 - 1) / WIRE_GRID_SCALE as u32,
        );

        let mut known_pins: HashMap<GridCellPosition, Direction> = HashMap::new();

        {
            let mut mark_in_extents =
                |pos: Position, v| match pos.try_into().and_then(|pos| router.get_cell_mut(pos)) {
                    Ok(vm) => *vm = v,
                    Err(_) => {}
                };

            for ((x, y, z), block) in output.iter_block_coords() {
                if (y % 16) > 8 {
                    continue;
                }
                let x = x as i32;
                let y = y as i32;
                let z = z as i32;
                let pos = Position::new(x, y, z);
                let block = output.info_for_index(block).ok_or_else(|| {
                    anyhow!(
                        "Failed to look up block info for {:?} while filling in routing grid",
                        block
                    )
                })?;
                match block.name.as_ref() {
                    "minecraft:redstone_wire" => {
                        // Redstone wire itself will happily connect to everything remotely close to it
                        // TODO: add step up/down cut analysis
                        mark_in_extents(pos, GridCell::Blocked);
                        for d in PLANAR_DIRECTIONS {
                            mark_in_extents(pos.offset(d), GridCell::Blocked);
                        }
                    }
                    "minecraft:oak_sign" => {
                        // Pin connection.
                        let grid_cell: GridCellPosition = pos.try_into()?;

                        let d = match block.properties.as_ref().and_then(|p| p.get("rotation")) {
                            Some(d) => {
                                let v = match d {
                                    PropertyValue::String(s) => s.parse().with_context(|| {
                                        anyhow!("Failed to parse rotation for pin {}: {:?}", pos, s)
                                    })?,
                                    PropertyValue::Byte(b) => *b,
                                };
                                match v {
                                    0 => Direction::South,
                                    1 => Direction::South,
                                    2 => Direction::South,
                                    3 => Direction::South,
                                    4 => Direction::West,
                                    5 => Direction::West,
                                    6 => Direction::West,
                                    7 => Direction::West,
                                    8 => Direction::North,
                                    9 => Direction::North,
                                    10 => Direction::North,
                                    11 => Direction::North,
                                    12 => Direction::East,
                                    13 => Direction::East,
                                    14 => Direction::East,
                                    15 => Direction::East,
                                    _ => {
                                        warn!("Pin has out of range rotation information {} at {}, assuming South", v, pos);
                                        Direction::South
                                    }
                                }
                            }
                            None => {
                                warn!("Pin was somehow missing rotation information at {}, assuming South", pos);
                                Direction::South
                            }
                        };

                        known_pins.insert(grid_cell, d);
                    }
                    "minecraft:redstone_torch" | "minecraft:redstone_wall_torch" => {
                        mark_in_extents(pos, GridCell::Blocked);
                        // technically we know one of the directions is going to be marked by whatever
                        // solid block, but it's more convenient to just unconditionally mark
                        // everything
                        for d in ALL_DIRECTIONS {
                            mark_in_extents(pos.offset(d), GridCell::Blocked);
                        }
                    }
                    "minecraft:repeater" => {
                        mark_in_extents(pos, GridCell::Blocked);
                        match block_facing(block) {
                            Some(Direction::North) | Some(Direction::South) => {
                                mark_in_extents(pos.offset(Direction::North), GridCell::Blocked);
                                mark_in_extents(pos.offset(Direction::South), GridCell::Blocked);
                            }
                            Some(Direction::East) | Some(Direction::West) => {
                                mark_in_extents(pos.offset(Direction::North), GridCell::Blocked);
                                mark_in_extents(pos.offset(Direction::South), GridCell::Blocked);
                            }
                            d => {
                                error!("Unsupported facing direction {:?} for redstone repeater", d)
                            }
                        }
                    }
                    "minecraft:lever" => {
                        mark_in_extents(pos, GridCell::Blocked);
                        for d in ALL_DIRECTIONS {
                            mark_in_extents(pos.offset(d), GridCell::Blocked);
                        }
                    }
                    "minecraft:piston" | "minecraft:sticky_piston" => {
                        // Pistons are giga cursed, we need to mark everything remotely closed to them
                        // as occupied to avoid phantom powering problems
                        mark_in_extents(pos, GridCell::Blocked);

                        // We also need to find the blocks attached to the face of the piston and mark
                        // the spaces those can push in to as occupied, potentially recursively (since
                        // the piston may be moving a block of redstone for example)
                        let piston_direction = block_facing(block);
                        if let Some(piston_direction) = piston_direction {
                            let po = pos.offset(piston_direction);
                            let is_sticky = output
                                .get_block(po.x as u32, po.y as u32, po.z as u32)
                                .ok()
                                .and_then(|b| {
                                    let b = output.info_for_index(*b)?;

                                    Some(b.is_sticky())
                                })
                                .unwrap_or(false);

                            // Punt on sticky block handling for now, none of our cells use it and
                            // handling it properly seems hard
                            ensure!(
                                !is_sticky,
                                "Sticky block propegation is currently unsupported"
                            );

                            // Mark the space that this block might get pushed into as blocked
                            mark_in_extents(po.offset(piston_direction), GridCell::Blocked);
                        } else {
                            error!("Piston missing facing property");
                        }
                    }
                    // Misc solid blocks
                    "minecraft:calcite" | "minecraft:redstone_lamp" | "minecraft:target" => {
                        mark_in_extents(pos, GridCell::Blocked);
                    }
                    s if s.ends_with("_wool") => {
                        mark_in_extents(pos, GridCell::Blocked);
                    }
                    "minecraft:air" => {
                        // Nothing to do for air, it's free space
                    }
                    s if s.ends_with("_stained_glass") => {
                        // Stained glass variants are just tier markers, allow routing through them.
                    }
                    _ => {
                        warn!("Unrecognized block type {}", block.name);
                    }
                }
            }
        }

        info!("Initial blocker mark done");

        let mut routing_pass = 0;

        const MAX_ROUTING_PASSES: u32 = 30;

        #[derive(PartialEq, Eq)]
        enum NetState {
            Unrouted,
            RippedUpInPass(u32),
            Routed,
        }

        // TODO: use unrandomized hashermap
        let mut net_states: HashMap<i64, (NetState, &netlist::Net)> = netlist
            .iter_nets()
            .map(|(net_idx, net)| (*net_idx, (NetState::Unrouted, net)))
            .collect();

        while routing_pass < MAX_ROUTING_PASSES
            && net_states.values().any(|(s, _)| *s != NetState::Routed)
        {
            info!("Begin routing pass {}", routing_pass);
            for (net_idx, net) in netlist.iter_nets() {
                let net_idx: u32 = (*net_idx)
                    .try_into()
                    .with_context(|| anyhow!("Convert net_idx {}", net_idx))?;
                if (routing_pass + net_idx) % 30 == 0 {
                    info!("Rip up net {}", net_idx);
                    net_states
                        .get_mut(&(net_idx as i64))
                        .map(|v| v.0 = NetState::RippedUpInPass(routing_pass));
                    // TODO: make this more efficient
                    let pins: Vec<_> = net
                        .iter_sinks(netlist)
                        .chain(net.iter_drivers(netlist))
                        .map(|pin| -> Result<_> {
                            let pos = Position::new(pin.x as i32, pin.y as i32, pin.z as i32);
                            let pos: GridCellPosition = pos.try_into()?;
                            Ok(pos)
                        })
                        .try_collect()?;

                    router
                        .rip_up(RouteId(net_idx), &pins)
                        .with_context(|| anyhow!("Rip up net {:?}", net_idx))?;
                }
            }

            for (net_idx, net) in netlist.iter_nets() {
                match net_states[net_idx].0 {
                    NetState::RippedUpInPass(p) if p == routing_pass => continue,
                    NetState::Routed => continue,
                    _ => {}
                }

                let net_idx: u32 = (*net_idx)
                    .try_into()
                    .with_context(|| anyhow!("Convert net_idx {}", net_idx))?;
                let mut drivers = net.iter_drivers(netlist);
                let driver = match drivers.next() {
                    Some(driver) => driver,
                    None => {
                        warn!("Undriven net {:?}", net);
                        continue;
                    }
                };
                if drivers.next().is_some() {
                    return Err(anyhow!("Driver-Driver conflict in net {:?}", net));
                }

                let start = Position::new(driver.x as i32, driver.y as i32, driver.z as i32);
                let start: GridCellPosition = start.try_into()?;
                if let GridCell::Occupied(_, RouteId(id)) = router.get_cell(start)? {
                    if id != &net_idx {
                        warn!(
                            "Starting position of net {} at {} is occupied by another net {}",
                            net_idx, start, id
                        )
                    }
                }
                let start_direction = known_pins
                    .get(&start)
                    .ok_or_else(|| anyhow!("Failed to find driver pin {}", start))?;
                *(router.get_cell_mut(start).context("Get start cell")?) =
                    GridCell::Occupied(*start_direction, RouteId(net_idx));

                let mut this_net_all_routed = true;

                for sink in net.iter_sinks(netlist) {
                    let end = Position::new(sink.x as i32, sink.y as i32, sink.z as i32);
                    let end: GridCellPosition = end.try_into()?;
                    if let GridCell::Occupied(_, RouteId(id)) =
                        router.get_cell(end).context("Get end cell")?
                    {
                        if id != &net_idx {
                            warn!(
                                "Ending position of net {} at {} is occupied by another net {}",
                                net_idx, end, id
                            );
                        }
                    }
                    let end_direction = known_pins
                        .get(&end)
                        .ok_or_else(|| anyhow!("Failed to find sink pin {}", end))?;
                    *(router.get_cell_mut(end).context("Get end cell")?) =
                        GridCell::Occupied(*end_direction, RouteId(net_idx));

                    match router.route(start, end, RouteId(net_idx)) {
                        Ok(_) => {}
                        Err(e) => {
                            if let Some(RoutingError::Unroutable) = e.downcast_ref() {
                                warn!("Failed to route net {:?} {:?}", driver, sink);
                                this_net_all_routed = false;
                                continue;
                            } else {
                                return Err(e);
                            }
                        }
                    }
                }

                if this_net_all_routed {
                    net_states
                        .get_mut(&(net_idx as i64))
                        .map(|v| v.0 = NetState::Routed);
                }
            }

            routing_pass += 1;
        }

        router
    };

    let b_wools = [
        "minecraft:white_wool",
        "minecraft:orange_wool",
        "minecraft:magenta_wool",
        "minecraft:light_blue_wool",
        "minecraft:yellow_wool",
        "minecraft:lime_wool",
        "minecraft:pink_wool",
        "minecraft:gray_wool",
        "minecraft:light_gray_wool",
        "minecraft:cyan_wool",
        "minecraft:purple_wool",
        "minecraft:blue_wool",
        "minecraft:brown_wool",
        "minecraft:green_wool",
        "minecraft:red_wool",
        "minecraft:black_wool",
        "minecraft:white_terracotta",
        "minecraft:orange_terracotta",
        "minecraft:magenta_terracotta",
        "minecraft:light_blue_terracotta",
        "minecraft:yellow_terracotta",
        "minecraft:lime_terracotta",
        "minecraft:pink_terracotta",
        "minecraft:gray_terracotta",
        "minecraft:light_gray_terracotta",
        "minecraft:cyan_terracotta",
        "minecraft:purple_terracotta",
        "minecraft:blue_terracotta",
        "minecraft:brown_terracotta",
        "minecraft:green_terracotta",
        "minecraft:red_terracotta",
        "minecraft:black_terracotta",
    ]
    .into_iter()
    .map(|ty| output.add_new_block_type(Block::new(ty.into())))
    .collect::<Vec<_>>();
    let b_glass = output.add_new_block_type(Block::new("minecraft:glass".into()));

    {
        for ((x, y, z), block) in output.iter_block_coords_mut() {
            let x = x as i32;
            let y = y as i32;
            let z = z as i32;
            match router
                .get_cell(Position::new(x, y, z).try_into()?)
                .context("Failed to get router cell in wire splat")?
            {
                GridCell::Free => {}
                GridCell::Blocked => {
                    // *block = b_glass;
                }
                GridCell::Occupied(d, net) => {
                    if y != 0 {
                        continue;
                    }
                    if (*d == Direction::North || *d == Direction::South) && x % 2 == 0 {
                        *block = b_wools[(net.0 as usize) % b_wools.len()];
                    }
                    if (*d == Direction::East || *d == Direction::West) && z % 2 == 0 {
                        *block = b_wools[(net.0 as usize) % b_wools.len()];
                    }
                }
                GridCell::Claimed(net) => {
                    *block = b_wools[(net.0 as usize) % b_wools.len()];
                }
            }
        }
    }

    Ok(())
}

fn build_output(config: &Config, netlist: &Netlist) -> Result<BlockStorage> {
    if GEN_TEST_SQUARES {
        let size = 2 * 7 * 4;
        Ok(BlockStorage::new(size, 16, size))
    } else {
        let (mx, mz) = netlist.iter_pins().fold((0, 0), |(mx, mz), pin| {
            (std::cmp::max(mx, pin.x), std::cmp::max(mz, pin.z))
        });

        Ok(BlockStorage::new(mx + 4, config.tiers * 16, mz + 4))
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let config = parse_args()?;

    let placed_design = {
        let inf = std::fs::read(&config.input_file).unwrap();
        PlacedDesign::decode(&inf[..]).unwrap()
    };

    let mut structure_cache = StructureCache::new(&config.structure_directory, &placed_design)?;
    let netlist = netlist::Netlist::new(&placed_design, &structure_cache)?;
    let mut output_structure = build_output(&config, &netlist)?;

    structure_cache.build_palette_maps(&mut output_structure)?;

    do_splat(
        &config,
        &placed_design,
        &structure_cache,
        &mut output_structure,
    )?;

    do_route(&config, &netlist, &mut output_structure)?;

    {
        let outf = std::fs::File::create(config.output_file).unwrap();

        serde_json::ser::to_writer(outf, &output_structure)?;
    }

    Ok(())
}
