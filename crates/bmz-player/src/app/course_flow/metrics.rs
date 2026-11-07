use super::*;

impl WinitApp {
    pub(super) fn begin_course_metrics_background(
        &mut self,
        course_id: i64,
        first_chart_id: i64,
        requested_at: Instant,
        update_ln_policy: bool,
    ) {
        self.play.course_metrics_generation = self.play.course_metrics_generation.wrapping_add(1);
        self.play.pending_course_metrics = Some(PendingCourseMetrics {
            generation: self.play.course_metrics_generation,
            course_id,
            first_chart_id,
            requested_at,
            update_ln_policy,
            worker_started_at: None,
            rx: None,
        });
    }

    pub(super) fn clear_pending_course_metrics(&mut self) {
        self.play.course_metrics_generation = self.play.course_metrics_generation.wrapping_add(1);
        self.play.pending_course_metrics = None;
    }

    pub(super) fn poll_course_metrics(&mut self) {
        let Some(pending) = self.play.pending_course_metrics.as_ref() else {
            return;
        };
        let is_current = pending.generation == self.play.course_metrics_generation
            && self
                .play
                .active_course
                .as_ref()
                .is_some_and(|course| course.course_id == pending.course_id);
        if !is_current {
            self.play.pending_course_metrics = None;
            return;
        }

        if pending.rx.is_none() {
            let first_chart_id = pending.first_chart_id;
            let Some(prepared) = self.play_preload_prepared_chart(first_chart_id) else {
                return;
            };
            let Some(course) = self.play.active_course.as_ref() else {
                return;
            };
            let definition = course.definition.clone();
            let entry_start_options = course.entry_start_options.clone();
            let ln_policy_setting = course.ln_policy_setting;
            let rule_mode = course.rule_mode;
            let first_metrics =
                crate::screens::play_session::scored_chart_metrics_from_prepared(&prepared);
            let first_chart_total_notes = first_metrics.total_notes;
            let library_db_path = self.boot.app_paths.library_db.clone();
            let app_config = self.play_session_app_config();
            let course_id = course.course_id;
            let (tx, rx) = mpsc::channel();
            let worker_started_at = Instant::now();
            thread::Builder::new()
                .name(format!("course-metrics-{course_id}"))
                .spawn(move || {
                    let result = (|| -> Result<CoursePlayMetrics> {
                        let library_db =
                            crate::storage::library_db::LibraryDatabase::open(&library_db_path)?;
                        course_play_metrics_for_definition_reusing_first(
                            &library_db,
                            &definition,
                            &app_config,
                            ln_policy_setting,
                            rule_mode,
                            &entry_start_options,
                            first_metrics,
                        )
                    })()
                    .map_err(|error| format!("{error:#}"));
                    let _ = tx.send(result);
                })
                .expect("failed to spawn course metrics thread");
            if let Some(pending) = self.play.pending_course_metrics.as_mut() {
                pending.worker_started_at = Some(worker_started_at);
                pending.rx = Some(rx);
                tracing::info!(
                    course_id,
                    first_chart_id,
                    wait_for_first_chart_ms = pending.requested_at.elapsed().as_millis(),
                    first_chart_total_notes,
                    "course background metrics started with reused play preload"
                );
            }
            return;
        }

        let received = self
            .play
            .pending_course_metrics
            .as_ref()
            .and_then(|pending| pending.rx.as_ref())
            .map(Receiver::try_recv);
        match received {
            Some(Ok(result)) => self.finish_course_metrics_result(result),
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                tracing::warn!(
                    "course background metrics worker disconnected; keeping metadata estimate"
                );
                self.play.pending_course_metrics = None;
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }
    }

    pub(super) fn await_course_metrics(&mut self) {
        self.poll_course_metrics();
        let Some(mut pending) = self.play.pending_course_metrics.take() else {
            return;
        };
        let Some(rx) = pending.rx.take() else {
            tracing::warn!(
                course_id = pending.course_id,
                "course background metrics never started; keeping metadata estimate"
            );
            return;
        };
        match rx.recv() {
            Ok(result) => {
                self.play.pending_course_metrics = Some(pending);
                self.finish_course_metrics_result(result);
            }
            Err(_) => tracing::warn!(
                course_id = pending.course_id,
                "course background metrics worker disconnected at finish; keeping metadata estimate"
            ),
        }
    }

    fn finish_course_metrics_result(
        &mut self,
        result: std::result::Result<CoursePlayMetrics, String>,
    ) {
        let Some(pending) = self.play.pending_course_metrics.take() else {
            return;
        };
        match result {
            Ok(metrics) => {
                let Some(course) = self
                    .play
                    .active_course
                    .as_mut()
                    .filter(|course| course.course_id == pending.course_id)
                else {
                    return;
                };
                let metadata_total_notes = course.course_total_notes;
                course.course_total_notes = metrics.total_notes;
                course.course_ln_mode = metrics.ln_mode;
                for (stage, provenance) in course.stage_charts.iter_mut().zip(metrics.entry_kaleid)
                {
                    stage.kaleid = Some(provenance);
                }
                if pending.update_ln_policy {
                    course.ln_policy = metrics.ln_policy;
                }
                tracing::info!(
                    course_id = pending.course_id,
                    metadata_total_notes,
                    exact_total_notes = metrics.total_notes,
                    total_elapsed_ms = pending.requested_at.elapsed().as_millis(),
                    worker_elapsed_ms = pending
                        .worker_started_at
                        .map(|started_at| started_at.elapsed().as_millis())
                        .unwrap_or_default(),
                    "course background metrics complete"
                );
            }
            Err(error) => tracing::warn!(
                course_id = pending.course_id,
                error = %error,
                "course background metrics failed; keeping metadata estimate"
            ),
        }
    }
}
