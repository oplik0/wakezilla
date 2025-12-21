{
  description = "A Wake-on-LAN proxy server written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        wakezilla-pkg = import ./pkgs/wakezilla { inherit pkgs crane; };
      in
      {
        packages = {
          default = wakezilla-pkg;
          wakezilla = wakezilla-pkg;
        };

        devShells = {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              rust-analyzer
              trunk
              pkg-config
              openssl
            ];
            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          };
        };
      }
    ) // {
      # NixOS modules should be at the flake's top level, not per-system
      nixosModules = {
        wakezilla = import ./modules/wakezilla;
        default = self.nixosModules.wakezilla;
      };
    };
}
