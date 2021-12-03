{
  description = "PnR for Minecraft";

  inputs = {
    bt-yosys.url = "github:bobtwinkles/yosys/master";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nix, nixpkgs, bt-yosys, flake-utils }:
    {
      overlay = final: prev: {
        mcpnr = rec {
          nix = prev.callPackage ./nix { };
        };
      };
    }
    //
    (
      flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays =[
              self.overlay
            ];
          };
        in
        {
          packages = flake-utils.lib.flattenTree pkgs.mcpnr.nix;

          checks = { };
        }
      )
    );
}
