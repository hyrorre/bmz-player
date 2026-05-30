use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind};

use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::result_model::{ResultJudgeCounts, ResultSummary};

pub struct ActiveCourseSession {
    pub course_id: i64,
    pub definition: CourseDefinition,
    pub current_index: usize,
    pub entry_results: Vec<CourseEntryResult>,
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
    pub total_ex_score: u32,
    pub max_ex_score: u32,
    pub total_notes: u32,
    pub judge_counts: ResultJudgeCounts,
    pub trophy_results: Vec<TrophyResult>,
    pub course_clear: bool,
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

        let judge_counts = self.entry_results.iter().fold(
            ResultJudgeCounts::default(),
            |acc, r| {
                let j = &r.finished.result.score.judges;
                ResultJudgeCounts {
                    pgreat: acc.pgreat + j.fast_pgreat + j.slow_pgreat,
                    great: acc.great + j.fast_great + j.slow_great,
                    good: acc.good + j.fast_good + j.slow_good,
                    bad: acc.bad + j.fast_bad + j.slow_bad,
                    poor: acc.poor + j.fast_poor + j.slow_poor,
                    empty_poor: acc.empty_poor + j.fast_empty_poor + j.slow_empty_poor,
                }
            },
        );

        let miss_count = judge_counts.bad + judge_counts.poor + judge_counts.empty_poor;
        let miss_rate =
            if total_notes > 0 { miss_count as f32 / total_notes as f32 * 100.0 } else { 0.0 };
        let score_rate =
            if max_ex_score > 0 { total_ex_score as f32 / max_ex_score as f32 * 100.0 } else { 0.0 };

        let trophy_results: Vec<TrophyResult> = self
            .definition
            .trophies
            .iter()
            .map(|trophy| TrophyResult {
                name: trophy.name.clone(),
                achieved: miss_rate <= trophy.max_miss_rate && score_rate >= trophy.min_score_rate,
            })
            .collect();

        let course_clear = trophy_results.iter().any(|t| t.achieved);

        let entry_summaries =
            self.entry_results.into_iter().map(|r| r.finished.summary).collect();

        CourseResultSummary {
            course_id: self.course_id,
            title: self.definition.title,
            kind: self.definition.kind,
            entry_summaries,
            total_ex_score,
            max_ex_score,
            total_notes,
            judge_counts,
            trophy_results,
            course_clear,
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
        use bmz_chart::hash::compute_chart_identity;
        use bmz_core::clear::{ClearType, GaugeType};
        PlayResult {
            chart_sha256: compute_chart_identity(b"test").file_sha256,
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 80.0,
            total_notes,
            score,
            autoplay: false,
        }
    }

    fn make_session(course_id: i64, entry_scores: Vec<(ScoreState, u32)>) -> ActiveCourseSession {
        use bmz_core::course::CourseKind;
        use crate::storage::play_result::StoredPlayResult;
        use crate::screens::result_model::ResultSummary;

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
                        },
                        summary: ResultSummary::from_play_result(
                            &make_play_result(
                                ScoreState::default(),
                                total_notes,
                            ),
                            &StoredPlayResult {
                                score_history_id: 0,
                                replay_path: String::new(),
                                slot_paths: [None, None, None, None],
                            },
                            &bmz_chart::model::ChartMetadata::default(),
                        ),
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
        }
    }

    #[test]
    fn into_result_aggregates_scores() {
        let session = make_session(
            1,
            vec![
                (make_score(100, 0), 100),
                (make_score(100, 0), 100),
            ],
        );
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
}
