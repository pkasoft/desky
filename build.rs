fn main() {
    println!("cargo:rerun-if-changed=src/config.rs");
    println!("cargo:rerun-if-changed=src/config.rs.example");
    println!("cargo:rerun-if-changed=desky.manifest");
    println!("cargo:rerun-if-changed=desky.rc");

    if !std::path::Path::new("src/config.rs").exists() {
        let _ = std::fs::copy("src/config.rs.example", "src/config.rs");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_manifest();
    }
}

fn embed_windows_manifest() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let manifest_path = std::path::Path::new(&manifest_dir).join("desky.manifest");
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    if target_env == "msvc" {
        println!("cargo:rustc-link-arg-bin=desky=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=desky=/MANIFESTINPUT:{}",
            manifest_path.display()
        );
        return;
    }

    let out_dir = match std::env::var("OUT_DIR") {
        Ok(dir) => dir,
        Err(_) => return,
    };
    let rc_path = std::path::Path::new(&manifest_dir).join("desky.rc");
    let res_obj = std::path::Path::new(&out_dir).join("desky_manifest.res.o");

    let candidates = ["windres", "x86_64-w64-mingw32-windres"];
    for tool in candidates {
        let status = std::process::Command::new(tool)
            .args([
                "-i",
                &rc_path.to_string_lossy(),
                "-O",
                "coff",
                "-o",
                &res_obj.to_string_lossy(),
            ])
            .status();

        if let Ok(s) = status
            && s.success()
        {
            println!("cargo:rustc-link-arg-bin=desky={}", res_obj.display());
            return;
        }
    }

    println!(
        "cargo:warning=Could not run windres to embed desky.manifest; runtime elevation check will still apply."
    );
}
