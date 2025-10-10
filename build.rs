use std::env;
use std::path::PathBuf;
use std::process::Command;
use which::which;

fn main() {
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/style");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/Trunk.toml");

    if env::var("SKIP_TRUNK_BUILD").is_ok() {
        println!("cargo:warning=Skipping trunk build because SKIP_TRUNK_BUILD is set");
        return;
    }

    let trunk_binary = ensure_trunk();

    let status = Command::new(trunk_binary)
        .args(["build", "--config", "Trunk.toml"])
        .current_dir(frontend_dir())
        .status()
        .expect("Failed to invoke trunk. Is it installed?");

    if !status.success() {
        panic!("trunk build failed with status {status}");
    }
}

fn frontend_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Missing manifest dir"));
    manifest_dir.join("frontend")
}

fn ensure_trunk() -> PathBuf {
    if let Ok(path) = which("trunk") {
        return path;
    }

    println!(
        "cargo:warning=trunk not found; attempting to install it with `cargo install trunk --locked`"
    );

    let status = Command::new("cargo")
        .args(["install", "trunk", "--locked"])
        .status()
        .expect("Failed to run `cargo install trunk --locked`");

    if !status.success() {
        panic!(
            "Unable to install trunk automatically. Install it manually or set SKIP_TRUNK_BUILD=1"
        );
    }

    which("trunk").expect("trunk installed but binary still not found on PATH")
}
