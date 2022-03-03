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
    mozilla-overlay = {
      type = "github";
      owner = "mozilla";
      repo = "nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, nix, nixpkgs, amulet, mozilla-overlay, flake-utils }:
    {
      mcpnr-rust-overlay = final: prev:
        let
          rustChannel = prev.rustChannelOf {
            channel = "stable";
            sha256 = "sha256-4IUZZWXHBBxcwRuQm9ekOwzc0oNqH/9NkI1ejW7KajU=";
          };
          rust = rustChannel.rust.override {
            extensions = [
              "rust-std"
              "rust-src"
            ];
          };
          src-ish = prev.stdenv.mkDerivation {
            name = "rust-lib-ish-src";
            src = rustChannel.rust-src;
            phases = [ "unpackPhase" "installPhase" ];

            installPhase = ''
              mv lib/rustlib/src/rust $out
            '';
          };
          mkPlatform = rustDrv: prev.makeRustPlatform {
            rustc = rustDrv // { src = src-ish; };
            cargo = rustDrv;
          };
        in
        rec {
          mcpnr-rust-platform = mkPlatform rustChannel.rust;
        };
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
            overlays = [
              (import mozilla-overlay)

              amulet.overlay
              self.mcpnr-rust-overlay
              self.overlay
            ];
          };
          mcpnrPackages = flake-utils.lib.flattenTree pkgs.mcpnr;
          mcpnrPackagesList = pkgs.lib.attrValues mcpnrPackages;
        in
        {
          packages = mcpnrPackages;

          devShell =
            let pythonPackage = pkgs.python37.withPackages (pythonPackages: [
              # xdot needs to be here or it chooses its own python version
              # (3.9) and then numpy explodes horribly because we installed the
              # numpy built against 3.7
              pythonPackages.xdot
              pkgs.amulet-core
            ]);
            in
            pkgs.mkShell {
              name = "mcpnr-devel-shell";

              buildInputs = with pkgs; [
                # For formatting Nix expressions
                nixpkgs-fmt

                # For viewing intermediate Yosys outputs
                graphviz

                # For the script that converts placed outputs to Minecraft worlds
                pythonPackage

                # Rust development
                mcpnr-rust-platform.rust.cargo
                (rust-analyzer.override {
                  rustPlatform = mcpnr-rust-platform;
                })
              ] ++ (pkgs.lib.concatMap (p: p.buildInputs) mcpnrPackagesList);

              nativeBuildInputs = with pkgs; [
              ] ++ (pkgs.lib.concatMap (p: p.nativeBuildInputs) mcpnrPackagesList);

              shellHook = ''
                # To pick up the specific version of Python we're using
                PYTHONPATH=${pythonPackage}/${pythonPackage.sitePackages}

                # Need to expose the icon data directory so xdot can find icons
                XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${pkgs.gnome.adwaita-icon-theme}/share

                export YOSYS_PROTO_PATH=${pkgs.yosys.src}/misc/yosys.proto
                RUST_SRC_PATH=${pkgs.mcpnr-rust-platform.rustLibSrc}
              '';
            };

          checks = { };
        }
      )
    );
}
