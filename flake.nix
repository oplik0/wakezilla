{
  description = "A Wake-on-LAN proxy server written in Rust";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        wakezilla-pkg = import ./pkgs/wakezilla { inherit pkgs crane; };
      in
      {
        packages = {
          default = wakezilla-pkg;
          wakezilla = wakezilla-pkg;
        };

        nixosModules = {
          wakezilla = import ./modules/wakezilla;
        };

        devShells = {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              rustc
              clippy
              rustfmt
              (rust-analyzer.override { extensions = [ "rust-analyzer" ]; })
            ];
            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          };
        };
      }
    );
}
