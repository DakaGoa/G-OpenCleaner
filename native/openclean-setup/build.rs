//! Embeds the compiled application into the installer.
//!
//! `build.cmd` builds `openclean` first and copies its exe to `assets/`. If the
//! asset is missing (for example `cargo check` on a clean tree) we emit an empty
//! payload so the workspace still compiles, and the installer reports it.

use std::path::Path;

fn main() {
    let assets = Path::new("assets");
    let exe = assets.join("openclean.exe");
    println!("cargo:rerun-if-changed={}", exe.display());

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dest = Path::new(&out_dir).join("payload.bin");

    if exe.exists() {
        std::fs::copy(&exe, &dest).expect("copy payload");
        println!("cargo:warning=Embedding {}", exe.display());
    } else {
        std::fs::write(&dest, b"").expect("write empty payload");
        println!("cargo:warning=No payload at assets\\openclean.exe — installer will be empty");
    }
}
