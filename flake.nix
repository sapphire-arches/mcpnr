{
  description = "PnR for Minecraft";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    bt-yosys.url = "github:bobtwinkles/yosys/master";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, bt-yosys, flake-utils }:
    {
      overlay = final: prev: {
        mcpnr = prev.callPackage ./nix { };
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
          mcpnrPackages = flake-utils.lib.flattenTree pkgs.mcpnr;
          mcpnrPackagesList = pkgs.lib.attrValues mcpnrPackages;
        in
        {
          packages = mcpnrPackages;

          devShell = pkgs.mkShell {
            buildInputs = with pkgs; [
              # For formatting Nix expressions
              nixpkgs-fmt

              # For viewing intermediate Yosys outputs
              xdot
              graphviz

            ] ++ (pkgs.lib.concatMap (p: p.buildInputs) mcpnrPackagesList);

            nativeBuildInputs = with pkgs; [
            ] ++ (pkgs.lib.concatMap (p: p.nativeBuildInputs) mcpnrPackagesList);

            shellHook = ''
              # Need to expose the icon data directory so xdot can find icons
              XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${pkgs.gnome.adwaita-icon-theme}/share
            '';
          };

          checks = { };
        }
      )
    );
}
