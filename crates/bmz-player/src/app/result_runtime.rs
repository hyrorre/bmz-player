use std::sync::Arc;

use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::InputDeviceKind;
use bmz_core::lane::KeyMode;
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use bmz_render::scene::{CourseResultSkinSnapshot, CourseStageResultSkinSnapshot};

use crate::screens::course_session::CourseResultSummary;
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_session::AppliedArrange;
use crate::screens::result_model::{ResultFastSlowJudgeCounts, ResultSummary};
use crate::select_options::ArrangeOption;
use crate::storage::play_result::StoredPlayResult;

pub(super) fn course_result_summary_for_skin(course: &CourseResultSummary) -> ResultSummary {
    let last = course.entry_summaries.last();
    let max_combo = course.course_max_combo;
    let cb = course.entry_summaries.iter().map(|summary| summary.cb).sum();
    let fast_slow_counts =
        course.entry_summaries.iter().fold(ResultFastSlowJudgeCounts::default(), |acc, summary| {
            ResultFastSlowJudgeCounts {
                fast_pgreat: acc.fast_pgreat + summary.fast_slow_counts.fast_pgreat,
                slow_pgreat: acc.slow_pgreat + summary.fast_slow_counts.slow_pgreat,
                fast_great: acc.fast_great + summary.fast_slow_counts.fast_great,
                slow_great: acc.slow_great + summary.fast_slow_counts.slow_great,
                fast_good: acc.fast_good + summary.fast_slow_counts.fast_good,
                slow_good: acc.slow_good + summary.fast_slow_counts.slow_good,
                fast_bad: acc.fast_bad + summary.fast_slow_counts.fast_bad,
                slow_bad: acc.slow_bad + summary.fast_slow_counts.slow_bad,
                fast_poor: acc.fast_poor + summary.fast_slow_counts.fast_poor,
                slow_poor: acc.slow_poor + summary.fast_slow_counts.slow_poor,
                fast_empty_poor: acc.fast_empty_poor + summary.fast_slow_counts.fast_empty_poor,
                slow_empty_poor: acc.slow_empty_poor + summary.fast_slow_counts.slow_empty_poor,
            }
        });
    let skin_best_score = course.previous_best_score.as_ref();
    let skin_best_clear_type =
        skin_best_score.and_then(|best| ClearType::from_label(&best.clear_type));
    // A course target is the sum of the per-stage pacemaker targets.  Keeping
    // this in the aggregate ResultSummary makes the standard Result value refs
    // (121/151, 122/123, 157/158 and signed diff 153) work for course skins too.
    // If even one played stage had no target, there is no meaningful course
    // target to display.
    let target_ex_score = course
        .entry_summaries
        .iter()
        .try_fold(0_u32, |total, summary| total.checked_add(summary.target_ex_score?));

    ResultSummary {
        clear_type: course.final_clear_type,
        skin_attempt: bmz_render::snapshot::SkinAttemptState {
            // Per-chart best options do not describe an aggregate course best.
            best_score_options: None,
            ..last.map_or_else(Default::default, |summary| summary.skin_attempt)
        },
        target_name: last.map(|summary| summary.target_name.clone()).unwrap_or_default(),
        target: last.map(|summary| summary.target).unwrap_or_default(),
        arrange: "NORMAL".to_string(),
        arrange_2p: "NORMAL".to_string(),
        lane_shuffle_pattern: Vec::new(),
        ex_score: course.total_ex_score,
        max_combo,
        bp: course.bp,
        cb,
        gauge_value: course.final_gauge_value,
        gauge_type: course.final_gauge_type,
        total_notes: course.total_notes,
        duration_ms: last.map(|summary| summary.duration_ms).unwrap_or(0),
        initial_bpm: last.map(|summary| summary.initial_bpm).unwrap_or(0.0),
        min_bpm: course
            .entry_summaries
            .iter()
            .map(|summary| summary.min_bpm)
            .filter(|bpm| *bpm > 0.0)
            .reduce(f32::min)
            .unwrap_or(0.0),
        max_bpm: course
            .entry_summaries
            .iter()
            .map(|summary| summary.max_bpm)
            .filter(|bpm| *bpm > 0.0)
            .reduce(f32::max)
            .unwrap_or(0.0),
        main_bpm: last.map(|summary| summary.main_bpm).unwrap_or(0.0),
        total_gauge: last.map(|summary| summary.total_gauge).unwrap_or(0.0),
        judge_rank: last.and_then(|summary| summary.judge_rank),
        key_mode: last.map(|summary| summary.key_mode).unwrap_or_default(),
        has_long_notes: last.is_some_and(|summary| summary.has_long_notes),
        long_note_mode: last.map(|summary| summary.long_note_mode).unwrap_or_default(),
        judge_counts: course.judge_counts.clone(),
        fast_slow_counts,
        replay_path: String::new(),
        replay_slots: course.replay_slots,
        saved_replay_slots: course.saved_replay_slots,
        score_history_id: course.best_score.as_ref().map(|best| best.course_score_id).unwrap_or(0),
        best_ex_score: skin_best_score.map(|best| best.ex_score),
        best_clear_type: skin_best_clear_type,
        best_max_combo: skin_best_score.map(|best| best.max_combo),
        best_bp: skin_best_score.map(|best| best.bp),
        previous_best_ex_score: skin_best_score.map(|best| best.ex_score),
        previous_best_clear_type: skin_best_clear_type,
        previous_best_max_combo: skin_best_score.map(|best| best.max_combo),
        previous_best_bp: skin_best_score.map(|best| best.bp),
        target_ex_score,
        target_max_combo: None,
        target_bp: None,
        target_clear_type: None,
        ir_queued_jobs: course.entry_summaries.iter().map(|summary| summary.ir_queued_jobs).sum(),
        ir_last_error: course
            .entry_summaries
            .iter()
            .find_map(|summary| summary.ir_last_error.clone()),
        title: course.title.clone(),
        subtitle: String::new(),
        artist: String::new(),
        subartist: String::new(),
        genre: match course.kind {
            bmz_core::course::CourseKind::Dan => "DAN".to_string(),
            bmz_core::course::CourseKind::Course => "COURSE".to_string(),
        },
        difficulty_name: String::new(),
        play_level: String::new(),
        graph: Arc::new(aggregate_course_result_graph(&course.entry_summaries)),
    }
}

pub(super) fn course_result_skin_snapshot(
    course: &CourseResultSummary,
) -> CourseResultSkinSnapshot {
    let mut stages =
        [CourseStageResultSkinSnapshot::default(); bmz_render::skin::SKIN_BMZ_COURSE_STAGE_COUNT];
    for (slot, summary) in stages.iter_mut().zip(&course.entry_summaries) {
        let max_ex_score = summary.total_notes.saturating_mul(2);
        *slot = CourseStageResultSkinSnapshot {
            ex_score: summary.ex_score,
            gauge: summary.gauge_value,
            bp: summary.bp,
            rate_basis_points: summary
                .ex_score
                .saturating_mul(10_000)
                .checked_div(max_ex_score)
                .unwrap_or(0),
        };
    }
    CourseResultSkinSnapshot {
        stage_count: course.entry_summaries.len().min(stages.len()) as u32,
        stages,
    }
}

pub(super) fn mark_course_replay_slot_saved(
    course: &mut CourseResultSummary,
    skin_summary: Option<&mut ResultSummary>,
    slot: usize,
) {
    course.saved_replay_slots[slot] = true;
    course.replay_slots[slot] = true;
    if let Some(summary) = skin_summary {
        summary.saved_replay_slots[slot] = true;
        summary.replay_slots[slot] = true;
    }
}

fn aggregate_course_result_graph(
    entries: &[ResultSummary],
) -> bmz_render::snapshot::ResultGraphSnapshot {
    let durations: Vec<i32> =
        entries.iter().map(|entry| result_graph_duration_ms(&entry.graph)).collect();
    let total_duration = durations.iter().copied().sum::<i32>().max(1);
    let mut offset_ms = 0_i32;
    let mut graph = bmz_render::snapshot::ResultGraphSnapshot::default();

    for (entry_index, (entry, duration_ms)) in entries.iter().zip(durations).enumerate() {
        let mut section_gauge_types = std::collections::HashSet::new();
        graph.gauge_points.extend(entry.graph.gauge_points.iter().map(|point| {
            let mut point = *point;
            point.time_ms = point.time_ms.saturating_add(offset_ms);
            point.course_section_start |=
                entry_index > 0 && section_gauge_types.insert(point.gauge_type);
            point
        }));
        graph.timing_points.extend(entry.graph.timing_points.iter().map(|point| {
            bmz_render::snapshot::ResultTimingPoint {
                time_ms: point.time_ms.saturating_add(offset_ms),
                delta_us: point.delta_us,
                judge: point.judge,
            }
        }));
        graph.judge_graph_buckets.extend_from_slice(&entry.graph.judge_graph_buckets);
        graph.note_graph_buckets.extend_from_slice(&entry.graph.note_graph_buckets);
        graph.early_late_graph_buckets.extend_from_slice(&entry.graph.early_late_graph_buckets);
        graph.judge_graph_density.extend_from_slice(&entry.graph.judge_graph_density);
        graph.bpm_graph_segments.extend(entry.graph.bpm_graph_segments.iter().map(|segment| {
            let start = offset_ms as f32 + segment.start_ratio * duration_ms as f32;
            let end = offset_ms as f32 + segment.end_ratio * duration_ms as f32;
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: (start / total_duration as f32).clamp(0.0, 1.0),
                end_ratio: (end / total_duration as f32).clamp(0.0, 1.0),
                bpm: segment.bpm,
                is_stop: segment.is_stop,
            }
        }));
        if entry.graph.hit_error_ring != Default::default() {
            graph.hit_error_ring = entry.graph.hit_error_ring;
        }
        offset_ms = offset_ms.saturating_add(duration_ms);
    }

    graph.timing_distribution = bmz_render::snapshot::ResultTimingDistribution::default();
    for point in &graph.timing_points {
        graph.timing_distribution.add((point.delta_us / 1_000) as i32);
    }
    graph.refresh_timing_metrics();

    graph
}

fn result_graph_duration_ms(graph: &bmz_render::snapshot::ResultGraphSnapshot) -> i32 {
    let gauge_ms = graph.gauge_points.last().map(|point| point.time_ms).unwrap_or(0);
    let timing_ms = graph.timing_points.last().map(|point| point.time_ms).unwrap_or(0);
    let density_ms = i32::try_from(graph.judge_graph_density.len()).unwrap_or(i32::MAX / 1_000);
    let judge_ms = i32::try_from(graph.judge_graph_buckets.len())
        .unwrap_or(i32::MAX / 1_000)
        .saturating_mul(1_000);
    let early_late_ms = i32::try_from(graph.early_late_graph_buckets.len())
        .unwrap_or(i32::MAX / 1_000)
        .saturating_mul(1_000);
    let note_ms = i32::try_from(graph.note_graph_buckets.len())
        .unwrap_or(i32::MAX / 1_000)
        .saturating_mul(1_000);
    gauge_ms
        .max(timing_ms)
        .max(density_ms.saturating_mul(1_000))
        .max(judge_ms)
        .max(early_late_ms)
        .max(note_ms)
        .max(1)
}

pub(super) fn debug_boot_finished_play_session() -> FinishedPlaySession {
    let summary = debug_boot_result_summary();
    let judge_counts = JudgeCounts {
        fast_pgreat: summary.fast_slow_counts.fast_pgreat,
        slow_pgreat: summary.fast_slow_counts.slow_pgreat,
        fast_great: summary.fast_slow_counts.fast_great,
        slow_great: summary.fast_slow_counts.slow_great,
        fast_good: summary.fast_slow_counts.fast_good,
        slow_good: summary.fast_slow_counts.slow_good,
        fast_bad: summary.fast_slow_counts.fast_bad,
        slow_bad: summary.fast_slow_counts.slow_bad,
        fast_poor: summary.fast_slow_counts.fast_poor,
        slow_poor: summary.fast_slow_counts.slow_poor,
        fast_empty_poor: summary.fast_slow_counts.fast_empty_poor,
        slow_empty_poor: summary.fast_slow_counts.slow_empty_poor,
    };
    let result = PlayResult {
        chart_sha256: [0; 32],
        clear_type: summary.clear_type,
        gauge_type: summary.gauge_type,
        gauge_value: summary.gauge_value,
        total_notes: summary.total_notes,
        score: ScoreState {
            judges: judge_counts,
            combo: 0,
            max_combo: summary.max_combo,
            past_notes: summary.total_notes,
            ghost: Vec::new(),
            empty_poor_breaks_combo: false,
        },
        autoplay: false,
    };
    let course_max_combo = result.score.max_combo;
    FinishedPlaySession {
        result,
        stored: StoredPlayResult {
            score_history_id: 0,
            played_at: 0,
            replay_path: String::new(),
            replay_sha256: None,
            slot_paths: [None, None, None, None],
            device_type: InputDeviceKind::Keyboard,
        },
        summary,
        gauge_carry: Vec::new(),
        course_combo: 0,
        course_max_combo,
        replay_playback: false,
        arrange: ArrangeOption::Normal,
        applied_arrange: AppliedArrange::default(),
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        double_option: crate::select_options::DoubleOptionScoreBucket::Off,
        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
        assist: Default::default(),
        score_data_changed: false,
    }
}

pub(super) fn debug_boot_result_summary() -> ResultSummary {
    let fast_slow_counts = ResultFastSlowJudgeCounts {
        fast_pgreat: 128,
        slow_pgreat: 92,
        fast_great: 31,
        slow_great: 69,
        fast_good: 9,
        slow_good: 20,
        fast_bad: 3,
        slow_bad: 5,
        fast_poor: 2,
        slow_poor: 8,
        fast_empty_poor: 1,
        slow_empty_poor: 2,
    };
    let judge_counts = crate::screens::result_model::ResultJudgeCounts {
        pgreat: fast_slow_counts.fast_pgreat + fast_slow_counts.slow_pgreat,
        great: fast_slow_counts.fast_great + fast_slow_counts.slow_great,
        good: fast_slow_counts.fast_good + fast_slow_counts.slow_good,
        bad: fast_slow_counts.fast_bad + fast_slow_counts.slow_bad,
        poor: fast_slow_counts.fast_poor + fast_slow_counts.slow_poor,
        empty_poor: fast_slow_counts.fast_empty_poor + fast_slow_counts.slow_empty_poor,
    };
    let total_notes = 594;
    let duration_ms = 180_000;
    ResultSummary {
        clear_type: ClearType::Failed,
        skin_attempt: bmz_render::snapshot::SkinAttemptState {
            source_key_mode: Some(KeyMode::K7),
            effective_key_mode: Some(KeyMode::K7),
            source_ln_profile_bits: Some(bmz_render::snapshot::SKIN_SOURCE_LN_DEFINED_CN_BIT),
            session_mode_index: Some(0),
            ln_mode_index: Some(1),
            has_bga: Some(true),
            has_random_sequence: Some(false),
            ..Default::default()
        },
        target_name: "RANK AAA".to_string(),
        target: crate::select_options::TargetOption::RankAaa,
        arrange: "RANDOM".to_string(),
        arrange_2p: "NORMAL".to_string(),
        lane_shuffle_pattern: vec![3, 1, 4, 2, 7, 5, 6],
        ex_score: judge_counts.pgreat * 2 + judge_counts.great,
        max_combo: 239,
        bp: 30,
        cb: 345,
        gauge_value: 39.4,
        gauge_type: GaugeType::Normal,
        total_notes,
        duration_ms,
        initial_bpm: 171.0,
        min_bpm: 128.0,
        max_bpm: 192.0,
        main_bpm: 171.0,
        total_gauge: 363.0,
        judge_rank: Some(2),
        key_mode: KeyMode::K7,
        has_long_notes: true,
        long_note_mode: bmz_chart::model::LongNoteMode::Cn,
        judge_counts,
        fast_slow_counts,
        replay_path: String::new(),
        replay_slots: [true, false, true, false],
        saved_replay_slots: [false, false, false, false],
        score_history_id: 0,
        best_ex_score: Some(780),
        best_clear_type: Some(ClearType::Easy),
        best_max_combo: Some(412),
        best_bp: Some(24),
        previous_best_ex_score: Some(760),
        previous_best_clear_type: Some(ClearType::Normal),
        previous_best_max_combo: Some(390),
        previous_best_bp: Some(36),
        target_ex_score: Some(1_056),
        target_max_combo: Some(594),
        target_bp: Some(10),
        target_clear_type: Some(ClearType::Hard),
        ir_queued_jobs: 0,
        ir_last_error: None,
        title: "Debug Result Boot [ANOTHER]".to_string(),
        subtitle: "synthetic result".to_string(),
        artist: "bmz-player".to_string(),
        subartist: "Codex".to_string(),
        genre: "DEBUG".to_string(),
        difficulty_name: "ANOTHER".to_string(),
        play_level: "12".to_string(),
        graph: Arc::new(debug_boot_result_graph(duration_ms)),
    }
}

fn debug_boot_result_graph(duration_ms: i32) -> bmz_render::snapshot::ResultGraphSnapshot {
    let mut graph = bmz_render::snapshot::ResultGraphSnapshot {
        gauge_points: (0..=18)
            .map(|index| bmz_render::snapshot::ResultGaugeGraphPoint {
                time_ms: index * 10_000,
                value: (100.0 - index as f32 * 3.2).max(12.0),
                max: 100.0,
                border: 20.0,
                gauge_type: GaugeType::Normal as i32,
                course_section_start: false,
            })
            .collect(),
        judge_graph_buckets: (0..360)
            .map(|index| bmz_render::snapshot::ResultJudgeGraphBucket {
                values: [
                    0,
                    1 + (index % 5) as u32,
                    (index % 4) as u32,
                    (index % 3) as u32,
                    (index % 2) as u32,
                    ((index + 1) % 2) as u32,
                ],
            })
            .collect(),
        early_late_graph_buckets: (0..360)
            .map(|index| bmz_render::snapshot::ResultEarlyLateGraphBucket {
                values: [
                    0,
                    1 + (index % 5) as u32,
                    (index % 4) as u32,
                    ((index + 2) % 3) as u32,
                    (index % 2) as u32,
                    0,
                    ((index + 1) % 5) as u32,
                    ((index + 3) % 4) as u32,
                    ((index + 1) % 3) as u32,
                    ((index + 1) % 2) as u32,
                ],
            })
            .collect(),
        bpm_graph_segments: vec![
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.0,
                end_ratio: 0.35,
                bpm: 171.0,
                is_stop: false,
            },
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.35,
                end_ratio: 0.55,
                bpm: 128.0,
                is_stop: false,
            },
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.55,
                end_ratio: 0.56,
                bpm: 0.0,
                is_stop: true,
            },
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.56,
                end_ratio: 1.0,
                bpm: 192.0,
                is_stop: false,
            },
        ],
        ..Default::default()
    };
    graph.judge_graph_density =
        graph.judge_graph_buckets.iter().map(|bucket| bucket.total().min(255) as u8).collect();
    graph.timing_points = (-60..=60)
        .map(|index| {
            let delta_ms: i32 = if index % 7 == 0 { index / 2 } else { index / 4 };
            let judge = if delta_ms.abs() <= 8 {
                bmz_core::judge::Judge::PGreat
            } else if delta_ms.abs() <= 24 {
                bmz_core::judge::Judge::Great
            } else {
                bmz_core::judge::Judge::Good
            };
            bmz_render::snapshot::ResultTimingPoint {
                time_ms: ((index + 60) * duration_ms / 120).clamp(0, duration_ms),
                delta_us: i64::from(delta_ms) * 1_000,
                judge,
            }
        })
        .collect();
    graph.timing_distribution = bmz_render::snapshot::ResultTimingDistribution::default();
    for point in &graph.timing_points {
        graph.timing_distribution.add((point.delta_us / 1_000) as i32);
    }
    graph.refresh_timing_metrics();
    graph
}

pub(super) fn result_min_bpm(summary: &ResultSummary) -> f32 {
    summary
        .graph
        .bpm_graph_segments
        .iter()
        .filter(|segment| !segment.is_stop && segment.bpm > 0.0)
        .map(|segment| segment.bpm)
        .reduce(f32::min)
        .unwrap_or(summary.min_bpm)
}

pub(super) fn result_max_bpm(summary: &ResultSummary) -> f32 {
    summary
        .graph
        .bpm_graph_segments
        .iter()
        .filter(|segment| !segment.is_stop && segment.bpm > 0.0)
        .map(|segment| segment.bpm)
        .reduce(f32::max)
        .unwrap_or(summary.max_bpm)
}

pub(super) fn result_main_bpm(summary: &ResultSummary) -> f32 {
    summary
        .graph
        .bpm_graph_segments
        .iter()
        .filter(|segment| !segment.is_stop && segment.bpm > 0.0)
        .max_by(|a, b| {
            let a_width = a.end_ratio - a.start_ratio;
            let b_width = b.end_ratio - b.start_ratio;
            a_width.total_cmp(&b_width)
        })
        .map(|segment| segment.bpm)
        .unwrap_or(summary.main_bpm)
}
