{
  description = "A very basic flake";

  inputs = {
    bt-yosys.url = "github:bobtwinkles/yosys/master";
  };

  outputs = { self, nix, nixpkgs, bt-yosys }:
    let
      supportedSystems = [ "x86_64-linux" "i686-linux" "aarch64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);
      version = "0.1-${self.shortRev or "dirty"}";
    in
    {
      overlay = final: prev: {
        mcpnr = with final; let nix = final.nix; in stdenv.mkDerivation {
          name = "mcpnr-${version}";
          buildInputs = [
            bt-yosys
          ];
        };

        src = self;
      };

      defaultPackage = forAllSystems (system: (import nixpkgs {
        inherit system;
        overlays = [ self.overlay nix.overlay ];
      }).mcpnr);
    };
}
