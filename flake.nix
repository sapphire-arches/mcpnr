{
  description = "PnR for Minecraft";

  inputs = {
    amulet = {
      url = "github:bobtwinkles/amulet-flake";
      inputs.flake-utils.follows = "flake-utils";
      inputs.nix.follows = "nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    nix = {
      url = "github:nixos/nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "github:nixos/nixpkgs";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nix, nixpkgs, amulet, fenix, flake-utils }:
    {
      overlay = final: prev: {
        mcpnr = prev.callPackage ./nix { };
        mcpnr-rust-platform = (prev.makeRustPlatform {
          inherit (fenix.packages.${prev.system}.stable) cargo rustc rust-src;
        });
      };
    }
    //
    (
      flake-utils.lib.eachSystem [ "x86_64-linux" ] (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [
              amulet.overlay
              fenix.overlay
              self.overlay
            ];
          };
          mcpnrPackages = flake-utils.lib.flattenTree pkgs.mcpnr;
          mcpnrPackagesList = pkgs.lib.attrValues mcpnrPackages;
        in
        {
          packages = mcpnrPackages;

          devShell =
            let
              pythonPackage = pkgs.python39.withPackages (pythonPackages: with pythonPackages; [
                xdot
                amulet-core
              ]);
              rust-gui-pkgs = with pkgs; [
                xorg.libX11
                xorg.libXcursor
                xorg.libXrandr
                xorg.libXi
                xorg.libxcb
                xorg.libXrender
                vulkan-loader
              ];
            in
            pkgs.mkShell {
              name = "mcpnr-devel-shell";

              buildInputs = with pkgs; [
                # Performance testing
                hyperfine

                # Graphics debugging
                renderdoc

                # For formatting Nix expressions
                nixpkgs-fmt

                # For viewing intermediate Yosys outputs
                graphviz

                # For the script that converts placed outputs to Minecraft worlds
                pythonPackage

                # Rust development
                (fenix.packages.${system}.stable.withComponents [
                  "cargo"
                  "rustc"
                  "rust-src"
                  "rustfmt"
                ])
                fenix.packages.${system}.rust-analyzer
              ] ++ (pkgs.lib.concatMap (p: p.buildInputs) mcpnrPackagesList);

              nativeBuildInputs = with pkgs; [
              ] ++ (pkgs.lib.concatMap (p: p.nativeBuildInputs) mcpnrPackagesList);

              shellHook = ''
                # To pick up the specific version of Python we're using
                PYTHONPATH=${pythonPackage}/${pythonPackage.sitePackages}

                # Need to expose the icon data directory so xdot can find icons
                XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${pkgs.gnome.adwaita-icon-theme}/share

                export YOSYS_PROTO_PATH=${pkgs.yosys.src}/misc/yosys.proto
                export RUST_SRC_PATH=${fenix.packages.${system}.stable.rust-src}/lib/rustlib/src/rust/library

                # Required for winit to find graphics libraries
                LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath rust-gui-pkgs}:$LD_LIBRARY_PATH
              '';
            };

          checks = { };
        }
      )
    );
}
