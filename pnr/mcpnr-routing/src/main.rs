mod detail_routing;
mod netlist;
mod routing_2d;
mod splat;
mod structure_cache;

use anyhow::{anyhow, ensure, Context, Result};
use detail_routing::wire_segment::{splat_wire_segment, LayerPosition, WireTierLayer};
use detail_routing::{DetailRouter, GridCell, GridCellPosition, Layer, RoutingError};
use log::{debug, error, info, warn};
use mcpnr_common::block_storage::{
    Block, BlockStorage, Direction, Position, PropertyValue, ALL_DIRECTIONS, PLANAR_DIRECTIONS,
};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use netlist::{Net, Netlist};
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
    structure_directory: PathBuf,
    output_file: PathBuf,
    tiers: u32,
}

fn parse_args() -> Result<Config> {
    use clap::{App, Arg};
    let matches = App::new("MCPNR Placer")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
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

#[derive(PartialEq, Eq)]
enum NetState {
    Unrouted,
    RippedUpInPass(u32),
    Routed,
}

const MAX_ROUTING_PASSES: u32 = 30;

struct Router<'nets> {
    netlist: &'nets Netlist,
    net_states: HashMap<u32, (NetState, &'nets Net)>,
    known_pins: HashMap<GridCellPosition, Direction>,
    detail_router: DetailRouter,
    routing_pass: u32,
}

impl<'nets> Router<'nets> {
    fn new(config: &Config, netlist: &'nets Netlist, output: &mut BlockStorage) -> Result<Self> {
        let extents = output.extents().clone();
        let mut detail_router = DetailRouter::new(
            extents[0] + (WIRE_GRID_SCALE as u32 - 1) / WIRE_GRID_SCALE as u32,
            config.tiers * LAYERS_PER_TIER,
            extents[2] + (WIRE_GRID_SCALE as u32 - 1) / WIRE_GRID_SCALE as u32,
        );

        let mut known_pins: HashMap<GridCellPosition, Direction> = HashMap::new();

        {
            let mut mark_in_extents = |pos: Position, v| match pos
                .try_into()
                .and_then(|pos| detail_router.get_cell_mut(pos))
            {
                Ok(vm) => *vm = v,
                Err(_) => {}
            };

            for ((x, y, z), block) in output.iter_block_coords() {
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
                                    12 => Direction::West,
                                    13 => Direction::West,
                                    14 => Direction::West,
                                    15 => Direction::West,
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

                        info!("Mark known pin at {:?}", grid_cell);
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

        // TODO: use unrandomized hashermap
        let net_states: HashMap<u32, (NetState, &netlist::Net)> = netlist
            .iter_nets()
            .map(|(net_idx, net)| (*net_idx as u32, (NetState::Unrouted, net)))
            .collect();

        Ok(Self {
            detail_router,
            netlist,
            net_states,
            known_pins,
            routing_pass: 0,
        })
    }

    fn rnr_loop(&mut self) -> Result<()> {
        self.routing_pass = 0;
        while self.routing_pass < MAX_ROUTING_PASSES
            && self
                .net_states
                .values()
                .any(|(s, _)| *s != NetState::Routed)
        {
            info!("Begin routing pass {}", self.routing_pass);
            for (net_idx, net) in self.netlist.iter_nets() {
                let net_idx: u32 = (*net_idx)
                    .try_into()
                    .with_context(|| anyhow!("Convert net_idx {}", net_idx))?;
                if (self.routing_pass + net_idx) % 30 == 0
                    && self.routing_pass != MAX_ROUTING_PASSES - 1
                {
                    info!("Rip up net {}", net_idx);
                    self.net_states
                        .get_mut(&net_idx)
                        .map(|v| v.0 = NetState::RippedUpInPass(self.routing_pass));

                    self.detail_router
                        .rip_up(RouteId(net_idx))
                        .with_context(|| anyhow!("Rip up net {:?}", net_idx))?;

                    for pin in net
                        .iter_sinks(self.netlist)
                        .chain(net.iter_drivers(self.netlist))
                    {
                        let pos = Position::new(pin.x as i32, pin.y as i32, pin.z as i32);
                        let pos: GridCellPosition = pos.try_into()?;
                        let pin_direction = self
                            .known_pins
                            .get(&pos)
                            .ok_or_else(|| anyhow!("Failed to find pin {}", pos))?;
                        *(self
                            .detail_router
                            .get_cell_mut(pos)
                            .context("Get start cell")?) =
                            GridCell::Occupied(*pin_direction, RouteId(net_idx));
                    }
                }
            }

            for (net_idx, _) in self.netlist.iter_nets() {
                self.route_net(*net_idx as u32)
                    .with_context(|| anyhow!("Route net {:?}", net_idx))?;
            }

            self.routing_pass += 1;
        }

        Ok(())
    }

    fn route_net(&mut self, net_idx: u32) -> Result<()> {
        let (net_state, net) = &self.net_states[&net_idx];
        match net_state {
            NetState::RippedUpInPass(p) if *p == self.routing_pass => return Ok(()),
            NetState::Routed => return Ok(()),
            _ => {}
        }

        let mut drivers = net.iter_drivers(self.netlist);
        let driver = match drivers.next() {
            Some(driver) => driver,
            None => {
                warn!("Undriven net {:?}", net);
                return Ok(());
            }
        };
        if drivers.next().is_some() {
            return Err(anyhow!("Driver-Driver conflict in net {:?}", net));
        }

        let start = Position::new(driver.x as i32, driver.y as i32, driver.z as i32);
        let start: GridCellPosition = start.try_into()?;
        if let GridCell::Occupied(_, RouteId(id)) = self.detail_router.get_cell(start)? {
            if id != &net_idx {
                warn!(
                    "Starting position of net {} at {} is occupied by another net {}",
                    net_idx, start, id
                )
            }
        }
        let start_direction = self
            .known_pins
            .get(&start)
            .ok_or_else(|| anyhow!("Failed to find driver pin {}", start))?;
        *(self
            .detail_router
            .get_cell_mut(start)
            .context("Get start cell")?) = GridCell::Blocked;

        let mut this_net_all_routed = true;

        for sink in net.iter_sinks(self.netlist) {
            let end = Position::new(sink.x as i32, sink.y as i32, sink.z as i32);
            let end: GridCellPosition = end.try_into()?;
            if let GridCell::Occupied(_, RouteId(id)) =
                self.detail_router.get_cell(end).context("Get end cell")?
            {
                if id != &net_idx {
                    warn!(
                        "Ending position of net {} at {} is occupied by another net {}",
                        net_idx, end, id
                    );
                }
            }
            let end_direction = self
                .known_pins
                .get(&end)
                .ok_or_else(|| anyhow!("Failed to find sink pin {}", end))?;
            *(self
                .detail_router
                .get_cell_mut(end)
                .context("Get end cell")?) = GridCell::Blocked;

            match self.detail_router.route(
                start,
                *start_direction,
                end,
                *end_direction,
                RouteId(net_idx),
            ) {
                Ok(_) => {}
                Err(e) => {
                    if let Some(RoutingError::Unroutable) = e.downcast_ref() {
                        warn!("Failed to route net {:?} -> {:?}", driver, sink);
                        for e in e.chain() {
                            warn!("  because ... {}", e);
                        }
                        this_net_all_routed = false;
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        if this_net_all_routed {
            info!("Mark net {:?} routed", net_idx);
            self.net_states
                .get_mut(&net_idx)
                .map(|v| v.0 = NetState::Routed);
        }

        Ok(())
    }
}

fn do_route(config: &Config, netlist: &Netlist, output: &mut BlockStorage) -> Result<()> {
    if GEN_TEST_SQUARES {
        return Ok(());
    }

    let mut router = Router::new(config, netlist, output)?;
    router.rnr_loop()?;

    info!("Begin wire splats");
    for (net_idx, net) in netlist.iter_nets() {
        let net_idx = *net_idx as u32;
        for pin in net.iter_sinks(netlist) {
            let pos = Position::new(pin.x as i32, pin.y as i32, pin.z as i32);
            let mut pos: GridCellPosition = pos.try_into()?;
            let mut prev_direction = *router.known_pins.get(&pos).unwrap();

            // TODO: actually route out of the cell
            pos = pos.offset(prev_direction);
            debug!(
                "Splat wire at {:?} {:?} for net {}",
                pos,
                router.detail_router.get_cell(pos),
                net_idx,
            );

            while let GridCell::Occupied(d, id) = router
                .detail_router
                .get_cell(pos)
                .context("Wire splat backtrack")?
            {
                if id.0 != net_idx {
                    break;
                }
                let d = *d;
                let tier = pos.y as u32 / LAYERS_PER_TIER;
                let layer = Layer::from_compact_idx(pos.y % LAYERS_PER_TIER as i32)?;
                let wire_pos = (WireTierLayer::new(tier, layer), prev_direction);
                if let Err(e) = splat_wire_segment(
                    output,
                    LayerPosition::new(pos.x, pos.z),
                    wire_pos,
                    (wire_pos.0, d),
                ) {
                    warn!("Failed to splat wire at {:?}: {}", wire_pos, e);
                }

                prev_direction = d;
                pos = pos.offset(d);
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

    do_splat(&placed_design, &structure_cache, &mut output_structure)?;

    do_route(&config, &netlist, &mut output_structure)?;

    {
        let outf = std::fs::File::create(config.output_file).unwrap();

        serde_json::ser::to_writer(outf, &output_structure)?;
    }

    Ok(())
}
