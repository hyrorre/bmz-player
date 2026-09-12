use super::*;

pub(super) struct EnqueueIrCourseJobRequest<'a> {
    pub(super) course_id: i64,
    pub(super) course_score_id: i64,
    pub(super) course_result: &'a crate::screens::course_session::CourseResultSummary,
    pub(super) rule_mode: bmz_gameplay::rule::RuleMode,
    pub(super) device_type: Option<bmz_core::input::InputDeviceKind>,
    pub(super) gauge: &'a str,
    pub(super) played_at: i64,
    pub(super) arrange: &'a str,
    pub(super) random_seed: Option<i64>,
}

impl WinitApp {
    /// コース定義から IR / score.db 用の identity (course_hash + charts sha256 +
    /// canonical constraints) を解決する。未解決の譜面 (sha256 不明) がある
    /// コースは score 保存 / IR 送信対象外。
    pub(super) fn course_identity_with_stored(
        &self,
        course_id: i64,
    ) -> Option<(
        crate::storage::library_db::StoredCourse,
        crate::ir::course_payload::IrCourseIdentity,
    )> {
        let stored = self
            .boot
            .library_db
            .list_courses()
            .ok()?
            .into_iter()
            .find(|course| course.id == course_id)?;
        let identity =
            crate::ir::course_payload::course_identity_from_stored(&self.boot.library_db, &stored)?;
        Some((stored, identity))
    }

    pub(super) fn course_result_ir_target(
        &self,
    ) -> Option<(String, String, Option<String>, String, String, bmz_gameplay::rule::RuleMode)>
    {
        let course = self.result.finished_course.as_ref()?;
        let course_hash = self.result.finished_course_hash.clone()?;
        let rian_course_hash_v1 = self.result.finished_course_rian_hash_v1.clone()?;
        let bms_ir_course_key = self.result.finished_course_bms_ir_key.clone();
        let gauge = course.final_gauge_type.as_str().to_string();
        let ln_policy = course.ln_policy.as_str().to_string();
        Some((
            course_hash,
            rian_course_hash_v1,
            bms_ir_course_key,
            gauge,
            ln_policy,
            course.rule_mode,
        ))
    }

    pub(super) fn start_result_ir_for_finished_play(&mut self, finished: &FinishedPlaySession) {
        if finished.stored.score_history_id <= 0 {
            return;
        }
        let chart_sha256_hex = crate::storage::common::hash_to_hex(&finished.result.chart_sha256);
        if self.result.result_ir.as_ref().is_some_and(|state| {
            state.matches_chart_result(
                finished.stored.score_history_id,
                &chart_sha256_hex,
                finished.ln_policy,
                finished.double_option,
                finished.rule_mode,
            )
        }) {
            return;
        }
        self.result.result_ir = crate::screens::result_ir::spawn_result_ir_task(
            self.boot.profile_paths.root_dir.clone(),
            self.boot.profile_paths.score_db.clone(),
            self.boot.profile_paths.network_db.clone(),
            self.boot.app_paths.logs_dir.clone(),
            &self.boot.profile_config.ir,
            finished.stored.score_history_id,
            chart_sha256_hex,
            finished.ln_policy,
            finished.double_option,
            finished.rule_mode,
        );
    }

    /// コーススコアの IR 送信ジョブを enqueue する。IR 未設定 / 定義未解決なら no-op。
    pub(super) fn enqueue_ir_course_job(&mut self, request: EnqueueIrCourseJobRequest<'_>) {
        let EnqueueIrCourseJobRequest {
            course_id,
            course_score_id,
            course_result,
            rule_mode,
            device_type,
            gauge,
            played_at,
            arrange,
            random_seed,
        } = request;
        let enabled: Vec<_> = self
            .boot
            .profile_config
            .ir
            .providers
            .iter()
            .filter(|provider| {
                provider.enabled
                    && !provider.base_url.is_empty()
                    && (!crate::ir::rian_ir::is_rian_ir_config(provider)
                        || crate::ir::rian_ir::course_submission_supported(
                            self.boot.profile_config.play.ln_mode_policy,
                            self.select.double_option,
                        ))
                    && (!crate::ir::bms_ir::is_bms_ir_config(provider)
                        || matches!(
                            rule_mode,
                            bmz_gameplay::rule::RuleMode::Beatoraja
                                | bmz_gameplay::rule::RuleMode::Lr2Oraja
                        ))
            })
            .cloned()
            .collect();
        if enabled.is_empty() {
            return;
        }
        let Some((stored, identity)) = self.course_identity_with_stored(course_id) else {
            tracing::info!(course_id, "course has unresolved charts; skipping IR submission");
            return;
        };
        if !stored.definition.release {
            tracing::info!(course_id, "course IR submission is disabled");
            return;
        }
        let definition = &identity.definition;
        let ln_policy = course_result.ln_policy.as_str().to_string();
        let payload = crate::ir::course_payload::build_course_submission(
            definition,
            course_result,
            &crate::ir::course_payload::IrCourseSubmissionContext {
                played_at,
                ln_policy: ln_policy.clone(),
                rule_mode: rule_mode.as_str().to_string(),
                gauge: gauge.to_string(),
                device_type: device_type.unwrap_or(bmz_core::input::InputDeviceKind::Keyboard),
                arrange: arrange.to_string(),
                random_seed,
                idempotency_key: format!("bmz-course-{}-{course_score_id}", identity.course_hash),
                bms_ir_course_key: identity.bms_ir_course_key.clone(),
            },
        );
        let Ok(payload_json) = serde_json::to_string(&payload) else {
            return;
        };
        let first_chart = definition
            .charts
            .first()
            .and_then(|sha| crate::storage::common::hex_to_hash::<32>(sha).ok())
            .unwrap_or([0; 32]);
        let ln_policy = course_result.ln_policy;
        for provider in enabled {
            let Some(provider_key) = crate::ir::provider_key::configured_provider_key(&provider)
            else {
                tracing::warn!(
                    provider = provider.provider,
                    "skipping IR course job because provider_key is missing; log in again"
                );
                continue;
            };
            if let Err(error) = self.boot.network_db.enqueue_ir_score_job(
                &crate::storage::network_db::NewIrScoreJob {
                    provider: provider_key.to_string(),
                    account_id: provider.account_id.clone(),
                    kind: crate::storage::network_db::IrJobKind::Course,
                    local_score_id: course_score_id,
                    chart_sha256: first_chart,
                    ln_policy,
                    payload_json: payload_json.clone(),
                    now: played_at,
                },
            ) {
                tracing::warn!(provider = provider.provider, provider_key, %error, "failed to enqueue IR course job");
            }
        }
    }

    pub(super) fn update_course_replay_slots(
        &mut self,
        course_hash: &str,
        ln_policy: crate::ln_policy::LnScorePolicy,
        rule_mode: bmz_gameplay::rule::RuleMode,
        course_score_id: i64,
        played_at: i64,
        ex_score: u32,
        bp: u32,
        max_combo: u32,
        clear_rank: u8,
    ) -> [bool; 4] {
        let slot_rules = self.boot.profile_config.replay.slot_rules;
        let candidate = crate::storage::play_result::CandidateMetrics {
            ex_score,
            bp,
            cb: bp,
            max_combo,
            clear_rank,
        };
        let mut saved_slots = [false; 4];
        for (slot_index, &rule) in slot_rules.iter().enumerate() {
            let slot = slot_index as u8;
            let prev = match self.boot.score_db.course_replay_slot(
                course_hash,
                ln_policy,
                rule_mode,
                slot,
            ) {
                Ok(record) => record,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        course_hash,
                        rule_mode = rule_mode.as_str(),
                        slot,
                        "failed to read course_replay_slot; skipping rule eval"
                    );
                    continue;
                }
            };
            let prev_metrics = prev.as_ref().map(|p| (p.ex_score, p.bp, p.max_combo, p.clear_rank));
            if !crate::storage::play_result::slot_rule_passes(rule, prev_metrics, &candidate) {
                continue;
            }
            let record = crate::storage::score_db::CourseReplaySlotRecord {
                course_hash: course_hash.to_string(),
                ln_policy,
                rule_mode,
                slot,
                rule: rule.as_str().to_string(),
                course_score_id,
                played_at,
                ex_score,
                bp,
                max_combo,
                clear_rank,
            };
            match self.boot.score_db.upsert_course_replay_slot(&record) {
                Ok(()) => saved_slots[slot_index] = true,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        course_hash,
                        slot,
                        "failed to upsert course_replay_slot"
                    );
                }
            }
        }
        saved_slots
    }
}
