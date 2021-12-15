mod netlist;
mod routing_2d;
mod splat;
mod structure_cache;

use anyhow::{anyhow, Context, Result};
use mcpnr_common::block_storage::{Block, BlockStorage};
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::PlacedDesign;
use netlist::Netlist;
use routing_2d::{Position, RouteId, Router2D};
use splat::Splatter;
use std::path::PathBuf;
use structure_cache::StructureCache;

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

    Ok(())
}

fn do_route(netlist: &Netlist, output: &mut BlockStorage) -> Result<()> {
    let extents = output.extents().clone();
    let router = {
        let mut router = Router2D::new(extents[0], extents[2]);

        for (net_idx, net) in netlist.iter_nets() {
            let net_idx: u32 = (*net_idx)
                .try_into()
                .with_context(|| anyhow!("Convert net_idx {}", net_idx))?;
            let mut drivers = net.iter_drivers(netlist);
            let driver = drivers
                .next()
                .ok_or_else(|| anyhow!("Undriven net {:?}", net))?;
            if drivers.next().is_some() {
                return Err(anyhow!("Driver-Driver conflict in net {:?}", net));
            }

            let start = routing_2d::Position::new(driver.x, driver.z);

            for sink in net.iter_sinks(netlist) {
                let end = routing_2d::Position::new(sink.x, sink.z);

                router.route(start, end, RouteId(net_idx))?;
            }
        }

        router
    };

    let y = 4;
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
    ]
    .into_iter()
    .map(|ty| output.add_new_block_type(Block::new(ty.into())))
    .collect::<Vec<_>>();

    {
        for z in 0..extents[2] {
            for x in 0..extents[0] {
                if let Some(net) = router.is_cell_occupied(Position::new(x, z))? {
                    *(output.get_block_mut(x, y, z)?) = b_wools[(net.0 as usize) % b_wools.len()];
                }
            }
        }
    }

    Ok(())
}

fn build_output(config: &Config, netlist: &Netlist) -> Result<BlockStorage> {
    let (mx, mz) = netlist.iter_pins().fold((0, 0), |(mx, mz), pin| {
        (std::cmp::max(mx, pin.x), std::cmp::max(mz, pin.z))
    });

    Ok(BlockStorage::new(mx + 2, config.tiers * 16, mz + 2))
}

fn main() -> Result<()> {
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

    do_route(&netlist, &mut output_structure)?;

    {
        let outf = std::fs::File::create(config.output_file).unwrap();

        serde_json::ser::to_writer(outf, &output_structure)?;
    }

    Ok(())
}
