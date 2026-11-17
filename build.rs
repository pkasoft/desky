fn main() {
    println!("cargo:rerun-if-changed=src/config.rs");
    println!("cargo:rerun-if-changed=src/config.rs.example");
    if !std::path::Path::new("src/config.rs").exists() {
        let _ = std::fs::copy("src/config.rs.example", "src/config.rs");
    }
}
