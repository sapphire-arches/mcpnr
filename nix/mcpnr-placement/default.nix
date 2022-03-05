{ pkgs
, mcpnr-rust-platform
, xorg
, vulkan-loader
}:
mcpnr-rust-platform.buildRustPackage rec {
  pname = "mcpnr-placement";
  version = "0.1.0";
  src = ../../pnr;
  cargoSha256 = "sha256-odoJDJHNLi7vcOkDvaUhPh/wWAaFsDcPy5b6PKtwS9s=";
  doCheck = false;

  cargoBuildFlags = [ "-p" pname ];

  YOSYS_PROTO_PATH = "${pkgs.yosys-proto}";

  #TODO: For packaging, we need to wrap the program to set LD_LIBRARY_PATH

  nativeBuildInputs = [ pkgs.protobuf ];

  buildInputs = [
    xorg.libX11
    xorg.libXcursor
    xorg.libXrandr
    xorg.libXi
    xorg.libxcb
    xorg.libXrender
    vulkan-loader
  ];
}
