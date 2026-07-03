use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind};
use bmz_gameplay::rule::RuleMode;

use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_session::AppliedArrange;
use crate::screens::result_model::{ResultJudgeCounts, ResultSummary};
use crate::storage::replay::QueuedCourseReplay;

pub struct ActiveCourseSession {
    pub course_id: i64,
    pub definition: CourseDefinition,
    pub current_index: usize,
    pub entry_results: Vec<CourseEntryResult>,
    /// Pre-loaded replays, one per course entry, when the course is being
    /// played back from a saved attempt.  Empty for a fresh course play.
    /// Indexed by entry position; absence at the current_index means the
    /// chart is played normally (e.g. saved replay file is missing).
    pub queued_replays: Vec<QueuedCourseReplay>,
    /// Per-entry arrange to reproduce when retrying the whole course with the
    /// same arrangement.  Indexed by entry position; absence at an index means
    /// that chart gets a fresh arrange (e.g. a chart never reached because the
    /// previous attempt failed early).  Empty for a fresh course play.
    pub arrange_overrides: Vec<AppliedArrange>,
    /// CLI/smoke boot course playback should progress through intermediate
    /// results without manual input.  Normal select-launched courses wait for
    /// the player on each intermediate result.
    pub auto_advance_intermediate_results: bool,
}

pub struct CourseEntryResult {
    pub chart_id: i64,
    pub finished: FinishedPlaySession,
}

#[derive(Debug, Clone)]
pub struct CourseResultSummary {
    pub course_id: i64,
    pub course_score_id: Option<i64>,
    pub course_played_at: Option<i64>,
    pub rule_mode: RuleMode,
    pub title: String,
    pub kind: CourseKind,
    pub course_titles: [String; 10],
    pub entry_summaries: Vec<ResultSummary>,
    /// Per-entry applied arrange (seed/pattern) of this attempt, in play order.
    /// Used to retry the whole course with the same arrangement.
    pub entry_arranges: Vec<AppliedArrange>,
    pub total_ex_score: u32,
    pub max_ex_score: u32,
    pub total_notes: u32,
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

    pub fn into_result(self) -> CourseResultSummary {
        let total_notes: u32 =
            self.entry_results.iter().map(|r| r.finished.result.total_notes).sum();
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

        let course_failed =
            self.entry_results.iter().any(|r| r.finished.result.clear_type == ClearType::Failed);
        let last_result = self.entry_results.last().map(|r| &r.finished.result);
        let final_clear_type = if course_failed {
            ClearType::Failed
        } else {
            last_result.map(|r| r.clear_type).unwrap_or(ClearType::NoPlay)
        };
        let final_gauge_type = last_result.map(|r| r.gauge_type).unwrap_or(GaugeType::Normal);
        let final_gauge_value = last_result.map(|r| r.gauge_value).unwrap_or(0.0);
        let total_entries = self.definition.entries.len();
        let played_entries = self.entry_results.len();

        let bp = judge_counts.bad + judge_counts.poor + judge_counts.empty_poor;
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

        let course_clear = !course_failed && trophy_results.iter().any(|t| t.achieved);

        let entry_arranges: Vec<AppliedArrange> =
            self.entry_results.iter().map(|r| r.finished.applied_arrange.clone()).collect();
        let course_titles =
            course_titles_from_results(&self.definition.entries, &self.entry_results);
        let rule_mode =
            self.entry_results.first().map(|r| r.finished.rule_mode).unwrap_or_default();
        let entry_summaries = self.entry_results.into_iter().map(|r| r.finished.summary).collect();

        CourseResultSummary {
            course_id: self.course_id,
            course_score_id: None,
            course_played_at: None,
            rule_mode,
            title: self.definition.title,
            kind: self.definition.kind,
            course_titles,
            entry_summaries,
            entry_arranges,
            total_ex_score,
            max_ex_score,
            total_notes,
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
            current_index: 0,
            entry_results,
            queued_replays: Vec::new(),
            arrange_overrides: Vec::new(),
            auto_advance_intermediate_results: false,
        }
    }

    #[test]
    fn into_result_aggregates_scores() {
        let mut session =
            make_session(1, vec![(make_score(100, 0), 100), (make_score(100, 0), 100)]);
        session.entry_results[0].finished.course_max_combo = 100;
        session.entry_results[1].finished.course_max_combo = 200;
        let result = session.into_result();
        assert_eq!(result.total_notes, 200);
        assert_eq!(result.max_ex_score, 400);
        assert_eq!(result.total_ex_score, 400);
        assert_eq!(result.course_max_combo, 200);
        assert_eq!(result.judge_counts.pgreat, 200);
    }

    #[test]
    fn trophy_achieved_when_conditions_met() {
        // 200 notes, 10 poors = 5% miss rate, score_rate = 190/200 = 95%
        let session = make_session(1, vec![(make_score(190, 10), 200)]);
        let result = session.into_result();
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
        assert!(!result.course_clear);
    }

    /// Build a session of `entry_count` entries, but only fill `played` of them
    /// with results.  Used to simulate a course that was aborted by a Failed.
    fn make_partial_session(
        entry_count: usize,
        played: Vec<(ScoreState, u32, bmz_core::clear::ClearType)>,
    ) -> ActiveCourseSession {
        use crate::screens::result_model::ResultSummary;
        use crate::storage::play_result::StoredPlayResult;

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
            current_index: 0,
            entry_results,
            queued_replays: Vec::new(),
            arrange_overrides: Vec::new(),
            auto_advance_intermediate_results: false,
        }
    }

    #[test]
    fn failed_chart_aborts_course_and_blocks_trophy() {
        use bmz_core::clear::ClearType;
        // 4-entry course, only first 2 played, second is Failed.
        let session = make_partial_session(
            4,
            vec![
                (make_score(100, 0), 100, ClearType::Normal),
                (make_score(0, 100), 100, ClearType::Failed),
            ],
        );
        let result = session.into_result();
        assert!(result.course_failed);
        assert_eq!(result.final_clear_type, ClearType::Failed);
        assert!(!result.course_clear);
        assert_eq!(result.played_entries, 2);
        assert_eq!(result.total_entries, 4);
        // Trophies are blocked when course_failed even if numeric thresholds pass.
        assert!(!result.trophy_results[0].achieved);
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
