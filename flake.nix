{
  description = "PnR for Minecraft";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    bt-yosys.url = "github:bobtwinkles/yosys/master";
    amulet.url = "github:bobtwinkles/amulet-flake";
    flake-utils.url = "github:numtide/flake-utils";
    mozilla-overlay = {
      type = "github";
      owner = "mozilla";
      repo = "nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, amulet, mozilla-overlay, bt-yosys, flake-utils }:
    {
      mcpnr-rust-overlay = final: prev:
        let
          rustChannel = prev.rustChannelOf {
            channel = "stable";
            sha256 = "sha256-6PfBjfCI9DaNRyGigEmuUP2pcamWsWGc4g7SNEHqD2c=";
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

              bt-yosys.outputs.overlay
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
              # For some reason we need to list all of these explicitly, I'm
              # probaly doing something wrong in the amulet flake.
              pythonPackages.numpy
              pkgs.pymctranslate
              pkgs.amulet-nbt
              pkgs.amulet-core
            ]);
            in
            pkgs.mkShell {
              name = "mcpnr-devel-shell";

              buildInputs = with pkgs; [
                # For formatting Nix expressions
                nixpkgs-fmt

                # For viewing intermediate Yosys outputs
                xdot
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

                export YOSYS_PROTO_PATH=${pkgs.yosys-proto}
                RUST_SRC_PATH=${pkgs.mcpnr-rust-platform.rustLibSrc}
              '';
            };

          checks = { };
        }
      )
    );
}
