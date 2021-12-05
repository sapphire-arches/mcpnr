{ pkgs, mcpnr-rust-platform }:
mcpnr-rust-platform.buildRustPackage rec {
  pname = "mcpnr-placement";
  version = "0.1.0";
  src = ../../pnr;
  cargoSha256 = "sha256-odoJDJHNLi7vcOkDvaUhPh/wWAaFsDcPy5b6PKtwS9s=";
  doCheck = false;

  cargoBuildFlags = [ "-p" pname ];

  YOSYS_PROTO_PATH = "${pkgs.yosys-proto}";

  nativeBuildInputs = [ pkgs.protobuf ];
  buildInputs = [ ];
}
