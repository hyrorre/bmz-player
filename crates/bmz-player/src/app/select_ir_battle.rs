use super::*;

use sha2::{Digest, Sha256};
use std::sync::mpsc::TryRecvError;

const IR_BATTLE_HOLD_DURATION: Duration = Duration::from_millis(120);
const IR_BATTLE_REPLAY_MAX_BYTES: usize = 8 * 1024 * 1024;
const IR_BATTLE_REPLAY_MAX_EVENTS: usize = 2_000_000;

#[derive(Default)]
pub(super) struct SelectIrBattleRuntime {
    pub(super) active: bool,
    pub(super) pinned: bool,
    pub(super) cursor: usize,
    pub(super) source_chart_id: Option<i64>,
    pub(super) source_sha256: Option<[u8; 32]>,
    pub(super) hold_started_at: Option<Instant>,
    pub(super) hold_control: Option<String>,
    pub(super) hold_short_action: Option<SelectAction>,
    pub(super) generation: u64,
    pub(super) loading: bool,
    pub(super) pending: Option<Receiver<SelectIrBattleReplayResult>>,
}

pub(super) struct SelectIrBattleReplayResult {
    generation: u64,
    chart_id: i64,
    target: std::result::Result<crate::screens::play_start::BattleTarget, String>,
}

#[derive(Debug, Clone)]
pub(in crate::app) enum SelectBattleChoice {
    Off,
    MyBest { available: bool },
    Replay { slot: u8, available: bool },
    Rival { name: String, available: bool },
    Ranking(Box<crate::screens::select_ir::SelectIrBattleEntry>),
}

impl WinitApp {
    pub(super) fn select_battle_choices(&self) -> Vec<SelectBattleChoice> {
        let row = self.selected_chart_row();
        let best_available = row
            .and_then(|row| row.best_score.as_ref())
            .is_some_and(|score| !score.replay_path.is_empty());
        let slots = self.selected_chart_replay_slots();
        let rival_name = self
            .boot
            .profile_config
            .rival
            .entries
            .iter()
            .find(|entry| entry.id == self.boot.profile_config.rival.active_rival)
            .map(|entry| entry.display_name.clone())
            .filter(|name| !name.is_empty());
        let mut choices =
            vec![SelectBattleChoice::Off, SelectBattleChoice::MyBest { available: best_available }];
        choices.extend(
            (0..4).map(|slot| SelectBattleChoice::Replay { slot, available: slots[slot as usize] }),
        );
        choices.push(SelectBattleChoice::Rival {
            name: rival_name.clone().unwrap_or_else(|| "--".to_string()),
            available: rival_name.is_some(),
        });
        choices.extend(
            self.select
                .select_ir
                .battle_entries_for(self.select.ir_battle.source_sha256)
                .iter()
                .cloned()
                .map(Box::new)
                .map(SelectBattleChoice::Ranking),
        );
        choices
    }

    fn select_ir_battle_source(&self) -> Option<(i64, [u8; 32], KeyMode)> {
        let row = self.selected_chart_row()?;
        let chart = row.chart.as_ref()?;
        Some((chart.chart_id, row.score_sha256()?, KeyMode::from_str_opt(&chart.mode)?))
    }

    fn select_ir_battle_is_available(&self) -> bool {
        if self.select.select_option_panel != 0
            || self.select.search.is_active()
            || self.select.course_builder.is_some()
            || in_settings_stack(&self.select.folder_stack)
        {
            return false;
        }
        let Some((_, _, _)) = self.select_ir_battle_source() else {
            return false;
        };
        true
    }

    /// KEY4 press is deferred while a usable IR ranking is present. A short
    /// press retains the normal back action; a 120 ms hold temporarily swaps
    /// the song list for the ranking, matching LR2's interaction.
    pub(super) fn begin_select_ir_battle_hold(
        &mut self,
        control: &str,
        short_action: Option<SelectAction>,
    ) -> bool {
        if !self.select_ir_battle_is_available() {
            return false;
        }
        if self.select.ir_battle.active {
            return true;
        }
        let Some((chart_id, sha256, _)) = self.select_ir_battle_source() else {
            return false;
        };
        self.select.ir_battle.source_chart_id = Some(chart_id);
        self.select.ir_battle.source_sha256 = Some(sha256);
        self.select.ir_battle.hold_started_at = Some(Instant::now());
        self.select.ir_battle.hold_control = Some(control.to_string());
        self.select.ir_battle.hold_short_action = short_action;
        true
    }

    pub(super) fn finish_select_ir_battle_hold(&mut self, control: &str) -> bool {
        if self.select.ir_battle.hold_control.as_deref() != Some(control) {
            return false;
        }
        let held_long_enough = self
            .select
            .ir_battle
            .hold_started_at
            .is_some_and(|started| started.elapsed() >= IR_BATTLE_HOLD_DURATION);
        self.select.ir_battle.hold_started_at = None;
        self.select.ir_battle.hold_control = None;
        let short_action = self.select.ir_battle.hold_short_action.take();
        if self.select.ir_battle.active || held_long_enough {
            if !self.select.ir_battle.pinned {
                self.close_select_ir_battle();
            }
        } else {
            if let Some(action) = short_action {
                self.apply_select_action(action, None);
            }
        }
        true
    }

    pub(super) fn advance_select_ir_battle_hold(&mut self) {
        if !self.ui.focused || !matches!(self.view_state(), AppViewState::Select) {
            if self.select.ir_battle.active
                || self.select.ir_battle.hold_started_at.is_some()
                || self.select.ir_battle.pending.is_some()
            {
                self.close_select_ir_battle();
            }
            return;
        }
        if self.select.ir_battle.active && !self.select_ir_battle_is_available() {
            self.close_select_ir_battle();
            return;
        }
        if self.select.ir_battle.active {
            return;
        }
        if self
            .select
            .ir_battle
            .hold_started_at
            .is_some_and(|started| started.elapsed() >= IR_BATTLE_HOLD_DURATION)
        {
            self.select.ir_battle.active = true;
            self.select.ir_battle.cursor = 0;
            self.restart_select_bar_timer_without_scroll(Instant::now());
            self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        }
    }

    pub(super) fn toggle_select_ir_battle(&mut self) -> bool {
        if self.select.ir_battle.active {
            self.close_select_ir_battle();
            return true;
        }
        self.open_select_ir_battle()
    }

    pub(super) fn open_select_ir_battle(&mut self) -> bool {
        if !self.select_ir_battle_is_available() {
            let text = Localizer::new(self.boot.profile_config.ui.locale());
            self.show_left_overlay_toast(text.text("toast-ir-battle-unavailable"));
            return true;
        }
        let Some((chart_id, sha256, _)) = self.select_ir_battle_source() else {
            return true;
        };
        self.select.ir_battle.source_chart_id = Some(chart_id);
        self.select.ir_battle.source_sha256 = Some(sha256);
        self.select.ir_battle.cursor = 0;
        self.select.ir_battle.active = true;
        self.select.ir_battle.pinned = true;
        self.restart_select_bar_timer_without_scroll(Instant::now());
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        true
    }

    pub(super) fn close_select_ir_battle(&mut self) -> bool {
        self.close_select_ir_battle_with_sound(true)
    }

    fn close_select_ir_battle_for_start(&mut self) -> bool {
        self.close_select_ir_battle_with_sound(false)
    }

    fn close_select_ir_battle_with_sound(&mut self, play_close_sound: bool) -> bool {
        let was_open =
            self.select.ir_battle.active || self.select.ir_battle.hold_started_at.is_some();
        self.select.ir_battle.active = false;
        self.select.ir_battle.pinned = false;
        self.select.ir_battle.cursor = 0;
        self.select.ir_battle.source_chart_id = None;
        self.select.ir_battle.source_sha256 = None;
        self.select.ir_battle.hold_started_at = None;
        self.select.ir_battle.hold_control = None;
        self.select.ir_battle.hold_short_action = None;
        self.select.ir_battle.loading = false;
        self.select.ir_battle.pending = None;
        self.select.ir_battle.generation = self.select.ir_battle.generation.wrapping_add(1);
        if was_open {
            self.restart_select_bar_timer_without_scroll(Instant::now());
            if let Some(sound_type) = select_ir_battle_close_sound(was_open, play_close_sound) {
                self.play_system_sound(sound_type);
            }
        }
        was_open
    }

    pub(super) fn move_select_ir_battle(&mut self, select_move: SelectMove, duration: Duration) {
        let len = self.select_battle_choices().len();
        if len == 0 {
            self.close_select_ir_battle();
            return;
        }
        let previous = self.select.ir_battle.cursor;
        self.select.ir_battle.cursor = moved_select_index(previous, len, select_move);
        if previous != self.select.ir_battle.cursor {
            self.select.select_bar_started_at = Instant::now();
            self.select.select_bar_scroll_direction = select_move_scroll_direction(select_move);
            self.select.select_bar_scroll_duration = duration;
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
    }

    pub(super) fn start_selected_battle(&mut self) {
        if self.select.ir_battle.loading {
            return;
        }
        let Some(chart_id) = self.select.ir_battle.source_chart_id else {
            return;
        };
        let Some(choice) = self.select_battle_choices().get(self.select.ir_battle.cursor).cloned()
        else {
            return;
        };
        match choice {
            SelectBattleChoice::Off => {
                let mut options = self.play_start_options();
                options.battle_target = None;
                if self.prepare_session_mode_or_show_error(chart_id, &mut options) {
                    self.close_select_ir_battle_for_start();
                    self.begin_decide_for_chart(chart_id, options);
                }
            }
            SelectBattleChoice::MyBest { available } => {
                if !available {
                    self.show_ir_battle_error("MYBEST has no full replay");
                    return;
                }
                let Some(best) = self.selected_chart_row().and_then(|row| row.best_score.clone())
                else {
                    self.show_ir_battle_error("MYBEST is not available");
                    return;
                };
                let path = self.boot.profile_paths.root_dir.join(&best.replay_path);
                self.start_local_battle_target(
                    chart_id,
                    &path,
                    "MYBEST".to_string(),
                    best.ex_score,
                    gauge_type_from_ir(&best.gauge_type),
                );
            }
            SelectBattleChoice::Replay { slot, available } => {
                if !available {
                    self.show_ir_battle_error(&format!("REPLAY {} is empty", slot + 1));
                    return;
                }
                self.start_replay_slot_battle(chart_id, slot);
            }
            SelectBattleChoice::Rival { available, .. } => {
                if !available || !self.start_active_rival_battle(chart_id) {
                    self.show_ir_battle_error("RIVAL score is not available for this chart");
                }
            }
            SelectBattleChoice::Ranking(entry) => self.start_ir_ranking_battle(chart_id, *entry),
        }
    }

    fn start_ir_ranking_battle(
        &mut self,
        chart_id: i64,
        entry: crate::screens::select_ir::SelectIrBattleEntry,
    ) {
        let Some(sha256) = self.select.ir_battle.source_sha256 else {
            return;
        };
        let key_mode =
            self.select_ir_battle_source().map(|(_, _, key_mode)| key_mode).unwrap_or_default();
        if entry.score_id.as_deref().is_none_or(str::is_empty) && entry.random_seed.is_some() {
            let provider =
                crate::ir::provider_key::primary_provider_config(&self.boot.profile_config.ir)
                    .and_then(crate::ir::provider_key::configured_provider_key)
                    .unwrap_or("rianIR")
                    .to_string();
            let target = seed_battle_target(provider, entry);
            self.launch_battle_target(chart_id, target);
            return;
        }
        let Some(score_id) = entry.score_id.clone().filter(|value| !value.is_empty()) else {
            self.show_ir_battle_error("ranking entry has neither replay nor arrangement seed");
            return;
        };
        let Some(provider) =
            crate::ir::provider_key::primary_provider_config(&self.boot.profile_config.ir).cloned()
        else {
            self.show_ir_battle_error("primary IR provider is not configured");
            return;
        };
        let Some(provider_key) =
            crate::ir::provider_key::configured_provider_key(&provider).map(str::to_string)
        else {
            self.show_ir_battle_error("primary IR provider key is missing");
            return;
        };
        let ln_policy = self
            .selected_chart_row()
            .map(|row| {
                crate::ln_policy::score_ln_policy(
                    self.boot.profile_config.play.ln_mode_policy,
                    row.chart.as_ref().map(|chart| chart.ln_profile).unwrap_or_default(),
                )
            })
            .unwrap_or(crate::ln_policy::LnScorePolicy::ForceLn);
        let replay_dir = self.boot.profile_paths.replay_dir.clone();
        let generation = self.select.ir_battle.generation.wrapping_add(1);
        self.select.ir_battle.generation = generation;
        let (sender, receiver) = mpsc::channel();
        self.select.ir_battle.pending = Some(receiver);
        self.select.ir_battle.loading = true;
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        self.show_left_overlay_toast(text.text("toast-ir-battle-loading"));

        tokio::spawn(async move {
            let target = download_ir_battle_target(
                &provider.base_url,
                &provider_key,
                &score_id,
                chart_id,
                sha256,
                key_mode,
                ln_policy,
                &replay_dir,
                entry,
            )
            .await
            .map_err(|error| format!("{error:#}"));
            let _ = sender.send(SelectIrBattleReplayResult { generation, chart_id, target });
        });
    }

    fn start_local_battle_target(
        &mut self,
        chart_id: i64,
        path: &Path,
        player_name: String,
        ex_score: u32,
        gauge: Option<GaugeType>,
    ) {
        let replay = match crate::storage::replay::load_replay(path) {
            Ok(replay) => replay,
            Err(error) => {
                self.show_ir_battle_error(&format!("failed to load replay: {error:#}"));
                return;
            }
        };
        self.launch_battle_target(
            chart_id,
            crate::screens::play_start::BattleTarget {
                provider: "local".to_string(),
                score_id: String::new(),
                player_id: String::new(),
                player_name,
                rank: 0,
                ex_score,
                gauge,
                playback: crate::screens::play_start::BattleTargetPlayback::Replay(Box::new(
                    replay,
                )),
            },
        );
    }

    fn start_replay_slot_battle(&mut self, chart_id: i64, slot: u8) {
        let Some(row) = self.selected_chart_row() else {
            return;
        };
        let Some(chart) = row.chart.as_ref() else {
            return;
        };
        let key_mode = KeyMode::from_str_opt(&chart.mode).unwrap_or_default();
        let key = crate::storage::score_db::ScoreKey::with_options(
            chart.sha256,
            crate::ln_policy::score_ln_policy(
                self.boot.profile_config.play.ln_mode_policy,
                chart.ln_profile,
            ),
            self.select.double_option.normalize_for_key_mode(key_mode).score_bucket(),
            self.boot.profile_config.play.rule_mode,
        );
        let record = match self.boot.score_db.replay_slot(key, slot) {
            Ok(Some(record)) => record,
            Ok(None) => {
                self.show_ir_battle_error("replay slot is empty");
                return;
            }
            Err(error) => {
                self.show_ir_battle_error(&format!("failed to read replay slot: {error:#}"));
                return;
            }
        };
        let path = self.boot.profile_paths.root_dir.join(&record.replay_path);
        self.start_local_battle_target(
            chart_id,
            &path,
            format!("REPLAY {}", slot + 1),
            record.ex_score.unwrap_or(0),
            None,
        );
    }

    fn start_active_rival_battle(&mut self, chart_id: i64) -> bool {
        let configured_rival = self
            .boot
            .profile_config
            .rival
            .entries
            .iter()
            .find(|entry| entry.id == self.boot.profile_config.rival.active_rival)
            .cloned();
        let Some(row) = self.selected_chart_row() else {
            return false;
        };
        let Some(chart) = row.chart.as_ref() else {
            return false;
        };
        let policy = crate::ln_policy::score_ln_policy(
            self.boot.profile_config.play.ln_mode_policy,
            chart.ln_profile,
        );
        let ln_mode = crate::screens::select_ir::rian_ln_mode_for_chart(chart.ln_profile, policy);
        let Some(score) = self.select.select_ir.active_rival_score(chart.sha256, ln_mode).cloned()
        else {
            let Some(rival) = configured_rival else {
                return false;
            };
            let Some(entry) = self
                .select
                .select_ir
                .battle_entries_for(Some(chart.sha256))
                .iter()
                .find(|entry| entry.player_id == rival.ir_user_id)
                .cloned()
            else {
                return false;
            };
            self.start_ir_ranking_battle(chart_id, entry);
            return true;
        };
        let name = self
            .select
            .select_ir
            .active_rival_display_name()
            .map(str::to_string)
            .or_else(|| configured_rival.map(|rival| rival.display_name))
            .unwrap_or_else(|| "RIVAL".to_string());
        let (arrange, arrange_2p, double_option) =
            super::play_flow_launch_preload::rival_arrange_options(&score);
        let target = crate::screens::play_start::BattleTarget {
            provider: "rianIR".to_string(),
            score_id: String::new(),
            player_id: String::new(),
            player_name: name,
            rank: 0,
            ex_score: score.ex_score,
            gauge: None,
            playback: crate::screens::play_start::BattleTargetPlayback::Seed {
                arrange,
                arrange_2p,
                double_option,
                packed_seed: score.play_seed,
            },
        };
        self.launch_battle_target(chart_id, target);
        true
    }

    fn launch_battle_target(
        &mut self,
        chart_id: i64,
        target: crate::screens::play_start::BattleTarget,
    ) {
        let mut options = self.play_start_options();
        options.battle_target = Some(target);
        if !self.prepare_session_mode_or_show_error(chart_id, &mut options) {
            return;
        }
        self.close_select_ir_battle_for_start();
        if options.session_mode.is_practice() {
            self.enter_practice_with_battle_target(
                chart_id,
                crate::screens::practice::PracticeCliOverrides::default(),
                options.battle_target,
            );
        } else {
            self.begin_decide_for_chart(chart_id, options);
        }
    }

    pub(super) fn poll_select_ir_battle_replay(&mut self) {
        let result = match self.select.ir_battle.pending.as_ref().map(Receiver::try_recv) {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Empty)) | None => return,
            Some(Err(TryRecvError::Disconnected)) => {
                self.select.ir_battle.pending = None;
                self.select.ir_battle.loading = false;
                self.show_ir_battle_error("IR replay worker stopped unexpectedly");
                return;
            }
        };
        self.select.ir_battle.pending = None;
        self.select.ir_battle.loading = false;
        if result.generation != self.select.ir_battle.generation || !self.select.ir_battle.active {
            return;
        }
        let target = match result.target {
            Ok(target) => target,
            Err(error) => {
                tracing::warn!(%error, "failed to prepare IR battle replay");
                self.show_ir_battle_error(&error);
                return;
            }
        };
        self.launch_battle_target(result.chart_id, target);
    }

    pub(super) fn show_ir_battle_error(&mut self, error: &str) {
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let mut args = FluentArgs::new();
        args.set("error", error.to_string());
        self.show_left_overlay_toast(text.format("toast-ir-battle-failed", &args));
    }
}

impl SelectBattleChoice {
    pub(in crate::app) fn title(&self) -> String {
        match self {
            Self::Off => "BATTLE OFF".to_string(),
            Self::MyBest { .. } => "MYBEST".to_string(),
            Self::Replay { slot, .. } => format!("REPLAY {}", slot + 1),
            Self::Rival { name, .. } => format!("RIVAL ({name})"),
            Self::Ranking(entry) => entry.player_name.clone(),
        }
    }
}

pub(in crate::app) fn select_ir_battle_snapshot_rows(
    choices: &[SelectBattleChoice],
    selected_index: usize,
    visible_limit: usize,
    source_row: Option<&SelectRowSnapshot>,
) -> Vec<SelectRowSnapshot> {
    select_visible_item_indices(choices.len(), selected_index, visible_limit)
        .into_iter()
        .map(|index| {
            let mut row = source_row.cloned().unwrap_or_default();
            row.level_display_override = None;
            row.index = index as u32;
            match &choices[index] {
                SelectBattleChoice::Ranking(entry) => {
                    row.title = entry.player_name.clone();
                    row.subtitle = entry.player_id.clone();
                    row.artist = format!("EX {} / BP {}", entry.ex_score, entry.bp);
                    row.genre = "G-BATTLE".to_string();
                    let ranking_level = format!("#{}", entry.rank);
                    row.play_level = ranking_level.clone();
                    row.table_level = ranking_level.clone();
                    row.table_text_secondary = ranking_level;
                    row.show_level = true;
                    row.clear_type = entry.clear.clone();
                    row.ex_score = Some(entry.ex_score);
                    row.max_combo = Some(entry.max_combo);
                    row.bp = Some(entry.bp);
                }
                choice => {
                    let available = match choice {
                        SelectBattleChoice::Off => true,
                        SelectBattleChoice::MyBest { available }
                        | SelectBattleChoice::Replay { available, .. }
                        | SelectBattleChoice::Rival { available, .. } => *available,
                        SelectBattleChoice::Ranking(_) => unreachable!(),
                    };
                    row.title = choice.title();
                    row.subtitle =
                        if available { String::new() } else { "UNAVAILABLE".to_string() };
                    row.artist.clear();
                    row.genre = "G-BATTLE".to_string();
                    row.play_level.clear();
                    row.table_level.clear();
                    row.table_text_secondary.clear();
                    row.show_level = false;
                }
            }
            row
        })
        .collect()
}

fn select_ir_battle_close_sound(
    was_open: bool,
    play_close_sound: bool,
) -> Option<crate::system_sound::SoundType> {
    (was_open && play_close_sound).then_some(crate::system_sound::SoundType::FolderClose)
}

async fn download_ir_battle_target(
    base_url: &str,
    provider: &str,
    score_id: &str,
    _chart_id: i64,
    chart_sha256: [u8; 32],
    key_mode: KeyMode,
    ln_policy: crate::ln_policy::LnScorePolicy,
    replay_dir: &Path,
    entry: crate::screens::select_ir::SelectIrBattleEntry,
) -> Result<crate::screens::play_start::BattleTarget> {
    let client = crate::ir::bmz_official::BmzOfficialIrClient::anonymous(base_url)?;
    let (bytes, metadata) =
        client.download_replay_with_metadata_limit(score_id, IR_BATTLE_REPLAY_MAX_BYTES).await?;
    if metadata.status.as_deref() != Some("verified") {
        anyhow::bail!("IR replay is not verified");
    }
    if metadata.format.as_deref() != Some("bmz-replay-v1") {
        anyhow::bail!("IR replay format is not supported");
    }
    if bytes.len() > IR_BATTLE_REPLAY_MAX_BYTES
        || metadata.size_bytes.is_some_and(|size| size > IR_BATTLE_REPLAY_MAX_BYTES as u64)
    {
        anyhow::bail!("IR replay exceeds the size limit");
    }
    if metadata.size_bytes.is_some_and(|size| size != bytes.len() as u64) {
        anyhow::bail!("IR replay size does not match its metadata");
    }
    let actual_hash = hash_to_hex(&Sha256::digest(&bytes));
    let declared_hash = metadata.hash.as_deref().filter(|hash| !hash.is_empty());
    if declared_hash != Some(actual_hash.as_str()) {
        anyhow::bail!("IR replay hash does not match its metadata");
    }
    let text = std::str::from_utf8(&bytes).context("IR replay is not UTF-8 TOML")?;
    let mut replay = crate::storage::replay::parse_replay(text)?;
    if replay.chart_sha256_bytes()? != chart_sha256 {
        anyhow::bail!("IR replay chart hash does not match the selected chart");
    }
    if !replay.ln_policy.is_empty()
        && crate::ln_policy::LnScorePolicy::from_str_opt(&replay.ln_policy) != Some(ln_policy)
    {
        anyhow::bail!("IR replay long note policy does not match the selected chart");
    }
    if replay.events.len() > IR_BATTLE_REPLAY_MAX_EVENTS {
        anyhow::bail!("IR replay has an invalid event count");
    }
    crate::screens::play_start::normalize_battle_replay_for_key_mode(&mut replay, key_mode)?;

    let provider_cache = hash_to_hex(&Sha256::digest(provider.as_bytes()));
    let score_cache = hash_to_hex(&Sha256::digest(score_id.as_bytes()));
    let cache_path = replay_dir.join("ir").join(provider_cache).join(format!("{score_cache}.toml"));
    crate::storage::replay::save_replay(&cache_path, &replay)
        .with_context(|| format!("failed to cache IR replay: {}", cache_path.display()))?;

    Ok(crate::screens::play_start::BattleTarget {
        provider: provider.to_string(),
        score_id: score_id.to_string(),
        player_id: entry.player_id,
        player_name: entry.player_name,
        rank: entry.rank,
        ex_score: entry.ex_score,
        gauge: entry.gauge.as_deref().and_then(gauge_type_from_ir),
        playback: crate::screens::play_start::BattleTargetPlayback::Replay(Box::new(replay)),
    })
}

fn seed_battle_target(
    provider: String,
    entry: crate::screens::select_ir::SelectIrBattleEntry,
) -> crate::screens::play_start::BattleTarget {
    crate::screens::play_start::BattleTarget {
        provider,
        score_id: entry.score_id.unwrap_or_default(),
        player_id: entry.player_id,
        player_name: entry.player_name,
        rank: entry.rank,
        ex_score: entry.ex_score,
        gauge: entry.gauge.as_deref().and_then(gauge_type_from_ir),
        playback: crate::screens::play_start::BattleTargetPlayback::Seed {
            arrange: entry
                .arrange_1p
                .as_deref()
                .map(super::play_flow_launch_preload::arrange_option_from_rian)
                .unwrap_or_default(),
            arrange_2p: entry
                .arrange_2p
                .as_deref()
                .map(super::play_flow_launch_preload::arrange_option_from_rian)
                .unwrap_or_default(),
            double_option: entry
                .double_option
                .as_deref()
                .map(super::play_flow_launch_preload::double_option_from_rian)
                .unwrap_or_default(),
            packed_seed: entry.random_seed,
        },
    }
}

fn gauge_type_from_ir(value: &str) -> Option<GaugeType> {
    let normalized: String = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    match normalized.as_str() {
        "assisteasy" | "aeasy" => Some(GaugeType::AssistEasy),
        "easy" => Some(GaugeType::Easy),
        "normal" | "groove" | "clear" => Some(GaugeType::Normal),
        "hard" => Some(GaugeType::Hard),
        "exhard" => Some(GaugeType::ExHard),
        "hazard" => Some(GaugeType::Hazard),
        "class" => Some(GaugeType::Class),
        "exclass" => Some(GaugeType::ExClass),
        "exhardclass" => Some(GaugeType::ExHardClass),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranking_rows_keep_source_chart_data_and_target_score_identity() {
        let source = SelectRowSnapshot {
            title: "SOURCE TITLE".to_string(),
            subtitle: "SOURCE SUBTITLE".to_string(),
            artist: "SOURCE ARTIST".to_string(),
            genre: "SOURCE GENRE".to_string(),
            difficulty_name: "ANOTHER".to_string(),
            play_level: "12".to_string(),
            level_display_override: Some("22".into()),
            total_notes: 2253,
            initial_bpm: 155.0,
            min_bpm: 130.0,
            max_bpm: 180.0,
            length_ms: 123_456,
            chart_total_gauge: 400.0,
            chart_peak_density: 42.5,
            ..SelectRowSnapshot::default()
        };
        let rows = select_ir_battle_snapshot_rows(
            &[SelectBattleChoice::Ranking(Box::new(
                crate::screens::select_ir::SelectIrBattleEntry {
                    rank: 2,
                    player_id: "rival-id".to_string(),
                    player_name: "RIVAL".to_string(),
                    score_id: Some("score-id".to_string()),
                    ex_score: 1234,
                    clear: "Hard".to_string(),
                    bp: 7,
                    max_combo: 400,
                    gauge: Some("Hard".to_string()),
                    verification: Some("verified_play".to_string()),
                    arrange_1p: None,
                    arrange_2p: None,
                    random_seed: None,
                    double_option: None,
                },
            ))],
            0,
            25,
            Some(&source),
        );
        assert_eq!(rows[0].title, "RIVAL");
        assert_eq!(rows[0].subtitle, "rival-id");
        assert_eq!(rows[0].artist, "EX 1234 / BP 7");
        assert_eq!(rows[0].genre, "G-BATTLE");
        assert_eq!(rows[0].ex_score, Some(1234));
        assert_eq!(rows[0].max_combo, Some(400));
        assert_eq!(rows[0].bp, Some(7));
        assert_eq!(rows[0].difficulty_name, "ANOTHER");
        assert_eq!(rows[0].play_level, "#2");
        assert_eq!(rows[0].level_display_override, None);
        assert_eq!(rows[0].table_level, "#2");
        assert_eq!(rows[0].table_text_secondary, "#2");
        assert!(rows[0].show_level);
        assert_eq!(rows[0].total_notes, 2253);
        assert_eq!(rows[0].initial_bpm, 155.0);
        assert_eq!(rows[0].min_bpm, 130.0);
        assert_eq!(rows[0].max_bpm, 180.0);
        assert_eq!(rows[0].length_ms, 123_456);
        assert_eq!(rows[0].chart_total_gauge, 400.0);
        assert_eq!(rows[0].chart_peak_density, 42.5);
    }

    #[test]
    fn starting_battle_suppresses_close_se() {
        assert_eq!(select_ir_battle_close_sound(true, false), None);
        assert_eq!(
            select_ir_battle_close_sound(true, true),
            Some(crate::system_sound::SoundType::FolderClose)
        );
        assert_eq!(select_ir_battle_close_sound(false, true), None);
    }

    #[test]
    fn ir_gauge_names_are_normalized() {
        assert_eq!(gauge_type_from_ir("EX-HARD"), Some(GaugeType::ExHard));
        assert_eq!(gauge_type_from_ir("groove"), Some(GaugeType::Normal));
    }

    #[test]
    fn fixed_choices_precede_ir_ranking_and_default_to_battle_off() {
        let choices = [
            SelectBattleChoice::Off,
            SelectBattleChoice::MyBest { available: false },
            SelectBattleChoice::Replay { slot: 0, available: false },
            SelectBattleChoice::Replay { slot: 1, available: false },
            SelectBattleChoice::Replay { slot: 2, available: false },
            SelectBattleChoice::Replay { slot: 3, available: false },
            SelectBattleChoice::Rival { name: "R".to_string(), available: true },
        ];

        assert_eq!(choices[0].title(), "BATTLE OFF");
        assert_eq!(choices[1].title(), "MYBEST");
        assert_eq!(choices[5].title(), "REPLAY 4");
        assert_eq!(choices[6].title(), "RIVAL (R)");

        let source = SelectRowSnapshot {
            play_level: "12".to_string(),
            table_level: "★12".to_string(),
            table_text_secondary: "★12".to_string(),
            ..SelectRowSnapshot::default()
        };
        let rows = select_ir_battle_snapshot_rows(&choices, 0, choices.len(), Some(&source));
        assert!(rows.iter().all(|row| row.play_level.is_empty()));
        assert!(rows.iter().all(|row| row.table_level.is_empty()));
        assert!(rows.iter().all(|row| row.table_text_secondary.is_empty()));
        assert!(rows.iter().all(|row| !row.show_level));
    }
}
