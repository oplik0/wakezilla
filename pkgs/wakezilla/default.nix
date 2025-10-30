{ pkgs, crane }:

let
  inherit (pkgs) lib stdenv;
  src = pkgs.nix-gitignore.gitignoreSourcePure [
    "*"
    "!*"
  ] ./.;

  craneLib = crane.lib.${pkgs.system}.overrideToolchain (pkgs.rust-bin.stable."1.90.0".default);

  cargoArtifacts = craneLib.buildDepsOnly {
    inherit src;
  };

  wakezilla = craneLib.buildPackage {
    inherit src cargoArtifacts;

    nativeBuildInputs = with pkgs; [
      pkg-config
      trunk
    ];

    buildInputs = with pkgs; [
      openssl
    ];

    preBuild = ''
      pushd frontend
      trunk build --release
      popd
    '';
  };

in
wakezilla
