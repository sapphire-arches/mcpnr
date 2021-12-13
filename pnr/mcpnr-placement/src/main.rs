use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::{PlacedDesign, Position};
use mcpnr_common::protos::yosys::pb::parameter::Value as YPValue;
use mcpnr_common::protos::yosys::pb::{Design, Parameter};
use mcpnr_common::protos::CellExt;
use placement_cell::{CellFactory, PlacementCell};
use std::collections::HashMap;
use std::path::PathBuf;

mod placement_cell;

#[derive(Clone, Debug)]
struct Config {
    input_file: PathBuf,
    output_file: PathBuf,
    structure_directory: PathBuf,
}

fn parse_args() -> Config {
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

    Config {
        input_file: PathBuf::from(matches.value_of_os("INPUT").unwrap()),
        output_file: PathBuf::from(matches.value_of_os("OUTPUT").unwrap()),
        structure_directory: techlib_directory.join("structures"),
    }
}

fn place(config: &Config, design: Design) -> Result<PlacedDesign> {
    let top_module = design
        .modules
        .into_values()
        .find(|m| {
            m.attribute.get("top")
                == Some(&Parameter {
                    value: Some(YPValue::Int(1)),
                })
        })
        .ok_or_else(|| anyhow!("Failed to locate top module"))?;

    let mut cell_factory = CellFactory::new(config.structure_directory.clone());

    let mut cells: HashMap<String, (PlacementCell, _)> = top_module
        .cell
        .into_iter()
        .map(|(key, cell)| -> Result<_> {
            Ok((
                key,
                (
                    cell_factory.build_cell(&cell)?,
                    (cell.attribute, cell.connection, cell.parameter, cell.r#type),
                ),
            ))
        })
        .try_collect()?;

    // TODO: smart place
    let mut cx = 0;
    let mut cz = 5;
    let mut row_max_z = 0;

    for (_, (cell, _)) in cells.iter_mut() {
        if cell.pos_locked {
            continue;
        }
        if cx + cell.sx > 50 {
            // TODO: don't hard code region size
            cz += row_max_z;
            row_max_z = 0;
            cx = 0;
        }
        cell.x = cx;
        cell.z = cz;

        cx += cell.sx;
        row_max_z = std::cmp::max(cell.sz, row_max_z);
    }

    // Convert intermediate placement cells to output format
    let cells = cells
        .into_iter()
        .map(|(_, (cell, (attribute, connection, parameter, ty)))| {
            let pos = cell.unexpanded_pos();
            let mcpnr_cell = mcpnr_common::protos::mcpnr::placed_design::Cell {
                attribute,
                connection,
                parameter,
                pos: Some(Position {
                    x: pos[0],
                    y: pos[1],
                    z: pos[2],
                }),
                r#type: ty,
            };
            mcpnr_cell
        })
        .collect();

    Ok(PlacedDesign {
        creator: format!(
            "Placed by MCPNR {}, Synth: {}",
            env!("CARGO_PKG_VERSION"),
            design.creator,
        ),
        nets: top_module.netname,
        cells,
    })
}

fn main() -> Result<()> {
    let config = parse_args();

    let design = {
        let inf = std::fs::read(&config.input_file)
            .with_context(|| anyhow!("Open input file {:?}", config.input_file))?;
        Design::decode(&inf[..])
            .with_context(|| anyhow!("Failed to parse file {:?}", config.input_file))?
    };

    let placed_design = place(&config, design)
        .with_context(|| anyhow!("Place design from {:?}", config.input_file))?;

    {
        use std::io::Write;
        let mut outf = std::fs::File::create(&config.output_file)
            .with_context(|| anyhow!("Failed to open/create output file {:?}", config.output_file))?;
        let encoded = placed_design.encode_to_vec();

        outf.write_all(&encoded[..])
            .with_context(|| anyhow!("Failed to write to output file {:?}", config.output_file))?;
    }

    Ok(())
}
