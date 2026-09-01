use super::course_flow_ir::EnqueueIrCourseJobRequest;
use super::*;

fn should_prepare_terminal_course_finish(
    current_chart_matches: bool,
    failed: bool,
    next_chart_exists: bool,
) -> bool {
    current_chart_matches && (failed || !next_chart_exists)
}

fn prepared_course_finish_matches(
    prepared_course_id: i64,
    prepared_entries: usize,
    active_course_id: i64,
    active_entries: usize,
) -> bool {
    prepared_course_id == active_course_id && prepared_entries == active_entries
}

fn course_result_persistence_enabled(
    any_autoplay: bool,
    any_replay_playback: bool,
    score_save_disabled: bool,
) -> bool {
    !any_autoplay && !any_replay_playback && !score_save_disabled
}

impl WinitApp {
    /// 最終ステージの単曲結果が確定した時点で、コース全体の保存と IR enqueue を行う。
    ///
    /// 表示用の `active_course` はまだ消費しない。Play の終了演出と最終曲の単曲
    /// Result を従来どおり表示し、後から `finish_active_course` がこの準備済み結果を
    /// 一度だけ昇格させる。
    pub(super) fn prepare_terminal_course_finish(
        &mut self,
        chart_id: i64,
        finished: &FinishedPlaySession,
    ) {
        if self.result.prepared_course_finish.is_some() {
            return;
        }
        let should_prepare = self.play.active_course.as_ref().is_some_and(|course| {
            let current_chart_matches =
                course.current_entry().and_then(|entry| entry.chart_id) == Some(chart_id);
            let failed = finished.result.clear_type == bmz_core::clear::ClearType::Failed;
            let next_chart_id = course
                .definition
                .entries
                .get(course.current_index.saturating_add(1))
                .and_then(|entry| entry.chart_id);
            should_prepare_terminal_course_finish(
                current_chart_matches,
                failed,
                next_chart_id.is_some(),
            )
        });
        if !should_prepare {
            return;
        }

        // 通常はプレイ中に完了済み。極端に短い譜面や早期 Failed でも、保存する
        // denominator と LN policy はバックグラウンド集計の確定値を使う。
        self.await_course_metrics();
        let Some(course) = self.play.active_course.as_ref() else {
            return;
        };
        if course.current_entry().and_then(|entry| entry.chart_id) != Some(chart_id) {
            tracing::warn!(chart_id, "course finish preparation ignored after stage changed");
            return;
        }
        let mut completed_course = course.clone();
        completed_course
            .entry_results
            .push(CourseEntryResult { chart_id, finished: finished.clone() });
        completed_course.current_index = completed_course.current_index.saturating_add(1);

        let prepared = self.prepare_course_finish(completed_course);
        tracing::info!(
            course_id = prepared.course_id,
            course_score_id = ?prepared.course_result.course_score_id,
            "course score finalized at last judgement"
        );
        self.result.prepared_course_finish = Some(prepared);
    }

    pub(super) fn finish_active_course(&mut self) {
        if self.play.pending_course_stage_launch.is_some() {
            self.invalidate_play_preload();
        }
        let prepared_matches_active = self
            .result
            .prepared_course_finish
            .as_ref()
            .zip(self.play.active_course.as_ref())
            .is_some_and(|(prepared, course)| {
                prepared_course_finish_matches(
                    prepared.course_id,
                    prepared.course_result.played_entries,
                    course.course_id,
                    course.entry_results.len(),
                )
            });
        if !prepared_matches_active {
            self.await_course_metrics();
        }
        let Some(course) = self.play.active_course.take() else {
            return;
        };
        let prepared = match self.result.prepared_course_finish.take() {
            Some(prepared)
                if prepared_course_finish_matches(
                    prepared.course_id,
                    prepared.course_result.played_entries,
                    course.course_id,
                    course.entry_results.len(),
                ) =>
            {
                prepared
            }
            Some(stale) => {
                tracing::warn!(
                    prepared_course_id = stale.course_id,
                    active_course_id = course.course_id,
                    prepared_entries = stale.course_result.played_entries,
                    active_entries = course.entry_results.len(),
                    "discarding stale prepared course finish"
                );
                self.prepare_course_finish(course)
            }
            None => self.prepare_course_finish(course),
        };
        self.present_prepared_course_finish(prepared);
    }

    fn prepare_course_finish(&mut self, course: ActiveCourseSession) -> PreparedCourseFinish {
        let course_id = course.course_id;
        let course_identity = self.course_identity_with_stored(course_id);
        let any_autoplay = course.entry_results.iter().any(|entry| entry.finished.result.autoplay);
        let any_replay_playback =
            course.entry_results.iter().any(|entry| entry.finished.replay_playback);
        let any_assist =
            course.entry_results.iter().any(|entry| !entry.finished.assist.score_update_enabled());
        let score_save_disabled = course.score_save_disabled;

        // `into_result` が entry_results を消費する前に保存用の情報を取り出す。
        let chart_records: Vec<crate::storage::score_db::CourseScoreChartRecord> = course
            .entry_results
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let chart_sha256 = course_identity.as_ref()?.1.chart_sha256s.get(index).copied()?;
                Some(crate::storage::score_db::CourseScoreChartRecord {
                    position: index as i64,
                    chart_sha256,
                    ex_score: if any_assist { 0 } else { entry.finished.result.score.ex_score() },
                    max_combo: if any_assist { 0 } else { entry.finished.result.score.max_combo },
                    clear_type: entry.finished.result.clear_type.as_str().to_string(),
                    gauge_value: entry.finished.result.gauge_value,
                })
            })
            .collect();
        let replay_records: Vec<crate::storage::score_db::CourseReplayRecord> = course
            .entry_results
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                if any_assist {
                    return None;
                }
                let chart_sha256 = course_identity.as_ref()?.1.chart_sha256s.get(index).copied()?;
                Some(crate::storage::score_db::CourseReplayRecord {
                    position: index as i64,
                    chart_sha256,
                    replay_path: entry.finished.stored.replay_path.clone(),
                })
            })
            .collect();
        let history_ids: Vec<i64> = course
            .entry_results
            .iter()
            .map(|entry| entry.finished.stored.score_history_id)
            .filter(|id| *id > 0)
            .collect();
        let last_finished = course.entry_results.last().map(|entry| entry.finished.clone());
        let max_combo: u32 = course
            .entry_results
            .iter()
            .map(|entry| entry.finished.course_max_combo)
            .max()
            .unwrap_or(0);
        let course_arrange = course
            .entry_results
            .first()
            .map(|entry| entry.finished.arrange.to_persistent_str().to_string())
            .unwrap_or_else(|| "Normal".to_string());

        let mut course_result = course.into_result();
        tracing::info!(
            title = %course_result.title,
            total_ex_score = course_result.total_ex_score,
            course_clear = course_result.course_clear,
            course_failed = course_result.course_failed,
            played = course_result.played_entries,
            total = course_result.total_entries,
            trophies = ?course_result
                .trophy_results
                .iter()
                .filter(|trophy| trophy.achieved)
                .map(|trophy| trophy.name.as_str())
                .collect::<Vec<_>>(),
            "course finished"
        );

        // Autoplay / replay playback は単曲と同じく保存しない。アシスト時は
        // クリアランプと回数だけを残し、数値・リプレイ・IR・トロフィーは更新しない。
        if course_result_persistence_enabled(any_autoplay, any_replay_playback, score_save_disabled)
        {
            if let Some((stored_course, identity)) = &course_identity {
                let course_ln_policy = course_result.ln_policy;
                let course_rule_mode = course_result.rule_mode;
                course_result.previous_best_score = self
                    .boot
                    .score_db
                    .best_course_score(&identity.course_hash, course_ln_policy, course_rule_mode)
                    .unwrap_or_else(|error| {
                        tracing::warn!(
                            %error,
                            course_id,
                            course_hash = %identity.course_hash,
                            rule_mode = course_rule_mode.as_str(),
                            "failed to read previous best course score"
                        );
                        None
                    });
                let final_clear_type = course_result.final_clear_type;
                let played_at = last_finished
                    .as_ref()
                    .map(|finished| finished.stored.played_at)
                    .unwrap_or_else(now_unix_seconds);
                let achieved_trophies: Vec<String> = if any_assist {
                    Vec::new()
                } else {
                    course_result
                        .trophy_results
                        .iter()
                        .filter(|trophy| trophy.achieved)
                        .map(|trophy| trophy.name.clone())
                        .collect()
                };
                let trophies_json =
                    serde_json::to_string(&achieved_trophies).unwrap_or_else(|_| "[]".to_string());
                let insert = crate::storage::score_db::CourseScoreInsert {
                    course_hash: identity.course_hash.clone(),
                    ln_policy: course_ln_policy,
                    rule_mode: course_rule_mode,
                    source: stored_course.source.clone(),
                    course_key: stored_course.definition.key.clone(),
                    title: stored_course.definition.title.clone(),
                    kind: identity.definition.kind.clone(),
                    constraints_json: identity.constraints_json.clone(),
                    chart_sha256s_json: identity.chart_sha256s_json.clone(),
                    ex_score: if any_assist { 0 } else { course_result.total_ex_score },
                    max_ex_score: course_result.max_ex_score,
                    clear_type: final_clear_type.as_str().to_string(),
                    gauge_type: course_result.final_gauge_type.as_str().to_string(),
                    gauge_value: course_result.final_gauge_value,
                    max_combo: if any_assist { 0 } else { max_combo },
                    bp: if any_assist { 0 } else { course_result.bp },
                    course_failed: course_result.course_failed,
                    course_clear: !any_assist && course_result.course_clear,
                    arrange: course_arrange,
                    trophies_json,
                    played_at,
                    charts: chart_records,
                    replays: replay_records,
                    achieved_trophies,
                };
                match self.boot.score_db.insert_course_score(&insert) {
                    Ok(course_score_id) => {
                        if !any_assist
                            && let Err(error) = self
                                .boot
                                .score_db
                                .tag_score_history_with_course(&history_ids, course_score_id)
                        {
                            tracing::warn!(
                                %error,
                                course_id,
                                course_score_id,
                                "failed to tag score_history rows with course_score_id"
                            );
                        }

                        if !any_assist {
                            self.enqueue_ir_course_job(EnqueueIrCourseJobRequest {
                                course_id,
                                course_score_id,
                                course_result: &course_result,
                                rule_mode: course_rule_mode,
                                device_type: last_finished
                                    .as_ref()
                                    .map(|finished| finished.stored.device_type),
                                gauge: &insert.gauge_type,
                                played_at,
                                arrange: &insert.arrange,
                                random_seed: course_result
                                    .entry_arranges
                                    .first()
                                    .and_then(|arrange| arrange.packed_beatoraja_seed_from_sides()),
                            });
                        }

                        course_result.course_score_id = Some(course_score_id);
                        course_result.course_played_at = Some(played_at);
                        let replay_complete = !any_assist
                            && self
                                .boot
                                .score_db
                                .course_replay_attempt_is_complete(course_score_id)
                                .unwrap_or_else(|error| {
                                    tracing::warn!(
                                        %error,
                                        course_id,
                                        course_score_id,
                                        "failed to validate saved course replay"
                                    );
                                    false
                                });
                        if !any_assist && !replay_complete {
                            tracing::warn!(
                                course_id,
                                course_score_id,
                                "course replay is incomplete; replay slots were not updated"
                            );
                        }
                        if replay_complete {
                            course_result.saved_replay_slots = self.update_course_replay_slots(
                                &identity.course_hash,
                                course_ln_policy,
                                course_rule_mode,
                                course_score_id,
                                played_at,
                                course_result.total_ex_score,
                                course_result.bp,
                                max_combo,
                                final_clear_type as u8,
                            );
                        }
                        course_result.replay_slots = self
                            .boot
                            .score_db
                            .course_replay_slot_presence(
                                &identity.course_hash,
                                course_ln_policy,
                                course_rule_mode,
                            )
                            .unwrap_or_else(|error| {
                                tracing::warn!(
                                    %error,
                                    course_id,
                                    course_hash = %identity.course_hash,
                                    rule_mode = course_rule_mode.as_str(),
                                    "failed to read course replay slot presence"
                                );
                                [false; 4]
                            });
                        for (index, saved) in course_result.saved_replay_slots.iter().enumerate() {
                            if *saved {
                                course_result.replay_slots[index] = true;
                            }
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, course_id, "failed to persist course score");
                    }
                }

                course_result.best_score = self
                    .boot
                    .score_db
                    .best_course_score(&identity.course_hash, course_ln_policy, course_rule_mode)
                    .unwrap_or_else(|error| {
                        tracing::warn!(
                            %error,
                            course_id,
                            course_hash = %identity.course_hash,
                            rule_mode = course_rule_mode.as_str(),
                            "failed to read best course score"
                        );
                        None
                    });
            } else {
                tracing::warn!(
                    course_id,
                    "course identity unavailable; skipping course score save"
                );
            }
        }

        PreparedCourseFinish {
            course_id,
            course_result,
            course_hash: course_identity.as_ref().map(|(_, identity)| identity.course_hash.clone()),
            rian_course_hash_v1: course_identity
                .as_ref()
                .map(|(_, identity)| identity.rian_course_hash_v1.clone()),
            bms_ir_course_key: course_identity
                .as_ref()
                .and_then(|(_, identity)| identity.bms_ir_course_key.clone()),
            last_finished,
        }
    }

    fn present_prepared_course_finish(&mut self, prepared: PreparedCourseFinish) {
        if prepared.course_result.saved_replay_slots.iter().any(|saved| *saved) {
            self.notify_obs_save_recording(crate::obs::ObsRecordingSaveReason::OnReplay);
        }
        self.install_finished_course(
            prepared.course_result,
            prepared.course_hash,
            prepared.rian_course_hash_v1,
            prepared.bms_ir_course_key,
        );
        if let Some(last) = prepared.last_finished {
            self.result.result_gauge_graph_type = last.summary.gauge_type as i32;
            self.result.finished_play = Some(last);
            self.result.result_key5_held = false;
            self.result.result_key7_held = false;
            self.result.result_scene_started_at = Instant::now();
            self.ensure_result_skin_ready_for_entry(ResultSkinSlot::Course);
        }
        let clear_type = self
            .result
            .finished_course
            .as_ref()
            .map(|course| course.final_clear_type)
            .unwrap_or(bmz_core::clear::ClearType::Failed);
        self.play_course_result_entry_sound(clear_type);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        course_result_persistence_enabled, prepared_course_finish_matches,
        should_prepare_terminal_course_finish,
    };

    #[test]
    fn course_finish_is_prepared_only_for_failed_or_last_stage() {
        assert!(!should_prepare_terminal_course_finish(true, false, true));
        assert!(should_prepare_terminal_course_finish(true, false, false));
        assert!(should_prepare_terminal_course_finish(true, true, true));
        assert!(!should_prepare_terminal_course_finish(false, true, false));
    }

    #[test]
    fn prepared_course_finish_requires_same_attempt_shape() {
        assert!(prepared_course_finish_matches(7, 3, 7, 3));
        assert!(!prepared_course_finish_matches(8, 3, 7, 3));
        assert!(!prepared_course_finish_matches(7, 2, 7, 3));
    }

    #[test]
    fn converted_course_never_persists() {
        assert!(course_result_persistence_enabled(false, false, false));
        assert!(!course_result_persistence_enabled(false, false, true));
        assert!(!course_result_persistence_enabled(true, false, false));
        assert!(!course_result_persistence_enabled(false, true, false));
    }
}
