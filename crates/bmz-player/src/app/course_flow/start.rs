use super::*;

impl WinitApp {
    /// Returns the beatoraja-compatible course stage marker for the currently
    /// playing chart in the active course (1, 2, 3, 4 or Final).  None when no
    /// course is active.
    ///
    /// The final entry always maps to `Final` (OPTION_COURSE_STAGE_FINAL=289);
    /// earlier entries map to Stage1..4 by their 1-based index, clamped to
    /// Stage4 for courses longer than 4 + final entry.
    pub(super) fn current_course_stage_marker(&self) -> Option<CourseStageMarker> {
        let course = self.play.active_course.as_ref()?;
        let total = course.definition.entries.len();
        if total == 0 {
            return None;
        }
        let index = course.current_index.min(total - 1);
        let is_final = index + 1 == total;
        if is_final {
            return Some(CourseStageMarker::Final);
        }
        Some(match index {
            0 => CourseStageMarker::Stage1,
            1 => CourseStageMarker::Stage2,
            2 => CourseStageMarker::Stage3,
            _ => CourseStageMarker::Stage4,
        })
    }

    pub(super) fn current_course_titles(&self) -> [String; 10] {
        let Some(course) = self.play.active_course.as_ref() else {
            return Default::default();
        };
        course_titles_from_entries(
            course
                .definition
                .entries
                .iter()
                .map(|entry| (entry.title_hint.as_str(), entry.chart_id.is_some())),
        )
    }

    pub(super) fn apply_course_skin_context(&self, snapshot: &mut RenderSnapshot) {
        snapshot.course_stage = self.current_course_stage_marker();
        snapshot.course_titles = self.current_course_titles();
    }

    pub(super) fn start_course(&mut self, course_id: i64) {
        self.select.autoplay_folder = None;
        self.start_course_with_arrange(course_id, Vec::new(), false);
    }

    /// Start a course in PLAY mode.  When `arrange_overrides` is non-empty, the
    /// recorded per-entry arrange (seed/pattern) is reapplied so the whole
    /// course replays with the same arrangement; entries without an override at
    /// their index get a fresh arrange.  A fresh course start passes an empty
    /// vec.
    pub(super) fn start_course_with_arrange(
        &mut self,
        course_id: i64,
        arrange_overrides: Vec<AppliedArrange>,
        auto_advance_intermediate_results: bool,
    ) {
        let started_at = Instant::now();
        let course_load_started_at = Instant::now();
        match self.boot.library_db.repair_course_entry_chart_links_for_course(course_id) {
            Ok(repaired) if repaired != 0 => {
                tracing::info!(course_id, repaired, "repaired course links before course start");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, course_id, "failed to repair course links before course start");
            }
        }
        let stored = match self.boot.library_db.course_by_id(course_id) {
            Ok(course) => course,
            Err(error) => {
                tracing::error!(%error, course_id, "failed to load course for start_course");
                return;
            }
        };
        let Some(stored) = stored else {
            tracing::warn!(course_id, "course not found");
            return;
        };
        let course_load_elapsed = course_load_started_at.elapsed();
        let mut definition = stored.definition;
        if definition.entries.is_empty()
            || definition.entries.iter().any(|entry| entry.chart_id.is_none())
        {
            let resolved =
                definition.entries.iter().filter(|entry| entry.chart_id.is_some()).count();
            tracing::warn!(
                course_id,
                resolved,
                total = definition.entries.len(),
                "course is missing entries"
            );
            return;
        }
        if let Err(error) = resolve_course_chart_sources(&self.boot.library_db, &mut definition) {
            tracing::warn!(%error, course_id, "failed to resolve course chart sources");
            return;
        }
        let first_chart_id = definition.entries.first().and_then(|e| e.chart_id);
        let Some(first_chart_id) = first_chart_id else {
            tracing::warn!(course_id, "no resolved chart in course");
            return;
        };
        tracing::info!(
            course_id,
            title = %definition.title,
            same_arrange = !arrange_overrides.is_empty(),
            "starting course"
        );
        let mut entry_start_options = Vec::with_capacity(definition.entries.len());
        for index in 0..definition.entries.len() {
            let mut options = self.play_start_options();
            normalize_session_mode_for_course(&mut options);
            apply_course_constraints(&mut options, &definition.constraints);
            // Reapply each chart's recorded arrange after constraints so the
            // constraint clamp doesn't overwrite it (same ordering as replay).
            if let Some(arrange) = arrange_overrides.get(index) {
                apply_arrange_override(&mut options, arrange);
            }
            entry_start_options.push(options);
        }
        let ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        let rule_mode = self.boot.profile_config.play.rule_mode;
        let metadata_started_at = Instant::now();
        let library_snapshot = match course_play_metrics_from_library_metadata(
            &self.boot.library_db,
            &definition,
            ln_policy_setting,
            &entry_start_options,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::error!(%error, course_id, "failed to load course chart metadata");
                return;
            }
        };
        let metadata_elapsed = metadata_started_at.elapsed();
        let score_save_disabled = library_snapshot.has_score_disabling_key_mode_conversion
            || self.boot.profile_config.cli_auto_scratch();
        if score_save_disabled {
            for options in &mut entry_start_options {
                options.score_save_disabled = true;
            }
        }
        let options = entry_start_options[0].clone();
        apply_course_entry_title_hints(&mut definition, &library_snapshot.titles);
        let course_title = definition.title.clone();
        let course_metrics = library_snapshot.metrics;
        let first_chart = library_snapshot.first_chart;
        self.play.active_course = Some(ActiveCourseSession {
            course_id,
            definition,
            ln_policy_setting,
            ln_policy: course_metrics.ln_policy,
            rule_mode,
            score_save_disabled,
            course_total_notes: course_metrics.total_notes,
            course_ln_mode: course_metrics.ln_mode,
            current_index: 0,
            entry_results: Vec::new(),
            entry_start_options,
            replay_stage_limit: None,
            auto_advance_intermediate_results,
        });
        self.begin_course_decide_for_chart(first_chart_id, options, &course_title, first_chart);
        self.begin_course_metrics_background(course_id, first_chart_id, started_at, true);
        tracing::info!(
            course_id,
            entries = self
                .play
                .active_course
                .as_ref()
                .map(|course| course.definition.entries.len())
                .unwrap_or_default(),
            course_load_elapsed_ms = course_load_elapsed.as_millis(),
            metadata_elapsed_ms = metadata_elapsed.as_millis(),
            metadata_total_notes = course_metrics.total_notes,
            select_to_decide_elapsed_ms = started_at.elapsed().as_millis(),
            "course decide transition scheduled"
        );
    }

    /// Start a course in replay mode, replaying the saved per-chart inputs of
    /// the given `course_score_id`.  Each chart of the course is launched in
    /// sequence with its saved ReplayPlayer attached, so the user can watch
    /// the entire course attempt back to back.
    ///
    /// If `course_score_id` refers to a partial course attempt (e.g. failed
    /// at chart 2 of 4), only the played charts replay; the queue ends there
    /// and the course session naturally finishes the same way the original
    /// attempt did.
    ///
    /// Errors during replay load (missing file, chart re-imported with
    /// different bytes) abort with a logged warning rather than crashing.
    pub fn start_course_replay(&mut self, course_id: i64, course_score_id: i64) {
        self.start_course_replay_with_auto_advance(course_id, course_score_id, false);
    }

    pub(super) fn start_course_replay_with_auto_advance(
        &mut self,
        course_id: i64,
        course_score_id: i64,
        auto_advance_intermediate_results: bool,
    ) {
        let started_at = Instant::now();
        let course_load_started_at = Instant::now();
        match self.boot.library_db.repair_course_entry_chart_links_for_course(course_id) {
            Ok(repaired) if repaired != 0 => {
                tracing::info!(course_id, repaired, "repaired course links before course replay");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, course_id, "failed to repair course links before course replay");
            }
        }
        let stored = match self.boot.library_db.course_by_id(course_id) {
            Ok(course) => course,
            Err(error) => {
                tracing::error!(
                    %error,
                    course_id,
                    "failed to load course for start_course_replay"
                );
                return;
            }
        };
        let Some(stored) = stored else {
            tracing::warn!(course_id, "course not found");
            return;
        };
        let course_load_elapsed = course_load_started_at.elapsed();

        let score_entry = match self.boot.score_db.course_score_entry_by_id(course_score_id) {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                tracing::warn!(course_id, course_score_id, "course score not found for replay");
                return;
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    course_id,
                    course_score_id,
                    "failed to load course score for replay"
                );
                return;
            }
        };

        let Some(identity) =
            crate::ir::course_payload::course_identity_from_stored(&self.boot.library_db, &stored)
        else {
            tracing::warn!(course_id, course_score_id, "course identity unavailable for replay");
            return;
        };
        if identity.course_hash != score_entry.course_hash {
            tracing::warn!(
                course_id,
                course_score_id,
                expected_course_hash = %identity.course_hash,
                stored_course_hash = %score_entry.course_hash,
                "course score does not belong to the current course definition"
            );
            return;
        }
        match self.boot.score_db.course_replay_attempt_is_complete(course_score_id) {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(
                    course_id,
                    course_score_id,
                    "course replay rows are incomplete or inconsistent"
                );
                return;
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    course_id,
                    course_score_id,
                    "failed to validate course replay rows"
                );
                return;
            }
        }

        let entries = match self.boot.score_db.list_course_replays(course_score_id) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::error!(
                    %error,
                    course_id,
                    course_score_id,
                    "failed to list course_replays rows"
                );
                return;
            }
        };
        if entries.is_empty() {
            tracing::warn!(course_id, course_score_id, "no replays saved for this attempt");
            return;
        }

        let entry_tuples: Vec<(i64, [u8; 32], String)> =
            entries.iter().map(|r| (r.position, r.chart_sha256, r.replay_path.clone())).collect();
        let replay_root = self.boot.profile_paths.root_dir.clone();
        let lookup = |chart_sha256: [u8; 32]| -> anyhow::Result<Option<i64>> {
            self.boot.library_db.available_chart_id_by_sha256(chart_sha256)
        };
        let mut queued = match crate::storage::replay::load_course_replays(
            &entry_tuples,
            &replay_root,
            lookup,
        ) {
            Ok(q) => q,
            Err(error) => {
                tracing::warn!(
                    %error,
                    course_id,
                    course_score_id,
                    "failed to load queued course replays"
                );
                return;
            }
        };

        let mut definition = stored.definition;
        if let Err(error) = resolve_course_chart_sources(&self.boot.library_db, &mut definition) {
            tracing::warn!(%error, course_id, "failed to resolve replay course chart sources");
            return;
        }
        let replay_layout_matches = queued.len() <= definition.entries.len()
            && queued.iter_mut().enumerate().all(|(index, replay)| {
                let expected_id = definition.entries.get(index).and_then(|entry| entry.chart_id);
                let expected_hash = expected_id.and_then(|id| {
                    self.boot.library_db.chart_sha256_by_chart_id(id).ok().flatten()
                });
                let matches = expected_hash == Some(replay.chart_sha256);
                if matches && let Some(id) = expected_id {
                    replay.chart_id = id;
                }
                replay.position == index as i64 && matches
            });
        if !replay_layout_matches {
            tracing::warn!(
                course_id,
                course_score_id,
                replays = queued.len(),
                entries = definition.entries.len(),
                "course replay positions do not match the current course entries"
            );
            return;
        }
        let first_chart_id = queued.first().map(|replay| replay.chart_id);
        let Some(first_chart_id) = first_chart_id else {
            tracing::warn!(course_id, course_score_id, "no replayable chart in course");
            return;
        };
        tracing::info!(
            course_id,
            course_score_id,
            title = %definition.title,
            replays = queued.len(),
            "starting course replay"
        );
        let mut entry_start_options = Vec::with_capacity(definition.entries.len());
        for index in 0..definition.entries.len() {
            let mut options = self.play_start_options();
            options.score_save_disabled = false;
            options.session_mode = SessionMode::Normal;
            options.autoplay = false;
            apply_course_constraints(&mut options, &definition.constraints);
            if let Some(replay) = queued.get(index)
                && let Err(error) = apply_queued_replay(&mut options, replay)
            {
                tracing::warn!(
                    %error,
                    course_id,
                    index,
                    "unsupported course replay arrangement"
                );
                return;
            }
            entry_start_options.push(options);
        }
        let options = entry_start_options[0].clone();
        let ln_policy = score_entry.ln_policy;
        let ln_policy_setting = ln_policy.as_setting();
        let rule_mode = score_entry.rule_mode;
        let metadata_started_at = Instant::now();
        let library_snapshot = match course_play_metrics_from_library_metadata(
            &self.boot.library_db,
            &definition,
            ln_policy_setting,
            &entry_start_options,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::error!(%error, course_id, "failed to load replay course chart metadata");
                return;
            }
        };
        let metadata_elapsed = metadata_started_at.elapsed();
        apply_course_entry_title_hints(&mut definition, &library_snapshot.titles);
        let course_title = definition.title.clone();
        let course_metrics = library_snapshot.metrics;
        let first_chart = library_snapshot.first_chart;
        self.play.active_course = Some(ActiveCourseSession {
            course_id,
            definition,
            ln_policy_setting,
            ln_policy,
            rule_mode,
            score_save_disabled: false,
            course_total_notes: course_metrics.total_notes,
            course_ln_mode: course_metrics.ln_mode,
            current_index: 0,
            entry_results: Vec::new(),
            entry_start_options,
            replay_stage_limit: Some(queued.len()),
            auto_advance_intermediate_results,
        });
        self.begin_course_decide_for_chart(first_chart_id, options, &course_title, first_chart);
        self.begin_course_metrics_background(course_id, first_chart_id, started_at, false);
        tracing::info!(
            course_id,
            course_score_id,
            course_load_elapsed_ms = course_load_elapsed.as_millis(),
            metadata_elapsed_ms = metadata_elapsed.as_millis(),
            metadata_total_notes = course_metrics.total_notes,
            select_to_decide_elapsed_ms = started_at.elapsed().as_millis(),
            "course replay decide transition scheduled"
        );
    }
}
