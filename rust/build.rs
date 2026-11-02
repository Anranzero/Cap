use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=resources/cap.ico");
    println!("cargo:rerun-if-changed=resources/app.manifest");
    println!("cargo:rerun-if-env-changed=RC");
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let icon = root
        .join("resources/cap.ico")
        .to_string_lossy()
        .replace('\\', "\\\\");
    let manifest = root
        .join("resources/app.manifest")
        .to_string_lossy()
        .replace('\\', "\\\\");
    let rc = out.join("cap.rc");
    let res = out.join("cap.res");
    fs::write(&rc, format!("1 ICON \"{icon}\"\n1 24 \"{manifest}\"\n"))
        .expect("Write icon/manifest resource source");
    let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
    let status = Command::new(compiler)
        .arg("/nologo")
        .arg("/c65001")
        .arg(format!("/fo{}", res.display()))
        .arg(&rc)
        .status()
        .expect("Windows SDK rc.exe is required; use build.ps1 from a configured SDK environment");
    assert!(status.success(), "Windows resource compilation failed");
    println!("cargo:rustc-link-arg-bin=cap-rust={}", res.display());
}
