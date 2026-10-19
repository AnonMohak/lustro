//! Puts the Lustro NSIS installer inside the setup exe, and passes the Lustro version.
//!
//! The installer comes from `LUSTRO_PAYLOAD`, or else from the normal bundle folder of the app.
//! Without one (for example in `tauri dev`), the setup gets an empty payload and says so.

use std::path::{Path, PathBuf};

fn main() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let conf = here.join("../../src-tauri/tauri.conf.json");
    println!("cargo:rerun-if-changed={}", conf.display());
    let conf: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&conf).expect("read tauri.conf.json"))
            .expect("parse tauri.conf.json");
    let version = conf["version"]
        .as_str()
        .expect("version in tauri.conf.json")
        .to_string();
    println!("cargo:rustc-env=LUSTRO_VERSION={version}");

    println!("cargo:rerun-if-env-changed=LUSTRO_PAYLOAD");
    let payload = std::env::var_os("LUSTRO_PAYLOAD")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            here.join(format!(
                "../../src-tauri/target/release/bundle/nsis/Lustro_{version}_x64-setup.exe"
            ))
        });
    println!("cargo:rerun-if-changed={}", payload.display());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("payload.exe");
    match std::fs::read(&payload) {
        Ok(bytes) => std::fs::write(&out, bytes).expect("write payload"),
        Err(_) => {
            println!(
                "cargo:warning=No Lustro installer at {}. The setup has no payload.",
                payload.display()
            );
            std::fs::write(&out, []).expect("write empty payload");
        }
    }

    tauri_build::build()
}
