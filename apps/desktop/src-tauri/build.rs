use std::env;
use std::path::PathBuf;
use std::process::Command;

const MANIFEST_FILE: &str = "windows-app-manifest.xml";

fn main() {
    let mut windows = tauri_build::WindowsAttributes::new();

    // tauri-build embeds the Common Controls v6 manifest only into the app
    // binary. The `cargo test` executables link the same Tauri code (the dialog
    // plugin imports comctl32 v6 functions), so without the manifest they die
    // at startup with STATUS_ENTRYPOINT_NOT_FOUND. Embed it for every target
    // instead, and tell tauri-build not to add a second copy.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        embed_manifest_for_all_targets();
    }

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}

fn embed_manifest_for_all_targets() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join(MANIFEST_FILE);
    println!("cargo:rerun-if-changed={MANIFEST_FILE}");

    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        return;
    }

    // GNU toolchain: compile a one-line resource script with windres.
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let rc_path = out_dir.join("app-manifest.rc");
    let object_path = out_dir.join("app-manifest.o");
    let manifest_literal = manifest.display().to_string().replace('\\', "/");
    // ld warns that this competes with MinGW's built-in default manifest; ours
    // wins, which is what the tests need.
    std::fs::write(&rc_path, format!("1 24 \"{manifest_literal}\"\n"))
        .expect("could not write manifest resource script");

    let windres = env::var("WINDRES").unwrap_or_else(|_| "windres".to_string());
    let status = Command::new(&windres)
        .args(["--input-format=rc", "--output-format=coff", "-i"])
        .arg(&rc_path)
        .arg("-o")
        .arg(&object_path)
        .status()
        .unwrap_or_else(|error| panic!("could not run {windres}: {error}"));
    assert!(status.success(), "{windres} failed to compile the app manifest");

    println!("cargo:rustc-link-arg={}", object_path.display());
}
