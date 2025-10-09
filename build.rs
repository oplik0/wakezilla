use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=frontend/src");
    println!("cargo:rerun-if-changed=frontend/style");
    println!("cargo:rerun-if-changed=frontend/index.html");
    println!("cargo:rerun-if-changed=frontend/Trunk.toml");

    if env::var("SKIP_TRUNK_BUILD").is_ok() {
        println!("cargo:warning=Skipping trunk build because SKIP_TRUNK_BUILD is set");
        return;
    }

    let status = Command::new("trunk")
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
