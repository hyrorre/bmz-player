use super::*;
use crate::{
    config::{
        load::{load_app_config, load_profile_config},
        profile_config::ProfileConfig,
    },
    paths::ProfilePaths,
    screens::play_session::{PreparedPlaySession, load_prepared_play_session_for_chart},
    storage::{
        library_db::LibraryDatabase,
        migration::{LIBRARY_MIGRATIONS, run_migrations},
        score_db::ScoreDatabase,
    },
};

pub struct Prepared {
    pub play: PreparedPlaySession,
    pub profile: ProfileConfig,
    pub best: Option<u32>,
    pub ghost: Option<Vec<u8>>,
    pub replay: Option<crate::storage::replay::ReplayFile>,
    pub seed: u64,
}

pub fn prepare(
    options: &VideoExportOptions,
    paths: &AppPaths,
    profile_id: Option<&str>,
) -> Result<Prepared> {
    let app = if paths.config_toml.exists() {
        load_app_config(&paths.config_toml)?
    } else {
        Default::default()
    };
    let profile_id = profile_id.unwrap_or(&app.active_profile);
    let profile_paths: ProfilePaths = crate::paths::resolve_profile_paths(paths, profile_id)?;
    let mut profile = load_profile_config(&profile_paths.profile_toml)
        .with_context(|| format!("cannot read profile {profile_id}"))?;
    // Parse/cache writes are isolated in an ephemeral library; the user's DBs never migrate.
    let mut library = LibraryDatabase::open(Path::new(":memory:"))?;
    run_migrations(library.conn_mut(), LIBRARY_MIGRATIONS)?;
    let seed = options.seed.unwrap_or(super::random_id()?.as_u128() as u64);
    let chart_path = options.chart.canonicalize().context("chart file does not exist")?;
    let imported =
        crate::storage::import::import_chart_file(&mut library, &chart_path, None, Some(seed), 0)?;
    profile.activate_play_mode(imported.chart.metadata.key_mode);
    let mut play_options = crate::app::offline_play_options(&profile);
    play_options.bms_random_seed = Some(seed);
    play_options.arrange_seed = Some((seed & 0xff_ffff) as i64);
    play_options.arrange_seed_2p = Some(((seed >> 24) & 0xff_ffff) as i64);
    let scores = profile_paths
        .score_db
        .exists()
        .then(|| ScoreDatabase::open_read_only(&profile_paths.score_db))
        .transpose()?;
    let replay = if let Some(slot) = options.replay_slot {
        let key = crate::storage::score_db::ScoreKey::with_options(
            imported.chart.identity.file_sha256,
            crate::ln_policy::score_ln_policy_for_chart(
                profile.play.ln_mode_policy,
                &imported.chart,
            ),
            play_options
                .double_option
                .normalize_for_key_mode(imported.chart.metadata.key_mode)
                .score_bucket(),
            profile.play.rule_mode,
        );
        let record = scores
            .as_ref()
            .context("profile has no score database")?
            .replay_slot(key, slot)?
            .context("replay slot is empty")?;
        let replay_path = profile_paths.root_dir.join(&record.replay_path);
        let file = crate::storage::replay::load_replay_for_chart_policy_and_double_option(
            &replay_path,
            imported.chart.identity.file_sha256,
            record.ln_policy,
            record.double_option,
        )?;
        play_options.session_mode = crate::select_options::SessionMode::Normal;
        play_options.autoplay = false;
        play_options.assist = Default::default();
        play_options.replay_player =
            Some(bmz_gameplay::replay::ReplayPlayer { events: file.events.clone(), next_index: 0 });
        play_options.gauge_override = file.recorded_gauge_type().or(play_options.gauge_override);
        play_options.arrange = file.arrange_option();
        play_options.arrange_2p = file.arrange_2p_option();
        play_options.double_option = file.double_option();
        play_options.arrange_seed = file.arrange_seed;
        play_options.arrange_seed_2p = file.arrange_seed_2p;
        play_options.legacy_arrange_seed = file.uses_legacy_seed_scheme();
        play_options.s_random_scheme = file.effective_s_random_scheme()?;
        play_options.s_random_scheme_2p = Some(file.effective_s_random_scheme_2p()?);
        play_options.h_random_threshold_ms = file.h_random_threshold_ms;
        play_options.bms_random_seed = None;
        play_options.bms_random_choices = file.bms_random_choices.clone();
        play_options.bms_switch_choices = file.bms_switch_choices.clone();
        play_options.arrange_pattern = file.lane_shuffle_pattern.clone();
        if imported.chart.metadata.has_bms_random
            && file.bms_random_choices.is_none()
            && !file.uses_legacy_seed_scheme()
        {
            anyhow::bail!(
                "replay lacks the BMS random branch choices required for reproducible export"
            );
        }
        Some(file)
    } else {
        None
    };
    let mut play =
        load_prepared_play_session_for_chart(&library, imported.chart_id, &profile, play_options)?;
    play.session.offsets.input_offset_us = 0;
    play.session.input_offset_auto_adjust_enabled = false;
    play.session.input_offset_auto_adjust = None;
    let best = scores.as_ref().map(|db| db.best_ex_score(play.score_key)).transpose()?.flatten();
    let ghost = scores
        .as_ref()
        .map(|db| {
            db.best_ghost(
                play.score_key,
                bmz_gameplay::score::scored_note_count(&play.session.chart),
            )
        })
        .transpose()?
        .flatten();
    Ok(Prepared { play, profile, best, ghost, replay, seed })
}
