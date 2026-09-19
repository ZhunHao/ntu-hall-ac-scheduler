fn main() {
    let cfg_path = std::path::Path::new("cfg.toml");
    if !cfg_path.exists() {
        let _ = std::fs::copy("cfg.toml.example", "cfg.toml");
    }
    println!("cargo:rerun-if-changed=cfg.toml");
    println!("cargo:rerun-if-changed=cfg.toml.example");

    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("espidf") {
        embuild::espidf::sysenv::output();
    }
}
