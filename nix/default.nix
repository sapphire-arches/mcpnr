{ pkgs }:
{
  yosys-synth-mc = pkgs.callPackage ./yosys-synth_mc { };
  mcpnr-placement = pkgs.callPackage ./mcpnr-placement { };
  /* mcpnr-common = pkgs.callPackage ./mcpnr-common { }; */
}
