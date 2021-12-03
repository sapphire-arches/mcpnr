{ pkgs, stdenv, mcpnr }:
stdenv.mkDerivation {
  name = "yosys-synth-mc";
  src = "../../yosys-synth_mc";

  doCheck = false;

  nativeBuildInputs = with pkgs; [
    readline
    zlib
  ];

  buildInputs = with pkgs; [
    yosys
  ];

  makeFlags = [ "YOSYS_CONFIG=yosys-config" ];
}
