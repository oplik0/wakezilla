{ pkgs, crane }:

let
  inherit (pkgs) lib;
  
  # Point to the actual project root (two levels up from this file)
  unfilteredRoot = ../../.;
  
  rustToolchainFor = p:
    p.rust-bin.stable.latest.default.override {
      targets = [ "wasm32-unknown-unknown" ];
    };
  
  craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchainFor;

  # Filter source files for the main package
  src = lib.fileset.toSource {
    root = unfilteredRoot;
    fileset = lib.fileset.unions [
      # Default files from crane (Rust and cargo files)
      (craneLib.fileset.commonCargoSources unfilteredRoot)
      # Public assets if needed by the backend
      (lib.fileset.maybeMissing (unfilteredRoot + /frontend/public))
    ];
  };

  # Filter source files for the frontend
  frontendSrc = lib.fileset.toSource {
    root = unfilteredRoot + /frontend;
    fileset = lib.fileset.unions [
      # Rust and cargo files in frontend
      (craneLib.fileset.commonCargoSources (unfilteredRoot + /frontend))
      # Frontend HTML, CSS, and config files
      (lib.fileset.fileFilter (file:
        lib.any file.hasExt [
          "html"
          "css"
          "toml"
          "ico"
          "png"
        ]
      ) (unfilteredRoot + /frontend))
      # Public assets
      (lib.fileset.maybeMissing (unfilteredRoot + /frontend/public))
    ];
  };

  # Common arguments for native build
  commonArgs = {
    inherit src;
    strictDeps = true;
    pname = "wakezilla";
    version = "0.1.41";

    nativeBuildInputs = with pkgs; [
      pkg-config
    ];

    buildInputs = with pkgs; [
      openssl
    ] ++ lib.optionals pkgs.stdenv.isDarwin [
      pkgs.libiconv
    ];
  };

  # Build dependencies for native packages
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Build frontend (wasm) - separate source
  wasmArgs = {
    src = frontendSrc;
    strictDeps = true;
    pname = "wakezilla-frontend";
    version = "0.1.41";
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
  };

  cargoArtifactsWasm = craneLib.buildDepsOnly (wasmArgs // {
    doCheck = false;
  });

  # Build the frontend using Trunk
  frontend = craneLib.buildTrunkPackage (wasmArgs // {
    cargoArtifacts = cargoArtifactsWasm;
    
    # Override trunk dist directory to use the default ./dist
    # since the Trunk.toml is configured for development with ../dist
    TRUNK_BUILD_DIST = "./dist";

    # Use wasm-bindgen-cli matching the version in Cargo.lock
    # To update: set hash to lib.fakeHash, try build, use printed hash
    wasm-bindgen-cli = pkgs.buildWasmBindgenCli rec {
      src = pkgs.fetchCrate {
        pname = "wasm-bindgen-cli";
        version = "0.2.104";
        hash = "sha256-9kW+a7IreBcZ3dlUdsXjTKnclVW1C1TocYfY8gUgewE=";
      };

      cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
        inherit src;
        inherit (src) pname version;
        hash = "sha256-V0AV5jkve37a5B/UvJ9B3kwOW72vWblST8Zxs8oDctE=";
      };
    };
  });

  # Build the main server package
  wakezilla = craneLib.buildPackage (commonArgs // {
    inherit cargoArtifacts;
    
    # Include frontend dist as environment variable
    FRONTEND_DIST = frontend;
    
    # If your server needs the frontend at build time, you can include it here
    preBuild = ''
      # Copy frontend dist if needed
      if [ -d "${frontend}" ]; then
        mkdir -p dist
        cp -r ${frontend}/* dist/
      fi
    '';
  });

in
wakezilla
