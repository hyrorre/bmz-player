fn main() {
    // Source archives carry BUILD-COMMIT; Git checkouts track the real ref too.
    for path in ["../../.git/HEAD", "../../.git/packed-refs", "../../BUILD-COMMIT"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=BMZ_BUILD_COMMIT_OVERRIDE");
    for path in ["../../crates", "../../Cargo.toml", "../../Cargo.lock"] {
        println!("cargo:rerun-if-changed={path}");
    }
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|output| output.trim().to_owned())
    };
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &reference])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let commit = std::env::var("BMZ_BUILD_COMMIT_OVERRIDE")
        .ok()
        .or_else(|| {
            git(&["rev-parse", "HEAD"]).map(|mut hash| {
                if git(&[
                    "status",
                    "--porcelain",
                    "--untracked-files=normal",
                    "--ignore-submodules=all",
                    "--",
                    "../../crates",
                    "../../Cargo.toml",
                    "../../Cargo.lock",
                ])
                .is_some_and(|changes| !changes.is_empty())
                {
                    hash.push_str("-dirty");
                }
                hash
            })
        })
        .or_else(|| std::fs::read_to_string("../../BUILD-COMMIT").ok())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=BMZ_BUILD_COMMIT={}", commit.trim());
    let mut features: Vec<_> = std::env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    features.sort();
    println!("cargo:rustc-env=BMZ_BUILD_FEATURES={}", features.join(","));
    println!("cargo:rustc-check-cfg=cfg(bmz_sparkle)");
    println!("cargo:rerun-if-changed=../../assets/app-icon/bmz-player.ico");
    for key in ["BMZ_SPARKLE_DIR", "BMZ_UPDATE_PUBLIC_KEY"] {
        println!("cargo:rerun-if-env-changed={key}");
    }

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resources = winres::WindowsResource::new();
        resources.set_icon("../../assets/app-icon/bmz-player.ico");
        resources.compile().expect("failed to embed the Windows executable icon");
    }

    println!("cargo:rerun-if-changed=native/sparkle.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    println!("cargo:rerun-if-changed=native/input_common.c");
    cc::Build::new()
        .file("native/input_common.c")
        .flag("-Wno-deprecated-declarations")
        .compile("bmz_input_common");
    for framework in ["CoreFoundation", "Carbon"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
    if std::env::var_os("CARGO_FEATURE_MACOS_IOHID").is_some() {
        println!("cargo:rerun-if-changed=native/keyboard.c");
        cc::Build::new().file("native/keyboard.c").compile("bmz_keyboard");
        println!("cargo:rustc-link-lib=framework=IOKit");
    }
    println!("cargo:rerun-if-changed=native/gamecontroller.m");
    cc::Build::new()
        .file("native/gamecontroller.m")
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .compile("bmz_gamecontroller");
    for framework in ["GameController", "Foundation"] {
        println!("cargo:rustc-link-lib=framework={framework}");
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
