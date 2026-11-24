fn main() {
    println!("cargo:rerun-if-changed=src/config.rs");
    println!("cargo:rerun-if-changed=src/config.rs.example");
    println!("cargo:rerun-if-changed=resky.manifest");
    println!("cargo:rerun-if-changed=resky.rc");

    if !std::path::Path::new("src/config.rs").exists() {
        let _ = std::fs::copy("src/config.rs.example", "src/config.rs");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let _ = embed_resource::compile_for("resky.rc", ["resky"], embed_resource::NONE);
    }
}
