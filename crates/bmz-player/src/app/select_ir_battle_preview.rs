use super::select_ir_battle::SelectBattleChoice;
use super::*;
use crate::screens::play_start::{BattleTargetArrangement, BattleTargetPlayback};
use bmz_render::scene::SelectBattleSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewKey {
    chart_id: i64,
    score: crate::storage::score_db::ScoreKey,
    cursor: usize,
}
type PreviewResult = (PreviewKey, Arc<SelectBattleSnapshot>);

#[derive(Default)]
pub(super) struct SelectBattlePreviewRuntime {
    key: Option<PreviewKey>,
    selected_at: Option<Instant>,
    ready: Option<Arc<SelectBattleSnapshot>>,
    pending: Option<(PreviewKey, Receiver<PreviewResult>)>,
    failed: bool,
}

impl SelectBattlePreviewRuntime {
    fn poll(&mut self, key: PreviewKey) {
        if self.key != Some(key) {
            self.key = Some(key);
            self.selected_at = Some(Instant::now());
            self.ready = None;
            self.failed = false;
        }
        if let Some((pending_key, result)) =
            self.pending.as_ref().map(|(key, receiver)| (*key, receiver.try_recv()))
        {
            match result {
                Ok((completed_key, ready)) => {
                    self.pending = None;
                    if completed_key == key {
                        self.ready = Some(ready);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.failed = pending_key == key;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
}

impl WinitApp {
    fn select_battle_preview_key(&self) -> Option<(PreviewKey, KeyMode)> {
        let (chart_id, sha256, key_mode) = self.select_ir_battle_source()?;
        let chart = self.selected_chart_row()?.chart.as_ref()?;
        let score = crate::storage::score_db::ScoreKey::with_options(
            sha256,
            crate::ln_policy::score_ln_policy(
                self.boot.profile_config.play.ln_mode_policy,
                chart.ln_profile,
            ),
            self.select.double_option.normalize_for_key_mode(key_mode).score_bucket(),
            self.boot.profile_config.play.rule_mode,
        );
        Some((PreviewKey { chart_id, score, cursor: self.select.ir_battle.cursor }, key_mode))
    }

    pub(super) fn select_battle_preview(
        &self,
        choices: &[SelectBattleChoice],
    ) -> Option<Arc<SelectBattleSnapshot>> {
        if !self.select.ir_battle.active {
            return None;
        }
        let (key, key_mode) = self.select_battle_preview_key()?;
        let choice = choices.get(self.select.ir_battle.cursor)?;
        let mut preview =
            SelectBattleSnapshot { name: choice.title(), key_mode, ..Default::default() };
        match choice {
            SelectBattleChoice::MyBest { available }
            | SelectBattleChoice::Replay { available, .. } => {
                if *available {
                    let runtime = &self.select.ir_battle.preview;
                    if runtime.key == Some(key) {
                        if let Some(ready) = &runtime.ready {
                            return Some(ready.clone());
                        }
                        if runtime.failed {
                            return Some(Arc::new(preview));
                        }
                    }
                    preview.loading = true;
                }
            }
            SelectBattleChoice::Ranking(entry) => preview = ranking_preview(entry, key_mode),
            SelectBattleChoice::Rival { .. } => {
                let chart = self.selected_chart_row()?.chart.as_ref()?;
                let policy = crate::ln_policy::score_ln_policy(
                    self.boot.profile_config.play.ln_mode_policy,
                    chart.ln_profile,
                );
                let ln_mode =
                    crate::screens::select_ir::rian_ln_mode_for_chart(chart.ln_profile, policy);
                if let Some(score) = self.select.select_ir.active_rival_score(chart.sha256, ln_mode)
                {
                    let (arrange, arrange_2p, double_option) =
                        super::play_flow_launch_preload::rival_arrange_options(score);
                    let arrangement = BattleTargetPlayback::Seed {
                        arrange,
                        arrange_2p,
                        double_option,
                        packed_seed: score.play_seed,
                    }
                    .arrangement();
                    preview = arrangement_preview(preview.name, key_mode, &arrangement);
                } else if let Some(rival) = self
                    .boot
                    .profile_config
                    .rival
                    .entries
                    .iter()
                    .find(|rival| rival.id == self.boot.profile_config.rival.active_rival)
                    && let Some(entry) = self
                        .select
                        .select_ir
                        .battle_entries_for(Some(chart.sha256))
                        .iter()
                        .find(|entry| entry.player_id == rival.ir_user_id)
                {
                    preview = ranking_preview(entry, key_mode);
                }
            }
            SelectBattleChoice::Off => {}
        }
        Some(Arc::new(preview))
    }

    pub(super) fn poll_select_battle_preview(&mut self) {
        if !self.select.ir_battle.active {
            return;
        }
        let Some((key, key_mode)) = self.select_battle_preview_key() else {
            return;
        };
        let sha256 = key.score.chart_sha256;
        let runtime = &mut self.select.ir_battle.preview;
        runtime.poll(key);
        if runtime.ready.is_some()
            || runtime.failed
            || runtime.pending.is_some()
            || runtime.selected_at.is_none_or(|at| at.elapsed() < Duration::from_millis(120))
        {
            return;
        }
        let choices = self.select_battle_choices();
        let Some(choice) = choices.get(key.cursor) else {
            return;
        };
        let path = match choice {
            SelectBattleChoice::MyBest { available: true } => self
                .selected_chart_row()
                .and_then(|row| row.best_score.as_ref())
                .map(|best| self.boot.profile_paths.root_dir.join(&best.replay_path)),
            SelectBattleChoice::Replay { slot, available: true } => {
                let Some(chart) = self.selected_chart_row().and_then(|row| row.chart.as_ref())
                else {
                    return;
                };
                let score_key = crate::storage::score_db::ScoreKey::with_options(
                    chart.sha256,
                    crate::ln_policy::score_ln_policy(
                        self.boot.profile_config.play.ln_mode_policy,
                        chart.ln_profile,
                    ),
                    self.select.double_option.normalize_for_key_mode(key_mode).score_bucket(),
                    self.boot.profile_config.play.rule_mode,
                );
                self.boot
                    .score_db
                    .replay_slot(score_key, *slot)
                    .ok()
                    .flatten()
                    .map(|record| self.boot.profile_paths.root_dir.join(record.replay_path))
            }
            _ => return,
        };
        let name = choice.title();
        let Some(path) = path else {
            self.select.ir_battle.preview.ready =
                Some(Arc::new(SelectBattleSnapshot { name, key_mode, ..Default::default() }));
            return;
        };
        let (sender, receiver) = mpsc::channel();
        self.select.ir_battle.preview.pending = Some((key, receiver));
        // At most one worker is in flight; scrolling only replaces the requested key.
        tokio::task::spawn_blocking(move || {
            let preview = crate::storage::replay::load_replay_for_chart(&path, sha256)
                .map(|replay| {
                    arrangement_preview(
                        name.clone(),
                        key_mode,
                        &BattleTargetPlayback::Replay(Box::new(replay)).arrangement(),
                    )
                })
                .unwrap_or_else(|_| SelectBattleSnapshot { name, key_mode, ..Default::default() });
            let _ = sender.send((key, Arc::new(preview)));
        });
    }
}

fn arrangement_preview(
    name: String,
    key_mode: KeyMode,
    arrangement: &BattleTargetArrangement,
) -> SelectBattleSnapshot {
    SelectBattleSnapshot {
        name,
        key_mode,
        arrange: Some(arrangement.arrange.as_str().to_string()),
        arrange_2p: Some(arrangement.arrange_2p.as_str().to_string()),
        double_option: Some(arrangement.double_option.as_str().to_string()),
        lane_shuffle_pattern: crate::screens::play_session::preview_arrange_pattern(
            key_mode,
            arrangement,
        )
        .unwrap_or_default(),
        loading: false,
    }
}

fn known_arrange(value: Option<&str>) -> Option<ArrangeOption> {
    let value = value?.trim().to_ascii_uppercase().replace(['-', '_', ' '], "");
    if matches!(value.as_str(), "NONE" | "OFF") {
        return Some(ArrangeOption::Normal);
    }
    ArrangeOption::VALUES.into_iter().find(|option| {
        option.as_str().replace('-', "") == value
            || option.to_persistent_str().to_ascii_uppercase() == value
    })
}

fn ranking_preview(
    entry: &crate::screens::select_ir::SelectIrBattleEntry,
    key_mode: KeyMode,
) -> SelectBattleSnapshot {
    let arrange = known_arrange(entry.arrange_1p.as_deref());
    let arrange_2p = known_arrange(entry.arrange_2p.as_deref());
    let double_option = entry.double_option.as_deref().and_then(|value| {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => Some(DoubleOption::Off),
            "flip" => Some(DoubleOption::Flip),
            "battle" => Some(DoubleOption::Battle),
            "battle_auto_scratch" | "battle_assist" => Some(DoubleOption::BattleAutoScratch),
            _ => None,
        }
    });
    let mut preview = SelectBattleSnapshot {
        name: entry.player_name.clone(),
        key_mode,
        arrange: arrange.map(|value| value.as_str().to_string()),
        arrange_2p: arrange_2p.map(|value| value.as_str().to_string()),
        double_option: double_option.map(|value| value.as_str().to_string()),
        ..Default::default()
    };
    if let Some(arrange) = arrange
        && (!matches!(key_mode, KeyMode::K10 | KeyMode::K14)
            || (arrange_2p.is_some() && double_option.is_some()))
    {
        let arrangement = BattleTargetPlayback::Seed {
            arrange,
            arrange_2p: arrange_2p.unwrap_or_default(),
            double_option: double_option.unwrap_or_default(),
            packed_seed: entry.random_seed,
        }
        .arrangement();
        preview.lane_shuffle_pattern =
            crate::screens::play_session::preview_arrange_pattern(key_mode, &arrangement)
                .unwrap_or_default();
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battle_preview_discards_completed_or_failed_workers_after_cursor_and_chart_changes() {
        let first = PreviewKey {
            chart_id: 1,
            cursor: 1,
            score: crate::storage::score_db::ScoreKey::new(
                [1; 32],
                crate::ln_policy::LnScorePolicy::ForceLn,
            ),
        };
        let mut changed_score = first;
        changed_score.score.chart_sha256 = [2; 32];
        for next in
            [PreviewKey { cursor: 2, ..first }, PreviewKey { chart_id: 2, ..first }, changed_score]
        {
            let mut runtime = SelectBattlePreviewRuntime::default();
            runtime.poll(first);
            let (sender, receiver) = mpsc::channel();
            runtime.pending = Some((first, receiver));
            sender.send((first, Arc::new(SelectBattleSnapshot::default()))).unwrap();
            runtime.poll(next);
            assert!(runtime.ready.is_none());
            assert!(runtime.pending.is_none());
            let (sender, receiver) = mpsc::channel();
            runtime.pending = Some((first, receiver));
            drop(sender);
            runtime.poll(next);
            assert!(!runtime.failed);
            let (sender, receiver) = mpsc::channel();
            runtime.pending = Some((next, receiver));
            drop(sender);
            runtime.poll(next);
            assert!(runtime.failed);
        }
    }

    #[test]
    fn battle_preview_ranking_metadata_stays_unknown_and_zero_seed_is_valid() {
        for option in ArrangeOption::VALUES {
            assert_eq!(known_arrange(Some(option.as_str())), Some(option));
            assert_eq!(known_arrange(Some(option.to_persistent_str())), Some(option));
        }
        assert_eq!(known_arrange(Some("all-scratch")), Some(ArrangeOption::AllScratch));
        let mut entry = crate::screens::select_ir::SelectIrBattleEntry {
            rank: 1,
            player_id: "player".into(),
            player_name: "PLAYER".into(),
            score_id: Some("replay".into()),
            ex_score: 1000,
            clear: "HARD".into(),
            bp: 1,
            max_combo: 500,
            gauge: Some("HARD".into()),
            verification: None,
            arrange_1p: None,
            arrange_2p: None,
            random_seed: None,
            double_option: None,
        };
        let missing = ranking_preview(&entry, KeyMode::K7);
        assert!(missing.arrange.is_none());
        assert!(missing.lane_shuffle_pattern.is_empty());
        entry.arrange_1p = Some("random".into());
        assert!(ranking_preview(&entry, KeyMode::K7).lane_shuffle_pattern.is_empty());
        entry.random_seed = Some(0);
        assert_eq!(ranking_preview(&entry, KeyMode::K7).lane_shuffle_pattern.len(), 16);
        entry.arrange_1p = Some("unrecognized".into());
        assert!(ranking_preview(&entry, KeyMode::K7).arrange.is_none());
        entry.arrange_1p = Some("s-random".into());
        let dynamic = ranking_preview(&entry, KeyMode::K7);
        assert_eq!(dynamic.arrange.as_deref(), Some("S-RANDOM"));
        assert!(dynamic.lane_shuffle_pattern.is_empty());
    }
}
