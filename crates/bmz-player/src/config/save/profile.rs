use super::*;
use std::time::Instant;

/// Window-thread cost of one config save, split into its stages.
/// Uses copies of real profile.toml / config.toml; writes only to a temp dir.
#[test]
#[ignore = "release config-save probe; set BMZ_PROBE_CONFIG_DIR to a dir with profile.toml and config.toml"]
fn profile_config_save_stages() -> Result<()> {
    use crate::config::load::{load_app_config, load_profile_config};
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    let dir = std::env::var_os("BMZ_PROBE_CONFIG_DIR")
        .ok_or_else(|| anyhow::anyhow!("set BMZ_PROBE_CONFIG_DIR"))?;
    let dir = Path::new(&dir);
    let profile = load_profile_config(&dir.join("profile.toml"))?;
    let app = load_app_config(&dir.join("config.toml"))?;
    let out = tempfile::tempdir()?;
    let profile_path = out.path().join("profile.toml");
    let app_path = out.path().join("config.toml");
    let profile_text = toml::to_string_pretty(&profile)?;
    eprintln!(
        "profile_bytes={} skin_history={} app_bytes={}",
        profile_text.len(),
        profile.skin.history.len(),
        serialize_app_config(&app)?.len()
    );

    let measure = |name: &str, run: &mut dyn FnMut() -> Result<()>| -> Result<()> {
        for _ in 0..10 {
            run()?;
        }
        let mut samples = Vec::new();
        for _ in 0..100 {
            let started = Instant::now();
            run()?;
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
        eprintln!(
            "{name:<24} median={:.3}ms p95={:.3}ms p99={:.3}ms max={:.3}ms",
            at(0.5),
            at(0.95),
            at(0.99),
            samples[samples.len() - 1]
        );
        Ok(())
    };
    measure("profile clone", &mut || {
        std::hint::black_box(profile.clone());
        Ok(())
    })?;
    measure("profile serialize", &mut || {
        std::hint::black_box(toml::to_string_pretty(&profile)?);
        Ok(())
    })?;
    measure("profile write+fsync", &mut || atomic_write(&profile_path, &profile_text))?;
    measure("profile save total", &mut || save_profile_config(&profile_path, &profile))?;
    measure("app save total", &mut || save_app_config(&app_path, &app))?;
    measure("play finish (both)", &mut || {
        save_profile_config(&profile_path, &profile)?;
        save_app_config(&app_path, &app)
    })?;
    Ok(())
}
