fn main() {
    println!("cargo:rustc-check-cfg=cfg(bmz_sparkle)");
    for key in ["BMZ_SPARKLE_DIR", "BMZ_UPDATE_PUBLIC_KEY"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    println!("cargo:rerun-if-changed=native/sparkle.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Some(directory) = std::env::var_os("BMZ_SPARKLE_DIR") else {
        return;
    };
    let directory =
        std::path::PathBuf::from(directory).canonicalize().expect("invalid BMZ_SPARKLE_DIR");
    assert!(directory.join("Sparkle.framework").is_dir(), "Sparkle.framework missing");
    cc::Build::new()
        .file("native/sparkle.m")
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .flag(format!("-F{}", directory.display()))
        .compile("bmz_sparkle_bridge");
    println!("cargo:rustc-link-search=framework={}", directory.display());
    println!("cargo:rustc-link-lib=framework=Sparkle");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    println!("cargo:rustc-cfg=bmz_sparkle");
}
