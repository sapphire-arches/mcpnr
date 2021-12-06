use std::path::PathBuf;
use mcpnr_common::prost::Message;

#[derive(Clone,Debug)]
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

fn main() {
    let config = parse_args();

    let design = {
        let inf = std::fs::read(config.input_file).unwrap();
        mcpnr_common::yosys::Design::decode(&inf[..]).unwrap()
    };

    {
        use std::io::Write;
        let mut outf = std::fs::File::create(config.output_file).unwrap();
        let encoded = design.encode_to_vec();

        outf.write_all(&encoded[..]).unwrap();
    }
}
