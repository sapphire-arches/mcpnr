{ pkgs, mcpnr-rust-platform }:
mcpnr-rust-platform.buildRustPackage rec {
  pname = "mcpnr-placement";
  version = "0.1.0";
  src = ../../pnr;
  cargoSha256 = "sha256-VAyEsnUaFi+INe5lbz/+6IN6GpL3TLMB97pMgVau2FY=";
  doCheck = false;

  cargoBuildFlags = [ "-p" pname ];

  nativeBuildInputs = [ ];
  buildInputs = [ ];
}
