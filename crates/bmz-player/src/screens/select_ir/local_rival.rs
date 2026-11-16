use super::*;
use crate::config::profile_config::RivalEntry;
use crate::paths::{AppPaths, resolve_profile_paths};
use crate::storage::migration::SCORE_MIGRATIONS;
use crate::storage::score_db::{BestScoreSummary, ScoreDatabase, ScoreKey};

#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalRivalTarget {
    path: std::path::PathBuf,
    name: String,
}

#[derive(Debug)]
pub enum LocalRivalLoadError {
    OutdatedSchema { found: i32, required: i32 },
    LoadFailed(String),
}

impl LocalRivalLoadError {
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::OutdatedSchema { .. } => "local-rival-schema-outdated",
            Self::LoadFailed(_) => "local-rival-load-failed",
        }
    }

    fn load_failed(error: impl std::fmt::Display) -> Self {
        Self::LoadFailed(format!("{error:#}"))
    }
}

impl std::fmt::Display for LocalRivalLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutdatedSchema { found, required } => {
                write!(f, "local rival score schema {found} requires an update to {required}")
            }
            Self::LoadFailed(error) => f.write_str(error),
        }
    }
}

fn load_local_rival_scores(path: &Path) -> Result<Vec<BestScoreSummary>, LocalRivalLoadError> {
    let db = ScoreDatabase::open_read_only(path).map_err(LocalRivalLoadError::load_failed)?;
    let found = db
        .conn()
        .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))
        .map_err(LocalRivalLoadError::load_failed)?;
    let required = SCORE_MIGRATIONS.last().expect("score migrations are nonempty").version;
    if found < required {
        return Err(LocalRivalLoadError::OutdatedSchema { found, required });
    }
    db.all_best_scores().map_err(LocalRivalLoadError::load_failed)
}

type LocalRivalResult = (u64, Result<Vec<BestScoreSummary>, LocalRivalLoadError>);

pub(super) struct LocalRivalState {
    target: Option<LocalRivalTarget>,
    generation: u64,
    scores: HashMap<ScoreKey, BestScoreSummary>,
    sender: Sender<LocalRivalResult>,
    receiver: Receiver<LocalRivalResult>,
    error: Option<LocalRivalLoadError>,
}

impl Default for LocalRivalState {
    fn default() -> Self {
        let (sender, receiver) = channel();
        Self { target: None, generation: 0, scores: HashMap::new(), sender, receiver, error: None }
    }
}

impl LocalRivalState {
    pub(super) fn name(&self) -> Option<&str> {
        self.target.as_ref().map(|target| target.name.as_str())
    }

    pub(super) fn score(&self, key: ScoreKey) -> Option<&BestScoreSummary> {
        self.scores.get(&key)
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.target = None;
        self.scores.clear();
        self.error = None;
    }

    fn update(&mut self, target: Option<LocalRivalTarget>) {
        if self.target != target {
            self.invalidate();
            self.target = target;
            if let Some(target) = self.target.clone() {
                let generation = self.generation;
                let sender = self.sender.clone();
                if let Err(error) =
                    std::thread::Builder::new().name("local-rival".into()).spawn(move || {
                        let scores = load_local_rival_scores(&target.path);
                        let _ = sender.send((generation, scores));
                    })
                {
                    self.error = Some(LocalRivalLoadError::load_failed(error));
                }
            }
        }
        while let Ok((generation, result)) = self.receiver.try_recv() {
            if generation != self.generation {
                continue;
            }
            match result {
                Ok(scores) => {
                    self.scores = scores
                        .into_iter()
                        .map(|score| {
                            let key = ScoreKey::with_options(
                                score.chart_sha256,
                                score.ln_policy,
                                score.double_option,
                                score.rule_mode,
                            );
                            (key, score)
                        })
                        .collect();
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
}

impl SelectIrRanking {
    #[must_use]
    pub fn update_local_rival(
        &mut self,
        profile: &ProfileConfig,
        paths: &AppPaths,
    ) -> Option<LocalRivalLoadError> {
        let entry = profile.rival.entries.iter().find(|entry| {
            entry.id == profile.rival.active_rival
                && matches!(entry.source, RivalSourceConfig::LocalProfile)
                && entry.profile_id != profile.id
        });
        let target = entry.and_then(|entry: &RivalEntry| {
            resolve_profile_paths(paths, &entry.profile_id).ok().map(|paths| LocalRivalTarget {
                path: paths.score_db,
                name: entry.display_name.clone(),
            })
        });
        self.local_rival.update(target);
        self.local_rival.error.take()
    }

    pub fn refresh_local_rival(&mut self) {
        self.local_rival.invalidate();
    }

    pub fn rival_snapshot_for(&self, key: ScoreKey, ln_mode: u8) -> Option<SelectRivalSnapshot> {
        if let Some(name) = self.local_rival.name() {
            let score = self.local_rival.score(key)?;
            return Some(SelectRivalSnapshot {
                display_name: name.to_string(),
                ex_score: score.ex_score,
                clear_index: i64::from(bmz_core::clear::ClearType::rank_from_label(
                    &score.clear_type,
                )),
                max_combo: score.max_combo,
                bp: score.bp,
                judge_counts: score.numeric_score_available.then_some(SelectRivalJudgeCounts {
                    pgreat: score.judge_counts.pgreat,
                    great: score.judge_counts.great,
                    good: score.judge_counts.good,
                    bad: score.judge_counts.bad,
                    poor: score.judge_counts.poor,
                }),
            });
        }
        self.active_rival_snapshot(key.chart_sha256, ln_mode, key.ln_policy, key.double_option)
    }

    pub fn local_rival_target_for(&self, key: ScoreKey) -> Option<ResolvedTarget> {
        let score = self.local_rival.score(key)?;
        score.numeric_score_available.then(|| ResolvedTarget {
            name: self.local_rival.name().unwrap_or_default().to_string(),
            ex_score: score.ex_score,
        })
    }

    pub fn rival_ex_score_for(&self, key: ScoreKey, ln_mode: u8) -> Option<u32> {
        if self.local_rival.name().is_some() {
            return self.local_rival_target_for(key).map(|target| target.ex_score);
        }
        self.active_rival_score(key.chart_sha256, ln_mode, key.ln_policy, key.double_option)
            .map(|score| score.ex_score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};
    use crate::storage::score_db::{ScoreRecord, ScoreRecordMetadata};
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::input::InputDeviceKind;

    fn record(
        policy: LnScorePolicy,
        double: DoubleOptionScoreBucket,
        rule: RuleMode,
        pg: u32,
    ) -> ScoreRecord {
        let mut result = bmz_gameplay::result::PlayResult {
            chart_sha256: [7; 32],
            clear_type: ClearType::Hard,
            gauge_type: GaugeType::Hard,
            gauge_value: 80.0,
            total_notes: 100,
            score: Default::default(),
            autoplay: false,
        };
        result.score.judges.fast_pgreat = pg;
        ScoreRecord::from_play_result(
            &result,
            ScoreRecordMetadata::new(
                policy,
                double,
                1,
                None,
                "Normal",
                "Hard",
                rule.as_str(),
                0,
                InputDeviceKind::Keyboard,
                "",
            ),
        )
    }

    fn database(path: &Path) -> ScoreDatabase {
        let mut db = ScoreDatabase::open(path).unwrap();
        run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
        db
    }

    fn wait_until(
        state: &mut LocalRivalState,
        target: &LocalRivalTarget,
        done: impl Fn(&LocalRivalState) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done(state) {
            assert!(Instant::now() < deadline, "local rival worker did not finish");
            state.update(Some(target.clone()));
            std::thread::yield_now();
        }
    }

    #[test]
    fn local_scores_use_full_keys_preserve_clear_only_and_do_not_write_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("score.db");
        let mut db = database(&path);
        for (policy, double, rule, pg) in [
            (LnScorePolicy::AutoLn, DoubleOptionScoreBucket::Off, RuleMode::Beatoraja, 10),
            (LnScorePolicy::ForceLn, DoubleOptionScoreBucket::Off, RuleMode::Beatoraja, 20),
            (LnScorePolicy::AutoLn, DoubleOptionScoreBucket::Battle, RuleMode::Beatoraja, 30),
            (LnScorePolicy::AutoLn, DoubleOptionScoreBucket::Off, RuleMode::Dx, 40),
        ] {
            db.insert_score(&record(policy, double, rule, pg)).unwrap();
        }
        let mut clear_only =
            record(LnScorePolicy::AutoLn, DoubleOptionScoreBucket::Off, RuleMode::Beatoraja, 0);
        clear_only.chart_sha256 = [8; 32];
        clear_only.clear_type = ClearType::AssistEasy;
        db.update_score_clear_only(&clear_only).unwrap();
        let before = db.all_best_scores().unwrap();
        assert_eq!(before.len(), 5);
        let readonly = ScoreDatabase::open_read_only(&path).unwrap();
        assert!(
            readonly.conn().execute_batch("CREATE TABLE must_not_write (value INTEGER)").is_err()
        );
        let target = LocalRivalTarget { path, name: "Past self".into() };
        let mut ranking = SelectIrRanking::default();
        ranking.local_rival.update(Some(target.clone()));
        wait_until(&mut ranking.local_rival, &target, |state| {
            state.scores.len() == 5 || state.error.is_some()
        });
        assert!(ranking.local_rival.error.is_none());
        assert_eq!(ranking.active_rival_display_name(), Some("Past self"));
        for score in &before {
            let key = ScoreKey::with_options(
                score.chart_sha256,
                score.ln_policy,
                score.double_option,
                score.rule_mode,
            );
            let snapshot = ranking.rival_snapshot_for(key, 0).unwrap();
            assert_eq!(snapshot.ex_score, score.ex_score);
            if score.chart_sha256 == [8; 32] {
                assert_eq!(snapshot.clear_index, ClearType::AssistEasy as i64);
                assert!(snapshot.judge_counts.is_none());
                assert!(ranking.local_rival_target_for(key).is_none());
                assert!(ranking.rival_ex_score_for(key, 0).is_none());
            } else {
                assert_eq!(ranking.local_rival_target_for(key).unwrap().ex_score, score.ex_score);
            }
        }
        assert_eq!(db.all_best_scores().unwrap(), before);
        assert_eq!(
            db.conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='must_not_write'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn switching_rivals_rejects_old_results_even_after_reselecting_the_same_profile() {
        let dir = tempfile::tempdir().unwrap();
        let target = LocalRivalTarget { path: dir.path().join("unused.db"), name: "Old".into() };
        let mut state = LocalRivalState::default();
        let old_generation = state.generation;
        state.invalidate();
        state.target = Some(target.clone());
        let mut db = database(&dir.path().join("fixture.db"));
        db.insert_score(&record(
            LnScorePolicy::AutoLn,
            DoubleOptionScoreBucket::Off,
            RuleMode::Beatoraja,
            10,
        ))
        .unwrap();
        let old_scores = db.all_best_scores().unwrap();
        let mut new_scores = old_scores.clone();
        new_scores[0].ex_score = 100;
        state.sender.send((state.generation, Ok(new_scores))).unwrap();
        state.sender.send((old_generation, Ok(old_scores))).unwrap();
        state
            .sender
            .send((old_generation, Err(LocalRivalLoadError::LoadFailed("obsolete error".into()))))
            .unwrap();
        state.update(Some(target));
        assert!(state.error.is_none());
        assert_eq!(state.scores.values().next().unwrap().ex_score, 100);
        state.update(None);
        assert!(state.name().is_none());
        assert!(state.scores.is_empty());
    }

    #[test]
    fn missing_source_is_reported_without_creating_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let target =
            LocalRivalTarget { path: dir.path().join("missing.db"), name: "Missing".into() };
        let mut state = LocalRivalState::default();
        state.update(Some(target.clone()));
        wait_until(&mut state, &target, |state| state.error.is_some());
        assert!(!target.path.exists());
        assert!(state.scores.is_empty());
    }

    #[test]
    fn outdated_schemas_request_an_update_without_modifying_the_source() {
        // v1 lacks columns queried today; v36 also predates the v37/v38 data repairs.
        for version in [1, 36] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("score.db");
            let mut db = ScoreDatabase::open(&path).unwrap();
            let migrations = SCORE_MIGRATIONS
                .iter()
                .take_while(|migration| migration.version <= version)
                .count();
            run_migrations(db.conn_mut(), &SCORE_MIGRATIONS[..migrations]).unwrap();
            // Close/checkpoint the writable connection before checking the file bytes.
            drop(db);
            let before = std::fs::read(&path).unwrap();
            let target = LocalRivalTarget { path: path.clone(), name: "Past self".into() };
            let mut state = LocalRivalState::default();
            state.update(Some(target.clone()));
            wait_until(&mut state, &target, |state| state.error.is_some());
            let error = state.error.take().unwrap();
            assert!(
                matches!(error, LocalRivalLoadError::OutdatedSchema { found, .. } if found == version)
            );
            assert_eq!(error.message_key(), "local-rival-schema-outdated");
            assert!(state.scores.is_empty());
            assert_eq!(std::fs::read(&path).unwrap(), before);

            let mut db = ScoreDatabase::open(&path).unwrap();
            run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
            drop(db);
            state.invalidate();
            state.update(Some(target.clone()));
            // Empty scores are valid, so wait for the worker message explicitly.
            let (generation, result) =
                state.receiver.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(generation, state.generation);
            assert!(result.unwrap().is_empty());
        }
    }

    #[test]
    fn local_rival_registration_and_selection_work_without_ir() {
        let mut profile = ProfileConfig::new_default("current", "Current", 0);
        profile.ir.primary_provider.clear();
        for id in ["past", "current", "../invalid"] {
            profile.rival.entries.push(RivalEntry {
                id: format!("local-{id}"),
                display_name: id.into(),
                source: RivalSourceConfig::LocalProfile,
                profile_id: id.into(),
                path: String::new(),
                ir_service: String::new(),
                ir_user_id: String::new(),
            });
        }
        assert_eq!(selectable_rival_ids(&profile), ["local-past"]);
        assert!(!crate::ir_cmd::sync_ir_rivals_into_profile(&mut profile, "rian-ir", &[]));
        assert_eq!(profile.rival.entries.len(), 3);
    }
}
