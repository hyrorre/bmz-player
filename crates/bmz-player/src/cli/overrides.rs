//! Validated, invocation-local settings shared by live playback and export.
use crate::config::{
    app_config::{AppConfig, WindowMode},
    profile_config::ProfileConfig,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayOverrides {
    pub values: BTreeMap<String, Value>,
    pub seed: Option<u64>,
}

macro_rules! play_fields {
    ($self:ident, $profile:ident, $capture:ident; $($key:literal => $($field:ident).+),+ $(,)?) => {
        $(if $self.values.contains_key($key) {
            if $capture {
                $self.values.insert($key.into(), json!($profile.$($field).+));
            } else {
                $profile.$($field).+ = serde_json::from_value($self.values[$key].clone())
                    .expect("validated CLI setting");
            }
        })+
    };
}

impl PlayOverrides {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.seed.is_none()
    }
    pub fn affects_replay(&self) -> bool {
        self.seed.is_some()
            || self.values.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "hispeed" | "base-hispeed" | "floating-policy" | "hs-fix" | "bga" | "guide-se"
                )
            })
    }
    fn transfer(&mut self, profile: &mut ProfileConfig, capture: bool) {
        play_fields!(self, profile, capture;
            "arrange" => play.random, "arrange-2p" => play.random2,
            "double-option" => play.double_option, "gauge" => play.gauge,
            "gas" => play.gauge_auto_shift, "gas-bottom" => play.bottom_shiftable_gauge,
            "hs-fix" => play.hs_fix, "hispeed" => lane.hispeed,
            "base-hispeed" => lane.base_hispeed, "floating-policy" => lane.floating_policy, "bga" => play.bga,
            "guide-se" => play.guide_se
        );
    }
    pub fn apply(&self, profile: &mut ProfileConfig) {
        self.clone().transfer(profile, false);
    }
    fn capture(&self, profile: &mut ProfileConfig) -> Self {
        let mut result = self.clone();
        result.transfer(profile, true);
        result
    }
    pub fn validate(&self) -> Result<()> {
        use crate::config::profile_config::*;
        for (key, value) in &self.values {
            macro_rules! check {
                ($ty:ty) => {{
                    let _: $ty = serde_json::from_value(value.clone())
                        .with_context(|| format!("invalid {key}"))?;
                }};
            }
            match key.as_str() {
                "arrange" | "arrange-2p" => check!(RandomOptionConfig),
                "double-option" => check!(DoubleOptionConfig),
                "gauge" => check!(GaugeTypeConfig),
                "gas" => check!(GaugeAutoShiftConfig),
                "gas-bottom" => check!(BottomShiftableGaugeConfig),
                "hs-fix" => check!(HsFixConfig),
                "bga" => check!(BgaModeConfig),
                "guide-se" | "auto-scratch" => check!(bool),
                "base-hispeed" => check!(BaseHispeedConfig),
                "floating-policy" => check!(FloatingPolicyConfig),
                "hispeed" => {
                    let hs: f32 = serde_json::from_value(value.clone())?;
                    ensure!(hs.is_finite() && (0.01..=20.0).contains(&hs), "invalid hispeed");
                }
                _ => bail!("unknown play override: {key}"),
            }
        }
        Ok(())
    }
}

impl ProfileConfig {
    pub fn set_cli_play(&mut self, options: PlayOverrides) {
        self.clear_cli_play();
        if !options.is_empty() {
            let baseline = options.capture(self);
            options.apply(self);
            self.cli_play = Some(Box::new((options, baseline)));
        }
    }
    pub fn clear_cli_play(&mut self) -> Option<PlayOverrides> {
        let state = self.cli_play.take()?;
        state.1.apply(self);
        Some(state.0)
    }
    pub fn cli_seed(&self) -> Option<u64> {
        self.cli_play.as_ref().and_then(|s| s.0.seed)
    }
    pub fn cli_auto_scratch(&self) -> bool {
        self.cli_play
            .as_ref()
            .and_then(|s| s.0.values.get("auto-scratch"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowOverrides {
    pub mode: Option<String>,
    pub size: Option<(u32, u32)>,
    pub monitor: Option<String>,
}
impl WindowOverrides {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
    pub fn apply(&self, app: &mut AppConfig) -> Result<()> {
        if let Some(mode) = &self.mode {
            app.video.mode = match mode.as_str() {
                "windowed" => WindowMode::Windowed,
                "borderless" => WindowMode::BorderlessFullscreen,
                "exclusive" => WindowMode::ExclusiveFullscreen,
                _ => bail!("invalid window mode"),
            };
        }
        if let Some((w, h)) = self.size {
            ensure!(
                w > 0 && h > 0 && w <= 16384 && h <= 16384,
                "window dimensions must be 1..16384"
            );
            ensure!(
                matches!(app.video.mode, WindowMode::Windowed),
                "--window-size requires windowed mode"
            );
            app.video.width = w;
            app.video.height = h;
        }
        if let Some(monitor) = &self.monitor {
            app.video.monitor_name =
                if monitor == "primary" { String::new() } else { monitor.clone() };
        }
        Ok(())
    }
}

impl AppConfig {
    pub fn set_cli_window(&mut self, options: WindowOverrides) -> Result<()> {
        let mut candidate = self.clone();
        options.apply(&mut candidate)?;
        let (mut combined, mut baseline) = self
            .cli_window_baseline
            .as_deref()
            .cloned()
            .unwrap_or_else(|| (WindowOverrides::default(), self.video.clone()));
        if options.mode.is_some() {
            if combined.mode.is_none() {
                baseline.mode = self.video.mode.clone();
            }
            combined.mode = options.mode;
        }
        if options.size.is_some() {
            if combined.size.is_none() {
                baseline.width = self.video.width;
                baseline.height = self.video.height;
            }
            combined.size = options.size;
        }
        if options.monitor.is_some() {
            if combined.monitor.is_none() {
                baseline.monitor_name = self.video.monitor_name.clone();
            }
            combined.monitor = options.monitor;
        }
        self.video = candidate.video;
        if !combined.is_empty() {
            self.cli_window_baseline = Some(Box::new((combined, baseline)));
        }
        Ok(())
    }
}

/// Read-only settings inspection; no app bootstrap, device initialization or media export.
pub fn print_effective(
    command: &super::Command,
    paths: &crate::paths::AppPaths,
    profile_id: Option<&str>,
) -> Result<()> {
    let (play, window, chart, export) = match command {
        super::Command::Run(o) => (
            &o.play_overrides,
            o.window_overrides.clone(),
            o.boot_play_path.clone().map(std::path::PathBuf::from).or_else(|| {
                o.boot_play_sample
                    .then(|| paths.resource_dir.join("songs/sample-playable/sample-playable.bms"))
            }),
            false,
        ),
        super::Command::Export(o) => {
            (&o.play_overrides, WindowOverrides::default(), Some(o.chart.clone()), true)
        }
        _ => bail!("settings inspection requires a playback command"),
    };
    let mut app = if paths.config_toml.exists() {
        crate::config::load::load_app_config(&paths.config_toml)?
    } else {
        AppConfig::default()
    };
    let id = profile_id.unwrap_or(&app.active_profile).to_string();
    let profile_paths = crate::paths::resolve_profile_paths(paths, &id)?;
    let mut profile = crate::config::load::load_profile_config(&profile_paths.profile_toml)?;
    if let Some(path) = &chart {
        use crate::storage::{
            library_db::LibraryDatabase,
            migration::{LIBRARY_MIGRATIONS, run_migrations},
        };
        let mut db = LibraryDatabase::open(std::path::Path::new(":memory:"))?;
        run_migrations(db.conn_mut(), LIBRARY_MIGRATIONS)?;
        let imported = crate::storage::import::import_chart_file(
            &mut db,
            &path.canonicalize()?,
            None,
            play.seed,
            0,
        )?;
        profile.activate_play_mode(imported.chart.metadata.key_mode);
    }
    profile.set_cli_play(play.clone());
    window.apply(&mut app)?;
    let value = json!({"profile": id, "chart": chart, "export": export,
        "window": app.video, "play": profile.play, "lane": profile.lane,
        "cli_play": play, "cli_window": window, "seed": play.seed,
        "source": "profile settings with listed CLI overrides; replay metadata is applied when loading the replay"});
    crate::stdio::stdout_line(format_args!("{}", serde_json::to_string_pretty(&value)?));
    Ok(())
}

pub(super) fn extract(
    args: Vec<String>,
) -> Result<(Vec<String>, PlayOverrides, WindowOverrides, bool)> {
    let mut remaining = Vec::new();
    let mut play = PlayOverrides::default();
    let mut window = WindowOverrides::default();
    let mut print = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--print-effective-options" {
            print = true;
            continue;
        }
        let (flag, inline) =
            arg.split_once('=').map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
        let key = flag.strip_prefix("--").unwrap_or("");
        if !matches!(
            key,
            "window-mode"
                | "window-size"
                | "monitor"
                | "arrange"
                | "arrange-2p"
                | "double-option"
                | "gauge"
                | "gas"
                | "gas-bottom"
                | "hs-fix"
                | "hispeed"
                | "bga"
                | "guide-se"
                | "seed"
                | "auto-scratch"
        ) {
            remaining.push(arg);
            continue;
        }
        let value = inline
            .map(str::to_owned)
            .or_else(|| args.next())
            .with_context(|| format!("{flag} requires a value"))?;
        let lower = value.to_ascii_lowercase();
        match key {
            "window-mode" => {
                ensure!(
                    ["windowed", "borderless", "exclusive"].contains(&lower.as_str()),
                    "invalid --window-mode: {value}"
                );
                window.mode = Some(lower);
            }
            "monitor" => {
                ensure!(!value.is_empty(), "monitor ID is empty");
                window.monitor = Some(if lower == "primary" { lower } else { value });
            }
            "window-size" => {
                let (w, h) =
                    lower.split_once('x').context("--window-size requires WIDTHxHEIGHT")?;
                let (w, h): (u32, u32) = (w.parse()?, h.parse()?);
                ensure!(
                    w > 0 && h > 0 && w <= 16384 && h <= 16384,
                    "window dimensions must be 1..16384"
                );
                window.size = Some((w, h));
            }
            "seed" => {
                play.seed =
                    Some(value.parse().context("--seed requires an unsigned 64-bit integer")?)
            }
            "hispeed" => {
                let hs: f32 = value.parse().context("invalid --hispeed")?;
                ensure!(
                    hs.is_finite() && (0.01..=20.0).contains(&hs),
                    "--hispeed must be 0.01..20.0"
                );
                play.values.insert(key.into(), json!(hs));
                play.values.insert("base-hispeed".into(), json!("Classic"));
                play.values.insert("floating-policy".into(), json!("Disabled"));
            }
            "guide-se" | "auto-scratch" => {
                ensure!(lower == "on" || lower == "off", "{flag} requires on/off");
                play.values.insert(key.into(), json!(lower == "on"));
            }
            _ => {
                let choices: &[(&str, &str)] = match key {
                    "arrange" | "arrange-2p" => &[
                        ("off", "Off"),
                        ("mirror", "Mirror"),
                        ("random", "Random"),
                        ("r-random", "RRandom"),
                        ("s-random", "SRandom"),
                        ("spiral", "Spiral"),
                        ("h-random", "HRandom"),
                        ("all-scratch", "AllScratch"),
                        ("random-ex", "RandomEx"),
                        ("s-random-ex", "SRandomEx"),
                        ("f-random", "FRandom"),
                        ("mf-random", "MFRandom"),
                    ],
                    "double-option" => &[
                        ("off", "Off"),
                        ("flip", "Flip"),
                        ("battle", "Battle"),
                        ("battle-auto-scratch", "BattleAutoScratch"),
                    ],
                    "gauge" => &[
                        ("assist-easy", "AssistEasy"),
                        ("easy", "Easy"),
                        ("normal", "Normal"),
                        ("hard", "Hard"),
                        ("ex-hard", "ExHard"),
                        ("hazard", "Hazard"),
                    ],
                    "gas" => &[
                        ("off", "Off"),
                        ("continue", "Continue"),
                        ("hard-to-groove", "HardToGroove"),
                        ("best-clear", "BestClear"),
                        ("select-to-under", "SelectToUnder"),
                    ],
                    "gas-bottom" => {
                        &[("assist-easy", "AssistEasy"), ("easy", "Easy"), ("normal", "Normal")]
                    }
                    "hs-fix" => &[
                        ("off", "Off"),
                        ("start-bpm", "StartBpm"),
                        ("min-bpm", "MinBpm"),
                        ("max-bpm", "MaxBpm"),
                        ("main-bpm", "MainBpm"),
                    ],
                    "bga" => &[("on", "On"), ("auto", "Auto"), ("off", "Off")],
                    _ => unreachable!(),
                };
                let canonical =
                    choices.iter().find(|(name, _)| *name == lower).with_context(|| {
                        format!(
                            "invalid {flag}: {value}; expected {}",
                            choices.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                        )
                    })?;
                play.values.insert(key.into(), json!(canonical.1));
            }
        }
    }
    play.validate()?;
    Ok((remaining, play, window, print))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Command, parse_cli_command};
    fn parse(s: &str) -> crate::cli::ParsedCommand {
        parse_cli_command(s.split_whitespace()).unwrap()
    }
    struct TestDir(std::path::PathBuf);
    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "bmz-cli-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn shared_flags_apply_to_modes_and_export() {
        for mode in ["", "-a", "-P"] {
            let invocation = parse(&format!(
                "--gauge=HARD chart.bms {mode} --arrange random --gas best-clear --seed 42"
            ));
            let Command::Run(o) = invocation.command else { panic!() };
            assert_eq!(o.play_overrides.values["gauge"], "Hard");
            assert_eq!(o.play_overrides.seed, Some(42));
        }
        let Command::Export(o) =
            parse("--profile default --arrange mirror export video chart.bms -o out.mp4 --seed 42")
                .command
        else {
            panic!()
        };
        assert_eq!(o.seed, Some(42));
        assert_eq!(o.play_overrides.values["arrange"], "Mirror");
    }
    #[test]
    fn no_chart_discards_only_play_overrides_and_validates_values() {
        let invocation = parse("--gauge hard --window-mode borderless");
        let Command::Run(o) = invocation.command else { panic!() };
        assert!(o.play_overrides.is_empty());
        assert_eq!(o.window_overrides.mode.as_deref(), Some("borderless"));
        assert!(!invocation.warnings.is_empty());
        for args in [
            "--gauge typo",
            "--hispeed NaN",
            "chart.bms -r1 --gas off",
            "export video chart.bms -o out.mp4 --window-mode windowed",
            "songs list --gauge hard",
        ] {
            assert!(parse_cli_command(args.split_whitespace()).is_err(), "{args}");
        }
        assert!(
            parse_cli_command("chart.bms -r1 --hispeed 2 --bga off".split_whitespace()).is_ok()
        );
    }
    #[test]
    fn temporary_play_values_survive_mode_switches_but_never_save() {
        use bmz_core::lane::KeyMode;
        let mut profile = ProfileConfig::new_default("default", "Tester", 0);
        profile.lane.hispeed = 1.25;
        profile.sync_active_play_mode();
        profile.activate_play_mode(KeyMode::K14);
        profile.lane.hispeed = 2.25;
        profile.sync_active_play_mode();
        profile.activate_play_mode(KeyMode::K7);
        let original_gauge = profile.play.gauge;
        let Command::Run(o) =
            parse("chart.bms --hispeed 3 --gauge hard --gas off --seed 42").command
        else {
            panic!()
        };
        profile.set_cli_play(o.play_overrides.clone());
        assert_eq!(profile.lane.hispeed, 3.0);
        profile.sync_active_play_mode();
        profile.activate_play_mode(KeyMode::K14);
        assert_eq!(profile.lane.hispeed, 3.0);
        profile.display_name = "Unrelated edit".into();
        let dir = TestDir::new();
        let path = dir.path().join("profile.toml");
        crate::config::save::save_profile_config(&path, &profile).unwrap();
        let mut saved = crate::config::load::load_profile_config(&path).unwrap();
        assert_eq!(saved.play.gauge, original_gauge);
        assert_eq!(saved.display_name, "Unrelated edit");
        assert_eq!(saved.lane.hispeed, 1.25);
        saved.activate_play_mode(KeyMode::K14);
        assert_eq!(saved.lane.hispeed, 2.25);
        // Select entry drops overrides; a subsequent chart uses stored mode values.
        profile.clear_cli_play();
        assert_eq!(profile.lane.hispeed, 2.25);
        assert_eq!(profile.play.gauge, original_gauge);
        assert_eq!(profile.cli_seed(), None);
        profile.activate_play_mode(KeyMode::K7);
        assert_eq!(profile.lane.hispeed, 1.25);
    }
    #[test]
    fn viewer_request_replaces_previous_overrides() {
        let mut profile = ProfileConfig::new_default("default", "Tester", 0);
        let original = profile.play.gauge;
        let Command::Run(o) = parse("chart.bms -P --gauge hard --seed 42").command else {
            panic!()
        };
        profile.set_cli_play(o.play_overrides);
        let request = crate::viewer_ipc::ViewerCommand::Play {
            path: "chart.bms".into(),
            measure: 0,
            battle: false,
            play_overrides: Default::default(),
            window_overrides: Default::default(),
        };
        let request: crate::viewer_ipc::ViewerCommand =
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        let crate::viewer_ipc::ViewerCommand::Play { play_overrides, .. } = request else {
            panic!()
        };
        profile.set_cli_play(play_overrides);
        assert_eq!(profile.play.gauge, original);
        assert_eq!(profile.cli_seed(), None);
    }
    #[test]
    fn window_overrides_do_not_leak_into_saved_config() {
        let mut app = AppConfig::default();
        let baseline = app.video.clone();
        app.set_cli_window(WindowOverrides {
            mode: Some("borderless".into()),
            ..Default::default()
        })
        .unwrap();
        app.set_cli_window(WindowOverrides {
            monitor: Some("secondary".into()),
            ..Default::default()
        })
        .unwrap();
        let dir = TestDir::new();
        let path = dir.path().join("config.toml");
        crate::config::save::save_app_config(&path, &app).unwrap();
        let saved = crate::config::load::load_app_config(&path).unwrap();
        assert_eq!(saved.video.mode, baseline.mode);
        assert_eq!(saved.video.monitor_name, baseline.monitor_name);
        assert!(matches!(app.video.mode, WindowMode::BorderlessFullscreen));
    }
}
