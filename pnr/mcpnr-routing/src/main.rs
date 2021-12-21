mod detail_routing;
mod netlist;
mod routing_2d;
mod splat;
mod structure_cache;

use anyhow::{anyhow, ensure, Context, Result};
use detail_routing::wire_segment::{splat_wire_segment, WirePosition, WireTierLayer};
use detail_routing::{
    DetailRouter, Direction, GridCell, Layer, Position, RoutingError, ALL_DIRECTIONS,
    PLANAR_DIRECTIONS,
};
use log::{error, info, warn};
use mcpnr_common::block_storage::{Block, BlockStorage, PropertyValue};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use netlist::Netlist;
use splat::Splatter;
use std::path::PathBuf;
use structure_cache::StructureCache;

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

    // for cell in design.cells.iter() {
    //     splatter
    //         .splat_cell(cell, output_structure)
    //         .context("Error during cell splat")?;
    // }

    // temp test code: splat all possible wire directions
    for (x, di) in PLANAR_DIRECTIONS.iter().enumerate() {
        for (z, od) in PLANAR_DIRECTIONS.iter().enumerate() {
            let wp = WirePosition::new(x as i32, z as i32);
            let r = splat_wire_segment(
                output_structure,
                wp,
                (WireTierLayer::new(0, Layer::LI), *di),
                (WireTierLayer::new(0, Layer::LI), *od),
            );
            info!("({}, {}): {:?} -> {:?}, {:?}", x, z, di, od, r);
        }
    }

    Ok(())
}

fn do_route(netlist: &Netlist, output: &mut BlockStorage) -> Result<()> {
    let extents = output.extents().clone();

    let router = {
        let mut router = DetailRouter::new(extents[0], extents[1], extents[2]);

        {
            let mut mark_in_extents = |pos, v| match router.get_cell_mut(pos) {
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
                        // TODO: we should look up what pin this is and set things to the right nets
                        // immediately
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

            for (net_idx, net) in netlist.iter_nets() {
                let net_idx: u32 = (*net_idx)
                    .try_into()
                    .with_context(|| anyhow!("Convert net_idx {}", net_idx))?;
                let occupied = GridCell::Occupied(RouteId(net_idx));

                for pin in net.iter_sinks(netlist) {
                    let pos = Position::new(pin.x as i32, pin.y as i32, pin.z as i32);
                    let _ = router.get_cell_mut(pos).map(|c| *c = occupied);
                }

                for pin in net.iter_drivers(netlist) {
                    let pos = Position::new(pin.x as i32, pin.y as i32, pin.z as i32);
                    let _ = router.get_cell_mut(pos).map(|c| *c = occupied);
                }
            }
        }

        for (net_idx, net) in netlist.iter_nets() {
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
            if let GridCell::Occupied(RouteId(id)) = router.get_cell(start)? {
                if id != &net_idx {
                    warn!(
                        "Starting position of net {} at {} is occupied by another net {}",
                        net_idx, start, id
                    )
                }
            }
            *(router.get_cell_mut(start)?) = GridCell::Occupied(RouteId(net_idx));

            for sink in net.iter_sinks(netlist) {
                let end = Position::new(sink.x as i32, sink.y as i32, sink.z as i32);
                if let GridCell::Occupied(RouteId(id)) = router.get_cell(end)? {
                    if id != &net_idx {
                        warn!(
                            "Ending position of net {} at {} is occupied by another net {}",
                            net_idx, end, id
                        );
                    }
                }
                *(router.get_cell_mut(end)?) = GridCell::Occupied(RouteId(net_idx));

                match router.route(start, end, RouteId(net_idx)) {
                    Ok(_) => {}
                    Err(e) => {
                        if let Some(RoutingError::Unroutable) = e.downcast_ref() {
                            warn!("Failed to route net {:?} {:?}", driver, sink);
                            for e in e.chain() {
                                warn!(" because... {}", e)
                            }
                            continue;
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
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
                .get_cell(Position::new(x, y, z))
                .context("Failed to get router cell in wire splat")?
            {
                GridCell::Free => {}
                GridCell::Blocked => {
                    // *block = b_glass;
                }
                GridCell::Occupied(net) => {
                    *block = b_wools[(net.0 as usize) % b_wools.len()];
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
    // let (mx, mz) = netlist.iter_pins().fold((0, 0), |(mx, mz), pin| {
    //     (std::cmp::max(mx, pin.x), std::cmp::max(mz, pin.z))
    // });

    // Ok(BlockStorage::new(mx + 4, config.tiers * 16, mz + 4))
    //
    Ok(BlockStorage::new(16, 16, 16))
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

    // do_route(&netlist, &mut output_structure)?;

    {
        let outf = std::fs::File::create(config.output_file).unwrap();

        serde_json::ser::to_writer(outf, &output_structure)?;
    }

    Ok(())
}
