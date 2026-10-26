#![cfg(all(windows, debug_assertions))]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bmz_player::config::app_config::AppConfig;
use bmz_player::config::profile_config::ProfileConfig;
use bmz_player::config::save::{save_app_config, save_profile_config};

struct StartupData(PathBuf);

impl StartupData {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir()
            .join(format!("bmz-startup-stack-{}-{stamp}-{id}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for StartupData {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_startup(select: &Path) {
    let data = StartupData::new();
    let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let mut config = AppConfig::default();
    for source in &mut config.tables.sources {
        source.enabled = false;
    }
    config.updates.enabled = false;
    config.input.gamepad_enabled = false;
    save_app_config(&data.0.join("config.toml"), &config).unwrap();
    let mut profile = ProfileConfig::new_default("default", "Stack regression", 0);
    profile.skin.select = select.to_string_lossy().into_owned();
    profile.audio_mix.master_volume = 0;
    save_profile_config(&data.0.join("profiles/default/profile.toml"), &profile).unwrap();

    // A child process exercises the real async main/startup frames and the
    // executable's default 1 MiB stack, rather than the test harness' larger stack.
    let console = data.0.join("console.log");
    let stdout = std::fs::File::create(&console).unwrap();
    let stderr = stdout.try_clone().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_bmz-player"))
        .args(["--smoke-exit-after-frames", "3"])
        .env("BMZ_RESOURCE_DIR", resources)
        .env("BMZ_DATA_DIR", &data.0)
        .env("BMZ_CACHE_DIR", data.0.join("cache"))
        .env("BMZ_LOGS_DIR", data.0.join("logs"))
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("startup timed out:\n{}", std::fs::read_to_string(&console).unwrap());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let log = std::fs::read_to_string(console).unwrap();
    assert!(status.success(), "startup failed: {status}\n{log}");
    let ready = log
        .lines()
        .find(|line| line.contains("startup select skin ready before first frame"))
        .unwrap_or_else(|| panic!("Select readiness missing: {log}"));
    assert!(ready.contains("select_installed=true"), "Select install missing: {log}");
    assert!(log.contains("first surface frame presented"), "first present missing: {log}");
    assert!(
        log.find("startup select skin ready before first frame").unwrap()
            < log.find("first surface frame presented").unwrap(),
        "Select must precede first present: {log}"
    );
    assert!(log.contains("smoke exit frame count reached"), "smoke exit missing: {log}");
}

#[test]
#[ignore = "starts a native Windows window and requires a GPU and audio device"]
fn lr2_select_startup_fits_main_thread_stack() {
    let assets = StartupData::new();
    let skin = assets.0.join("select.lr2skin");
    image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]))
        .save(assets.0.join("atlas.png"))
        .unwrap();
    std::fs::write(
        &skin,
        "#INFORMATION,5,Stack regression,BMZ\n\
         #IMAGE,atlas.png\n\
         #SRC_IMAGE,0,0,0,0,10,10,1,1,0,0\n\
         #DST_IMAGE,0,0,0,0,640,480\n",
    )
    .unwrap();
    assert_startup(&skin);
}

#[test]
#[ignore = "starts a native Windows window; requires WMIX assets, a GPU and audio device"]
fn wmix_select_startup_fits_main_thread_stack_when_available() {
    let skin = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMIX_HD/select/select.lr2skin");
    if !skin.is_file() {
        eprintln!("SKIP missing WMIX_HD/select/select.lr2skin");
        return;
    }
    assert_startup(&skin);
}
