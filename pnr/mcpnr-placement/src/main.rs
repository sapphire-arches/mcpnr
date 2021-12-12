use anyhow::{Context, Result};
use itertools::Itertools;
use mcpnr_common::prost::Message;
use mcpnr_common::protos::mcpnr::{PlacedDesign, Position};
use mcpnr_common::protos::yosys::pb::parameter::Value as YPValue;
use mcpnr_common::protos::yosys::pb::{Design, Parameter};
use mcpnr_common::protos::CellExt;
use std::path::PathBuf;

#[derive(Clone, Debug)]
struct Config {
    input_file: PathBuf,
    output_file: PathBuf,
}

fn parse_args() -> Config {
    use clap::{App, Arg};
    let matches = App::new("MCPNR Placer")
        .version(env!("CARGO_PKG_VERSION"))
        .author(clap::crate_authors!())
        .about("Placement phase for the MCPNR flow")
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

    Config {
        input_file: PathBuf::from(matches.value_of_os("INPUT").unwrap()),
        output_file: PathBuf::from(matches.value_of_os("OUTPUT").unwrap()),
    }
}

fn place(design: Design) -> Result<PlacedDesign> {
    let cells = design
        .modules
        .into_values()
        .find(|m| {
            m.attribute.get("top")
                == Some(&Parameter {
                    value: Some(YPValue::Int(1)),
                })
        })
        .unwrap()
        .cell
        .into_iter()
        .map(|(_, cell)| -> Result<_> {
            fn get_pos_attrs(
                cell: &mcpnr_common::protos::yosys::pb::module::Cell,
            ) -> Result<(u32, u32, u32)> {
                Ok((
                    cell.get_param_i64_with_default("POS_X", 0)
                        .map_err(anyhow::Error::from)
                        .and_then(|v| v.try_into().map_err(anyhow::Error::from))
                        .with_context(|| "Read attr POS_X")?,
                    cell.get_param_i64_with_default("POS_Y", 0)
                        .map_err(anyhow::Error::from)
                        .and_then(|v| v.try_into().map_err(anyhow::Error::from))
                        .with_context(|| "Read attr POS_Y")?,
                    cell.get_param_i64_with_default("POS_Z", 0)
                        .map_err(anyhow::Error::from)
                        .and_then(|v| v.try_into().map_err(anyhow::Error::from))
                        .with_context(|| "Read attr POS_Z")?,
                ))
            }

            let (x, y, z) = match cell.r#type.as_ref() {
                "MCPNR_SWITCHES" | "MCPNR_LIGHTS" => get_pos_attrs(&cell)
                    .with_context(|| format!("Getting position from cell {:?}", &cell))?,
                _ => (10, 0, 0),
            };
            let mcpnr_cell = mcpnr_common::protos::mcpnr::placed_design::Cell {
                attribute: cell.attribute,
                connection: cell.connection,
                parameter: cell.parameter,
                pos: Some(Position { x, y, z }),
                r#type: cell.r#type,
            };
            Ok(mcpnr_cell)
        })
        .try_collect()?;
    Ok(PlacedDesign {
        creator: format!(
            "Placed by MCPNR {}, Synth: {}",
            env!("CARGO_PKG_VERSION"),
            design.creator,
        ),
        cells,
    })
}

fn main() -> Result<()> {
    let config = parse_args();

    let design = {
        let inf = std::fs::read(config.input_file).unwrap();
        Design::decode(&inf[..]).unwrap()
    };

    let placed_design = place(design)?;

    {
        use std::io::Write;
        let mut outf = std::fs::File::create(config.output_file).unwrap();
        let encoded = placed_design.encode_to_vec();

        outf.write_all(&encoded[..]).unwrap();
    }

    Ok(())
}
