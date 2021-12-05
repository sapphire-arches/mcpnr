{
  description = "PnR for Minecraft";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/master";
    bt-yosys.url = "github:bobtwinkles/yosys/master";
    flake-utils.url = "github:numtide/flake-utils";
    mozilla-overlay = {
      type = "github";
      owner = "mozilla";
      repo = "nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, nixpkgs, mozilla-overlay, bt-yosys, flake-utils }:
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
              self.mcpnr-rust-overlay
              self.overlay
            ];
          };
          mcpnrPackages = flake-utils.lib.flattenTree pkgs.mcpnr;
          mcpnrPackagesList = pkgs.lib.attrValues mcpnrPackages;
        in
        {
          packages = mcpnrPackages;

          devShell = pkgs.mkShell {
            name = "mcpnr-devel-shell";

            buildInputs = with pkgs; [
              # For formatting Nix expressions
              nixpkgs-fmt

              # For viewing intermediate Yosys outputs
              xdot
              graphviz

              # Rust development
              mcpnr-rust-platform.rust.cargo
              (rust-analyzer.override {
                rustPlatform = mcpnr-rust-platform;
              })
            ] ++ (pkgs.lib.concatMap (p: p.buildInputs) mcpnrPackagesList);

            nativeBuildInputs = with pkgs; [
            ] ++ (pkgs.lib.concatMap (p: p.nativeBuildInputs) mcpnrPackagesList);

            shellHook = ''
              # Need to expose the icon data directory so xdot can find icons
              XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:${pkgs.gnome.adwaita-icon-theme}/share

              RUST_SRC_PATH=${pkgs.mcpnr-rust-platform.rustLibSrc}
            '';
          };

          checks = { };
        }
      )
    );
}
