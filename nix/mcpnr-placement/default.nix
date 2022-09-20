{ pkgs
, cmake
, pkg-config
, protobuf

, fontconfig
, freetype
, lapack, blas, gfortran
, mcpnr-rust-platform
, vulkan-loader
, xorg
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

  nativeBuildInputs = [
    cmake
    pkg-config
    protobuf
    gfortran
  ];

  buildInputs = [
    fontconfig
    freetype
    lapack blas
    vulkan-loader
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
    xorg.libXrender
    xorg.libxcb
  ];
}
