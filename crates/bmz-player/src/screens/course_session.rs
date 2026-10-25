use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind};
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::session::AssistLevel;

use crate::ln_policy::{LnPolicySetting, LnScorePolicy};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_session::AppliedArrange;
use crate::screens::play_start::PlayStartOptions;
use crate::screens::result_model::{ResultJudgeCounts, ResultSummary};

/// Library level and difficulty of one course stage. LR2 Course Result skins show
/// every stage of the course, including stages left unplayed after a FAILED.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CourseStageChartInfo {
    pub play_level: String,
    pub difficulty_name: String,
}

#[derive(Clone)]
pub struct ActiveCourseSession {
    pub course_id: i64,
    pub definition: CourseDefinition,
    /// Runtime profile setting captured when this attempt starts.
    pub ln_policy_setting: LnPolicySetting,
    /// Course-wide normalized score key derived from every entry's LN profile.
    pub ln_policy: LnScorePolicy,
    pub rule_mode: RuleMode,
    /// Any converted 7K stage makes the entire course attempt non-persistent.
    pub score_save_disabled: bool,
    /// Course-wide total initialized from library metadata before Decide, then
    /// replaced by the background source import using the entry's actual play
    /// options. This remains course-wide when a Failed chart aborts early.
    pub course_total_notes: u32,
    /// Effective course-wide LN kind. Like `course_total_notes`, this starts
    /// from library metadata and is replaced by the exact background import.
    pub course_ln_mode: Option<bmz_chart::model::LongNoteMode>,
    pub current_index: usize,
    pub entry_results: Vec<CourseEntryResult>,
    /// コース開始時に全エントリ分を確定した開始条件。source chart の事前集計と
    /// 実プレイで同じ LN / DOUBLE / RANDOM 条件を使うため、曲間で作り直さない。
    pub entry_start_options: Vec<PlayStartOptions>,
    /// Library metadata of each definition entry, captured at course start.
    pub stage_charts: Vec<CourseStageChartInfo>,
    /// Replay attempts may end before the full definition after a failed
    /// stage. This bound prevents playback from falling through into normal
    /// interactive play when no later replay was recorded.
    pub replay_stage_limit: Option<usize>,
    /// CLI/smoke boot course playback should progress through intermediate
    /// results without manual input.  Normal select-launched courses wait for
    /// the player on each intermediate result.
    pub auto_advance_intermediate_results: bool,
}

#[derive(Clone)]
pub struct CourseEntryResult {
    pub chart_id: i64,
    pub finished: FinishedPlaySession,
}

#[derive(Debug, Clone)]
pub struct CourseResultSummary {
    pub course_id: i64,
    pub course_score_id: Option<i64>,
    pub course_played_at: Option<i64>,
    pub ln_policy: LnScorePolicy,
    pub rule_mode: RuleMode,
    pub title: String,
    pub kind: CourseKind,
    pub course_titles: [String; 10],
    /// Every definition stage; played stages use the played chart's metadata.
    pub stage_charts: Vec<CourseStageChartInfo>,
    pub entry_summaries: Vec<ResultSummary>,
    /// Per-entry applied arrange (seed/pattern) of this attempt, in play order.
    /// Used to retry the whole course with the same arrangement.
    pub entry_arranges: Vec<AppliedArrange>,
    pub total_ex_score: u32,
    pub max_ex_score: u32,
    pub total_notes: u32,
    /// Effective course-wide LN kind after applying the attempt's settings.
    pub course_ln_mode: Option<bmz_chart::model::LongNoteMode>,
    /// Course-wide BP including unprocessed notes in a failed chart and every
    /// note in the remaining unplayed charts, matching beatoraja course results.
    pub bp: u32,
    pub final_clear_type: ClearType,
    pub final_gauge_type: GaugeType,
    pub final_gauge_value: f32,
    /// Course-wide max combo with combo carry across chart boundaries.
    pub course_max_combo: u32,
    pub judge_counts: ResultJudgeCounts,
    pub trophy_results: Vec<TrophyResult>,
    pub course_clear: bool,
    /// True when the course ended because one chart was Failed
    /// (remaining charts were not played).  Mirrors beatoraja behavior.
    pub course_failed: bool,
    /// Total number of charts in the course definition.
    pub total_entries: usize,
    /// Number of entries the player actually played (includes the failed one).
    pub played_entries: usize,
    pub replay_slots: [bool; 4],
    pub saved_replay_slots: [bool; 4],
    /// Best persisted course score (queried after the current attempt was
    /// inserted, so this reflects the new attempt when it improved the
    /// record).  `None` if persistence is unavailable (autoplay, etc.) or
    /// the lookup failed.
    pub best_score: Option<crate::storage::score_db::CourseBestScore>,
    /// Best persisted course score before the current attempt was inserted.
    /// Result skins use this as MYBEST / diff baseline.
    pub previous_best_score: Option<crate::storage::score_db::CourseBestScore>,
}

#[derive(Debug, Clone)]
pub struct TrophyResult {
    pub name: String,
    pub achieved: bool,
}

impl ActiveCourseSession {
    pub fn current_entry(&self) -> Option<&CourseEntry> {
        self.definition.entries.get(self.current_index)
    }

    /// 中間リザルト後に開始する次ステージの確定済み開始条件を返す。
    ///
    /// コース開始時に固定した arrange/replay 条件へ、直前ステージの gauge と
    /// combo を一度だけ重ねる。Failed 後や未解決 entry では先読みしない。
    pub fn next_stage_start(&self) -> Option<(usize, i64, PlayStartOptions)> {
        let previous = self.entry_results.last();
        if previous.is_some_and(|previous| previous.finished.result.clear_type == ClearType::Failed)
        {
            return None;
        }
        let entry_index = self.current_index;
        if self.replay_stage_limit.is_some_and(|limit| entry_index >= limit) {
            return None;
        }
        let chart_id = self.definition.entries.get(entry_index)?.chart_id?;
        let mut options = self.entry_start_options.get(entry_index)?.clone();
        if let Some(previous) = previous {
            options.initial_gauge_values = Some(previous.finished.gauge_carry.clone());
            options.initial_course_combo = Some(previous.finished.course_combo);
        }
        Some((entry_index, chart_id, options))
    }

    pub fn into_result(self) -> CourseResultSummary {
        let total_entries = self.definition.entries.len();
        let played_entries = self.entry_results.len();
        let played_total_notes = self
            .entry_results
            .iter()
            .fold(0u32, |total, r| total.saturating_add(r.finished.result.total_notes));
        let total_notes = self.course_total_notes.max(played_total_notes);
        let course_failed = played_entries < total_entries
            || self.entry_results.iter().any(|r| r.finished.result.clear_type == ClearType::Failed);
        let played_bp = self
            .entry_results
            .iter()
            .fold(0u32, |total, r| total.saturating_add(r.finished.result.record_bp()));
        let bp = if course_failed {
            played_bp.saturating_add(total_notes.saturating_sub(played_total_notes))
        } else {
            played_bp
        };
        let total_ex_score: u32 =
            self.entry_results.iter().map(|r| r.finished.result.score.ex_score()).sum();
        let max_ex_score: u32 = total_notes.saturating_mul(2);
        let course_max_combo =
            self.entry_results.iter().map(|r| r.finished.course_max_combo).max().unwrap_or(0);

        let judge_counts =
            self.entry_results.iter().fold(ResultJudgeCounts::default(), |acc, r| {
                let j = &r.finished.result.score.judges;
                ResultJudgeCounts {
                    pgreat: acc.pgreat + j.fast_pgreat + j.slow_pgreat,
                    great: acc.great + j.fast_great + j.slow_great,
                    good: acc.good + j.fast_good + j.slow_good,
                    bad: acc.bad + j.fast_bad + j.slow_bad,
                    poor: acc.poor + j.fast_poor + j.slow_poor,
                    empty_poor: acc.empty_poor + j.fast_empty_poor + j.slow_empty_poor,
                }
            });

        let last_result = self.entry_results.last().map(|r| &r.finished.result);
        let assist_level = self
            .entry_results
            .iter()
            .map(|entry| entry.finished.assist.level)
            .max()
            .unwrap_or(AssistLevel::None);
        let final_clear_type = if course_failed {
            ClearType::Failed
        } else {
            match assist_level {
                AssistLevel::Assist => ClearType::AssistEasy,
                AssistLevel::LightAssist => ClearType::LightAssistEasy,
                AssistLevel::None => {
                    if total_notes > 0
                        && judge_counts.bad == 0
                        && judge_counts.poor == 0
                        && judge_counts.pgreat + judge_counts.great + judge_counts.good
                            == total_notes
                        && self.entry_results.iter().all(|entry| {
                            let score = &entry.finished.result.score;
                            score.past_notes == score.combo
                        })
                    {
                        if judge_counts.good > 0 {
                            ClearType::FullCombo
                        } else if judge_counts.great > 0 {
                            ClearType::Perfect
                        } else {
                            ClearType::Max
                        }
                    } else {
                        last_result
                            .map(|result| {
                                // A last-stage FC lamp does not describe the entire course.
                                bmz_gameplay::gauge::gauge_definitions_for_rule_mode(
                                    Default::default(),
                                    self.rule_mode,
                                )
                                .into_iter()
                                .find(|definition| definition.gauge_type == result.gauge_type)
                                .and_then(|definition| definition.clear_type)
                                .unwrap_or(ClearType::Failed)
                            })
                            .unwrap_or(ClearType::NoPlay)
                    }
                }
            }
        };
        let final_gauge_type = last_result.map(|r| r.gauge_type).unwrap_or(GaugeType::Normal);
        let final_gauge_value = last_result.map(|r| r.gauge_value).unwrap_or(0.0);
        let miss_rate = if total_notes > 0 { bp as f32 / total_notes as f32 * 100.0 } else { 0.0 };
        let score_rate = if max_ex_score > 0 {
            total_ex_score as f32 / max_ex_score as f32 * 100.0
        } else {
            0.0
        };

        // Beatoraja awards trophies only when every chart was played (i.e. not failed).
        let trophy_results: Vec<TrophyResult> = self
            .definition
            .trophies
            .iter()
            .map(|trophy| TrophyResult {
                name: trophy.name.clone(),
                achieved: !course_failed
                    && miss_rate <= trophy.max_miss_rate
                    && score_rate >= trophy.min_score_rate,
            })
            .collect();

        // Trophies are optional achievements. As in beatoraja, clearing the
        // course depends on surviving every stage, not on earning a trophy.
        let course_clear = !course_failed;

        let entry_arranges: Vec<AppliedArrange> =
            self.entry_results.iter().map(|r| r.finished.applied_arrange.clone()).collect();
        let course_titles =
            course_titles_from_results(&self.definition.entries, &self.entry_results);
        let stage_charts = stage_charts_from_results(&self.stage_charts, &self.entry_results);
        let entry_summaries = self.entry_results.into_iter().map(|r| r.finished.summary).collect();

        CourseResultSummary {
            course_id: self.course_id,
            course_score_id: None,
            course_played_at: None,
            ln_policy: self.ln_policy,
            rule_mode: self.rule_mode,
            title: self.definition.title,
            kind: self.definition.kind,
            course_titles,
            stage_charts,
            entry_summaries,
            entry_arranges,
            total_ex_score,
            max_ex_score,
            total_notes,
            course_ln_mode: self.course_ln_mode,
            bp,
            final_clear_type,
            final_gauge_type,
            final_gauge_value,
            course_max_combo,
            judge_counts,
            trophy_results,
            course_clear,
            course_failed,
            total_entries,
            played_entries,
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            // Populated separately by the caller (advance_course_after_finish)
            // after persisting this attempt, so the lookup includes the row
            // we just inserted.
            best_score: None,
            previous_best_score: None,
        }
    }
}

fn course_titles_from_entries(entries: &[CourseEntry]) -> [String; 10] {
    let mut titles: [String; 10] = Default::default();
    for (index, entry) in entries.iter().take(10).enumerate() {
        let title = if entry.title_hint.is_empty() { "----" } else { entry.title_hint.as_str() };
        titles[index] =
            if entry.chart_id.is_some() { title.to_string() } else { format!("(no song) {title}") };
    }
    titles
}

fn course_titles_from_results(
    entries: &[CourseEntry],
    results: &[CourseEntryResult],
) -> [String; 10] {
    let mut titles = course_titles_from_entries(entries);
    for (index, result) in results.iter().take(10).enumerate() {
        let title = result.finished.summary.title.trim();
        if !title.is_empty() {
            titles[index] = title.to_string();
        }
    }
    titles
}

fn stage_charts_from_results(
    stage_charts: &[CourseStageChartInfo],
    results: &[CourseEntryResult],
) -> Vec<CourseStageChartInfo> {
    let mut stage_charts = stage_charts.to_vec();
    for (index, result) in results.iter().enumerate() {
        let summary = &result.finished.summary;
        let played = CourseStageChartInfo {
            play_level: summary.play_level.clone(),
            difficulty_name: summary.difficulty_name.clone(),
        };
        match stage_charts.get_mut(index) {
            Some(stage) => *stage = played,
            None => stage_charts.push(played),
        }
    }
    stage_charts
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_core::course::{CourseConstraints, CourseEntry, CourseTrophy};
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::{JudgeCounts, ScoreState};

    fn make_score(pgreat: u32, poor: u32) -> ScoreState {
        ScoreState {
            judges: JudgeCounts { fast_pgreat: pgreat, fast_poor: poor, ..Default::default() },
            ..Default::default()
        }
    }

    #[test]
    fn course_completion_lamp_uses_all_stage_judgements() {
        let mut missed = make_session(1, vec![(make_score(9, 1), 10), (make_score(10, 0), 10)]);
        missed.entry_results[1].finished.result.clear_type = ClearType::Max;
        assert_eq!(missed.into_result().final_clear_type, ClearType::Normal);
        let mut broken = make_session(1, vec![(make_score(10, 0), 10), (make_score(10, 0), 10)]);
        let first = &mut broken.entry_results[0].finished.result.score;
        first.empty_poor_breaks_combo = true;
        first.judges.fast_empty_poor = 1;
        first.past_notes = 10;
        first.combo = 9;
        broken.entry_results[1].finished.result.clear_type = ClearType::Max;
        assert_eq!(broken.into_result().final_clear_type, ClearType::Normal);
        for (good, great, expected) in
            [(1, 0, ClearType::FullCombo), (0, 1, ClearType::Perfect), (0, 0, ClearType::Max)]
        {
            let mut first = make_score(10 - good - great, 0);
            first.judges.fast_good = good;
            first.judges.fast_great = great;
            let mut session = make_session(1, vec![(first, 10), (make_score(10, 0), 10)]);
            session.entry_results[1].finished.result.clear_type = ClearType::Max;
            assert_eq!(session.into_result().final_clear_type, expected);
        }
    }

    fn make_play_result(score: ScoreState, total_notes: u32) -> PlayResult {
        use bmz_core::clear::ClearType;
        make_play_result_with(score, total_notes, ClearType::Normal)
    }

    fn make_play_result_with(
        score: ScoreState,
        total_notes: u32,
        clear_type: bmz_core::clear::ClearType,
    ) -> PlayResult {
        use bmz_chart::hash::compute_chart_identity;
        use bmz_core::clear::GaugeType;
        PlayResult {
            chart_sha256: compute_chart_identity(b"test").file_sha256,
            clear_type,
            gauge_type: GaugeType::Normal,
            gauge_value: 80.0,
            total_notes,
            score,
            autoplay: false,
        }
    }

    fn make_result_chart(total_notes: u32) -> bmz_chart::model::PlayableChart {
        bmz_chart::model::PlayableChart {
            identity: bmz_core::chart::ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: bmz_chart::model::ChartMetadata {
                initial_bpm: 128.0,
                ..bmz_chart::model::ChartMetadata::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes,
            end_time: bmz_core::time::TimeUs(1_000_000),
        }
    }

    fn make_session(course_id: i64, entry_scores: Vec<(ScoreState, u32)>) -> ActiveCourseSession {
        use crate::screens::result_model::ResultSummary;
        use crate::storage::play_result::StoredPlayResult;
        use bmz_core::course::CourseKind;

        let course_total_notes = entry_scores.iter().map(|(_, total_notes)| *total_notes).sum();
        let entries: Vec<CourseEntry> = (0..entry_scores.len())
            .map(|i| CourseEntry {
                title_hint: format!("Song {i}"),
                md5: None,
                sha256: None,
                chart_id: Some(i as i64 + 1),
            })
            .collect();

        let entry_results: Vec<CourseEntryResult> = entry_scores
            .into_iter()
            .enumerate()
            .map(|(i, (score, total_notes))| {
                let result = make_play_result(score, total_notes);
                let course_combo = result.score.combo;
                let course_max_combo = result.score.max_combo;
                CourseEntryResult {
                    chart_id: i as i64 + 1,
                    finished: FinishedPlaySession {
                        result,
                        stored: StoredPlayResult {
                            score_history_id: 0,
                            played_at: 0,
                            replay_path: String::new(),
                            replay_sha256: None,
                            slot_paths: [None, None, None, None],
                            device_type: bmz_core::input::InputDeviceKind::Keyboard,
                        },
                        summary: ResultSummary::from_play_result(
                            &make_play_result(ScoreState::default(), total_notes),
                            &StoredPlayResult {
                                score_history_id: 0,
                                played_at: 0,
                                replay_path: String::new(),
                                replay_sha256: None,
                                slot_paths: [None, None, None, None],
                                device_type: bmz_core::input::InputDeviceKind::Keyboard,
                            },
                            &make_result_chart(total_notes),
                        ),
                        gauge_carry: Vec::new(),
                        course_combo,
                        course_max_combo,
                        replay_playback: false,
                        arrange: crate::select_options::ArrangeOption::Normal,
                        applied_arrange: crate::screens::play_session::AppliedArrange::default(),
                        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
                        double_option: crate::select_options::DoubleOptionScoreBucket::Off,
                        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
                        assist: Default::default(),
                        score_data_changed: false,
                    },
                }
            })
            .collect();

        ActiveCourseSession {
            course_id,
            definition: CourseDefinition {
                key: "test#0".to_string(),
                title: "Test Course".to_string(),
                kind: CourseKind::Dan,
                entries,
                constraints: CourseConstraints::default(),
                trophies: vec![
                    CourseTrophy {
                        name: "gold".to_string(),
                        max_miss_rate: 2.0,
                        min_score_rate: 80.0,
                    },
                    CourseTrophy {
                        name: "silver".to_string(),
                        max_miss_rate: 5.0,
                        min_score_rate: 60.0,
                    },
                ],
                release: true,
            },
            ln_policy_setting: LnPolicySetting::ForceLn,
            ln_policy: LnScorePolicy::ForceLn,
            rule_mode: RuleMode::Beatoraja,
            score_save_disabled: false,
            course_total_notes,
            course_ln_mode: Some(bmz_chart::model::LongNoteMode::Ln),
            current_index: 0,
            entry_results,
            entry_start_options: Vec::new(),
            stage_charts: Vec::new(),
            replay_stage_limit: None,
            auto_advance_intermediate_results: false,
        }
    }

    #[test]
    fn into_result_aggregates_scores() {
        let mut session =
            make_session(1, vec![(make_score(100, 0), 100), (make_score(100, 0), 100)]);
        session.ln_policy = LnScorePolicy::ForceCn;
        session.entry_results[0].finished.course_max_combo = 100;
        session.entry_results[1].finished.course_max_combo = 200;
        let result = session.into_result();
        assert_eq!(result.total_notes, 200);
        assert_eq!(result.max_ex_score, 400);
        assert_eq!(result.total_ex_score, 400);
        assert_eq!(result.course_max_combo, 200);
        assert_eq!(result.judge_counts.pgreat, 200);
        assert_eq!(result.bp, 0);
        assert_eq!(result.ln_policy, LnScorePolicy::ForceCn);
        assert_eq!(result.course_ln_mode, Some(bmz_chart::model::LongNoteMode::Ln));
    }

    #[test]
    fn into_result_keeps_strongest_assist_across_course_for_every_rule_mode() {
        use bmz_gameplay::session::AssistLevel;

        for rule_mode in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
            let mut light = make_session(
                1,
                vec![
                    (make_score(100, 0), 100),
                    (make_score(100, 0), 100),
                    (make_score(100, 0), 100),
                ],
            );
            light.rule_mode = rule_mode;
            light.entry_results[1].finished.assist.level = AssistLevel::LightAssist;
            assert_eq!(light.into_result().final_clear_type, ClearType::LightAssistEasy);

            let mut assist = make_session(
                1,
                vec![
                    (make_score(100, 0), 100),
                    (make_score(100, 0), 100),
                    (make_score(100, 0), 100),
                ],
            );
            assist.rule_mode = rule_mode;
            assist.entry_results[0].finished.assist.level = AssistLevel::LightAssist;
            assist.entry_results[1].finished.assist.level = AssistLevel::Assist;
            assist.entry_results[2].finished.assist.level = AssistLevel::LightAssist;
            assert_eq!(assist.into_result().final_clear_type, ClearType::AssistEasy);
        }
    }

    #[test]
    fn failed_course_overrides_assist_clear() {
        use bmz_gameplay::session::AssistLevel;

        let mut session = make_partial_session(
            2,
            vec![
                (make_score(100, 0), 100, ClearType::Normal),
                (make_score(0, 100), 100, ClearType::Failed),
            ],
        );
        session.entry_results[0].finished.assist.level = AssistLevel::Assist;
        assert_eq!(session.into_result().final_clear_type, ClearType::Failed);
    }

    #[test]
    fn trophy_achieved_when_conditions_met() {
        // 200 notes, 10 poors = 5% miss rate, score_rate = 190/200 = 95%
        let session = make_session(1, vec![(make_score(190, 10), 200)]);
        let result = session.into_result();
        assert_eq!(result.bp, 10);
        // gold: miss_rate <= 2.0 → not achieved (10/200 = 5%)
        // silver: miss_rate <= 5.0 → achieved (exactly 5%), score_rate 95% >= 60%
        assert!(!result.trophy_results[0].achieved);
        assert!(result.trophy_results[1].achieved);
        assert!(result.course_clear);
    }

    #[test]
    fn trophy_not_achieved_when_miss_too_high() {
        // 200 notes, 80 poors = 40% miss rate → neither trophy
        let session = make_session(1, vec![(make_score(100, 80), 200)]);
        let result = session.into_result();
        assert!(!result.trophy_results[0].achieved);
        assert!(!result.trophy_results[1].achieved);
        assert!(result.course_clear);
    }

    #[test]
    fn course_without_trophies_clears_after_surviving_every_stage() {
        let mut session = make_session(1, vec![(make_score(100, 20), 100)]);
        session.definition.trophies.clear();

        let result = session.into_result();

        assert!(result.trophy_results.is_empty());
        assert!(result.course_clear);
    }

    /// Build a session of `entry_count` entries, but only fill `played` of them
    /// with results.  Used to simulate a course that was aborted by a Failed.
    fn make_partial_session(
        entry_count: usize,
        played: Vec<(ScoreState, u32, bmz_core::clear::ClearType)>,
    ) -> ActiveCourseSession {
        use crate::screens::result_model::ResultSummary;
        use crate::storage::play_result::StoredPlayResult;

        let course_total_notes = played.iter().map(|(_, total_notes, _)| *total_notes).sum();
        let entries: Vec<CourseEntry> = (0..entry_count)
            .map(|i| CourseEntry {
                title_hint: format!("Song {i}"),
                md5: None,
                sha256: None,
                chart_id: Some(i as i64 + 1),
            })
            .collect();

        let entry_results: Vec<CourseEntryResult> = played
            .into_iter()
            .enumerate()
            .map(|(i, (score, total_notes, clear_type))| {
                let result = make_play_result_with(score, total_notes, clear_type);
                let course_combo = result.score.combo;
                let course_max_combo = result.score.max_combo;
                let stored = StoredPlayResult {
                    score_history_id: 0,
                    played_at: 0,
                    replay_path: String::new(),
                    replay_sha256: None,
                    slot_paths: [None, None, None, None],
                    device_type: bmz_core::input::InputDeviceKind::Keyboard,
                };
                let summary = ResultSummary::from_play_result(
                    &result,
                    &stored,
                    &make_result_chart(total_notes),
                );
                CourseEntryResult {
                    chart_id: i as i64 + 1,
                    finished: FinishedPlaySession {
                        result,
                        stored,
                        summary,
                        gauge_carry: Vec::new(),
                        course_combo,
                        course_max_combo,
                        replay_playback: false,
                        arrange: crate::select_options::ArrangeOption::Normal,
                        applied_arrange: crate::screens::play_session::AppliedArrange::default(),
                        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
                        double_option: crate::select_options::DoubleOptionScoreBucket::Off,
                        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
                        assist: Default::default(),
                        score_data_changed: false,
                    },
                }
            })
            .collect();

        ActiveCourseSession {
            course_id: 1,
            definition: CourseDefinition {
                key: "test#0".to_string(),
                title: "Test".to_string(),
                kind: bmz_core::course::CourseKind::Dan,
                entries,
                constraints: CourseConstraints::default(),
                trophies: vec![CourseTrophy {
                    name: "gold".to_string(),
                    max_miss_rate: 100.0,
                    min_score_rate: 0.0,
                }],
                release: true,
            },
            ln_policy_setting: LnPolicySetting::ForceLn,
            ln_policy: LnScorePolicy::ForceLn,
            rule_mode: RuleMode::Beatoraja,
            score_save_disabled: false,
            course_total_notes,
            course_ln_mode: None,
            current_index: 0,
            entry_results,
            entry_start_options: Vec::new(),
            stage_charts: Vec::new(),
            replay_stage_limit: None,
            auto_advance_intermediate_results: false,
        }
    }

    #[test]
    fn failed_chart_aborts_course_and_blocks_trophy() {
        use bmz_core::clear::ClearType;
        // 4-entry course, only first 2 played, second is Failed.
        let mut session = make_partial_session(
            4,
            vec![
                (make_score(100, 0), 100, ClearType::Normal),
                (make_score(0, 100), 100, ClearType::Failed),
            ],
        );
        let library = |level: &str| CourseStageChartInfo {
            play_level: level.to_string(),
            difficulty_name: "ANOTHER".to_string(),
        };
        session.stage_charts = vec![library("1"), library("2"), library("3"), library("4")];
        session.entry_results[0].finished.summary.play_level = "11".to_string();
        let result = session.into_result();
        assert!(result.course_failed);
        assert_eq!(result.final_clear_type, ClearType::Failed);
        assert!(!result.course_clear);
        assert_eq!(result.played_entries, 2);
        assert_eq!(result.total_entries, 4);
        // Played stages use the played chart; unplayed ones keep the library metadata.
        let levels = result.stage_charts.iter().map(|chart| chart.play_level.as_str());
        assert_eq!(levels.collect::<Vec<_>>(), ["11", "", "3", "4"]);
        // Trophies are blocked when course_failed even if numeric thresholds pass.
        assert!(!result.trophy_results[0].achieved);
    }

    #[test]
    fn next_stage_start_carries_fixed_options_gauges_and_combo() {
        use bmz_core::clear::{ClearType, GaugeType};
        use bmz_gameplay::gauge::GaugeCarryValue;

        let mut session =
            make_partial_session(2, vec![(make_score(100, 0), 100, ClearType::Normal)]);
        session.current_index = 1;
        session.entry_start_options = vec![PlayStartOptions::default(); 2];
        session.entry_start_options[1].arrange_seed = Some(42);
        session.entry_results[0].finished.gauge_carry =
            vec![GaugeCarryValue { gauge_type: GaugeType::Class, value: 63.5 }];
        session.entry_results[0].finished.course_combo = 87;

        let (entry_index, chart_id, options) = session.next_stage_start().unwrap();

        assert_eq!(entry_index, 1);
        assert_eq!(chart_id, 2);
        assert_eq!(options.arrange_seed, Some(42));
        assert_eq!(
            options.initial_gauge_values,
            Some(vec![GaugeCarryValue { gauge_type: GaugeType::Class, value: 63.5 }])
        );
        assert_eq!(options.initial_course_combo, Some(87));
    }

    #[test]
    fn next_stage_start_rejects_failed_or_unresolved_entries() {
        use bmz_core::clear::ClearType;

        let mut failed =
            make_partial_session(2, vec![(make_score(0, 100), 100, ClearType::Failed)]);
        failed.current_index = 1;
        failed.entry_start_options = vec![PlayStartOptions::default(); 2];
        assert!(failed.next_stage_start().is_none());

        let mut unresolved =
            make_partial_session(2, vec![(make_score(100, 0), 100, ClearType::Normal)]);
        unresolved.current_index = 1;
        unresolved.entry_start_options = vec![PlayStartOptions::default(); 2];
        unresolved.definition.entries[1].chart_id = None;
        assert!(unresolved.next_stage_start().is_none());

        let mut finished =
            make_partial_session(1, vec![(make_score(100, 0), 100, ClearType::Normal)]);
        finished.current_index = 1;
        finished.entry_start_options = vec![PlayStartOptions::default()];
        assert!(finished.next_stage_start().is_none());
    }

    #[test]
    fn partial_replay_limit_never_falls_through_to_interactive_stage() {
        use bmz_core::clear::ClearType;

        let mut session = make_partial_session(
            4,
            vec![
                (make_score(100, 0), 100, ClearType::Normal),
                (make_score(100, 0), 100, ClearType::Normal),
            ],
        );
        session.current_index = 2;
        session.entry_start_options = vec![PlayStartOptions::default(); 4];
        session.replay_stage_limit = Some(2);

        assert!(session.next_stage_start().is_none());
        let result = session.into_result();
        assert!(result.course_failed);
        assert!(!result.course_clear);
        assert_eq!(result.played_entries, 2);
        assert_eq!(result.total_entries, 4);
    }

    #[test]
    fn failed_course_keeps_course_total_notes_as_score_rate_denominator() {
        use bmz_core::clear::ClearType;

        let mut session = make_partial_session(
            4,
            vec![
                (make_score(100, 0), 100, ClearType::Normal),
                (make_score(50, 50), 100, ClearType::Failed),
            ],
        );
        session.course_total_notes = 400;

        let result = session.into_result();

        assert!(result.course_failed);
        assert_eq!(result.total_ex_score, 300);
        assert_eq!(result.total_notes, 400);
        assert_eq!(result.max_ex_score, 800);
    }

    #[test]
    fn failed_course_bp_counts_failed_chart_remainder_and_unplayed_entries() {
        use bmz_core::clear::ClearType;

        // Four 100-note charts. The first chart has 5 BP, then the player fails
        // the second after processing 40 notes with 10 BP. Beatoraja records:
        // 5 + (10 + 60 unprocessed) + 200 unplayed = 275 BP.
        let mut first_score = make_score(95, 5);
        first_score.past_notes = 100;
        let mut failed_score = make_score(30, 10);
        failed_score.past_notes = 40;
        let mut session = make_partial_session(
            4,
            vec![(first_score, 100, ClearType::Normal), (failed_score, 100, ClearType::Failed)],
        );
        session.course_total_notes = 400;

        let result = session.into_result();

        assert_eq!(result.bp, 275);
        assert_eq!(result.judge_counts.poor, 15);
        assert_eq!(result.entry_summaries[0].bp, 5);
        assert_eq!(result.entry_summaries[1].bp, 70);
        assert_eq!(result.played_entries, 2);
        assert_eq!(result.total_entries, 4);
    }

    #[test]
    fn into_result_prefers_played_chart_titles_for_course_titles() {
        use bmz_core::clear::ClearType;

        let mut session = make_partial_session(
            4,
            vec![
                (make_score(100, 0), 100, ClearType::Normal),
                (make_score(0, 100), 100, ClearType::Failed),
            ],
        );
        session.definition.entries[0].title_hint = "Stage One".to_string();
        session.definition.entries[1].title_hint = "Stage Two".to_string();
        session.definition.entries[2].title_hint = "Missing Stage".to_string();
        session.definition.entries[2].chart_id = None;
        session.definition.entries[3].title_hint.clear();
        session.entry_results[0].finished.summary.title = "Resolved One".to_string();
        session.entry_results[1].finished.summary.title = "Resolved Two".to_string();

        let result = session.into_result();

        assert_eq!(result.course_titles[0], "Resolved One");
        assert_eq!(result.course_titles[1], "Resolved Two");
        assert_eq!(result.course_titles[2], "(no song) Missing Stage");
        assert_eq!(result.course_titles[3], "----");
        assert_eq!(result.course_titles[4], "");
    }
}
