use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind};

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
}

pub struct CourseEntryResult {
    pub chart_id: i64,
    pub finished: FinishedPlaySession,
}

#[derive(Debug, Clone)]
pub struct CourseResultSummary {
    pub course_id: i64,
    pub title: String,
    pub kind: CourseKind,
    pub entry_summaries: Vec<ResultSummary>,
    /// Per-entry applied arrange (seed/pattern) of this attempt, in play order.
    /// Used to retry the whole course with the same arrangement.
    pub entry_arranges: Vec<AppliedArrange>,
    pub total_ex_score: u32,
    pub max_ex_score: u32,
    pub total_notes: u32,
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
    pub best_score: Option<crate::storage::library_db::CourseBestScore>,
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

        let course_failed = self
            .entry_results
            .iter()
            .any(|r| r.finished.result.clear_type == bmz_core::clear::ClearType::Failed);
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
        let entry_summaries = self.entry_results.into_iter().map(|r| r.finished.summary).collect();

        CourseResultSummary {
            course_id: self.course_id,
            title: self.definition.title,
            kind: self.definition.kind,
            entry_summaries,
            entry_arranges,
            total_ex_score,
            max_ex_score,
            total_notes,
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
        }
    }
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
                CourseEntryResult {
                    chart_id: i as i64 + 1,
                    finished: FinishedPlaySession {
                        result,
                        stored: StoredPlayResult {
                            score_history_id: 0,
                            replay_path: String::new(),
                            slot_paths: [None, None, None, None],
                            device_type: bmz_core::input::InputDeviceKind::Keyboard,
                        },
                        summary: ResultSummary::from_play_result(
                            &make_play_result(ScoreState::default(), total_notes),
                            &StoredPlayResult {
                                score_history_id: 0,
                                replay_path: String::new(),
                                slot_paths: [None, None, None, None],
                                device_type: bmz_core::input::InputDeviceKind::Keyboard,
                            },
                            &make_result_chart(total_notes),
                        ),
                        replay_playback: false,
                        arrange: crate::select_options::ArrangeOption::Normal,
                        applied_arrange: crate::screens::play_session::AppliedArrange::default(),
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
        }
    }

    #[test]
    fn into_result_aggregates_scores() {
        let session = make_session(1, vec![(make_score(100, 0), 100), (make_score(100, 0), 100)]);
        let result = session.into_result();
        assert_eq!(result.total_notes, 200);
        assert_eq!(result.max_ex_score, 400);
        assert_eq!(result.total_ex_score, 400);
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
                CourseEntryResult {
                    chart_id: i as i64 + 1,
                    finished: FinishedPlaySession {
                        result,
                        stored: StoredPlayResult {
                            score_history_id: 0,
                            replay_path: String::new(),
                            slot_paths: [None, None, None, None],
                            device_type: bmz_core::input::InputDeviceKind::Keyboard,
                        },
                        summary: ResultSummary::from_play_result(
                            &make_play_result(ScoreState::default(), total_notes),
                            &StoredPlayResult {
                                score_history_id: 0,
                                replay_path: String::new(),
                                slot_paths: [None, None, None, None],
                                device_type: bmz_core::input::InputDeviceKind::Keyboard,
                            },
                            &make_result_chart(total_notes),
                        ),
                        replay_playback: false,
                        arrange: crate::select_options::ArrangeOption::Normal,
                        applied_arrange: crate::screens::play_session::AppliedArrange::default(),
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
        assert!(!result.course_clear);
        assert_eq!(result.played_entries, 2);
        assert_eq!(result.total_entries, 4);
        // Trophies are blocked when course_failed even if numeric thresholds pass.
        assert!(!result.trophy_results[0].achieved);
    }
}
