use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=AX_CONFIG_PATH");
    if let Ok(config_path) = std::env::var("AX_CONFIG_PATH") {
        println!("cargo:rerun-if-changed={config_path}");
    }

    println!("cargo:rustc-link-search={}", out_dir().display());
    println!("cargo::rustc-link-arg=-Tlink.x");
    println!("cargo::rustc-link-arg=-no-pie");
    println!("cargo::rustc-link-arg=-znostart-stop-gc");

    let script = "link.ld";

    println!("cargo:rerun-if-changed={script}");
    let ld_content = std::fs::read_to_string(script).unwrap();

    std::fs::write(out_dir().join("link.x"), ld_content).expect("link.x write failed");
}

fn out_dir() -> PathBuf {
    PathBuf::from(std::env::var("OUT_DIR").unwrap())
}
