use super::*;

impl ObsConnectionState {
    pub(super) fn new(config: ObsConfig) -> Self {
        Self {
            main_started: false,
            lifecycle: lifecycle::ObsLifecycle::default(),
            config,
            request_counter: 0,
            is_recording: false,
            restart_recording: false,
            save_requested: false,
            last_output_path: None,
            pending_stop_deadline: None,
            pending_scene_requests: HashMap::new(),
            pending_requests: HashMap::new(),
            scene_request_generation: 0,
            was_identified: false,
            identified_this_connection: false,
            last_disconnect: None,
        }
    }

    pub(super) fn begin_connection(&mut self) {
        self.identified_this_connection = false;
        self.last_disconnect = None;
        self.pending_scene_requests.clear();
        self.pending_requests.clear();
        if self.config.rules.is_some() {
            self.restart_recording = false;
            self.pending_stop_deadline = None;
        }
    }

    pub(super) fn mark_identified(&mut self) {
        self.was_identified = true;
        self.identified_this_connection = true;
    }

    pub(super) fn set_disconnect(&mut self, detail: impl Into<String>, expected: bool) {
        self.last_disconnect = Some(ObsDisconnect { detail: detail.into(), expected });
    }

    pub(super) fn next_request_id(&mut self, request_type: &str) -> String {
        self.request_counter = self.request_counter.wrapping_add(1);
        format!("{request_type}-{}", self.request_counter)
    }

    pub(super) async fn send_request(
        &mut self,
        sink: &mut ObsSink,
        request_type: &str,
        request_data: Option<Value>,
    ) -> Result<()> {
        let request_id = self.next_request_id(request_type);
        if self.pending_requests.len() >= 1024 {
            self.pending_requests.clear();
        }
        self.pending_requests.insert(request_id.clone(), request_type.to_owned());
        send_json(sink, request_message(request_type, &request_id, request_data)).await
    }

    pub(super) async fn apply_event(&mut self, sink: &mut ObsSink, key: ObsEventKey) -> Result<()> {
        if self.pending_stop_deadline.take().is_some() {
            self.send_request(sink, "StopRecord", None).await?;
        }

        self.scene_request_generation = self.scene_request_generation.wrapping_add(1);
        self.apply_scene_for_event(sink, key, 0, self.scene_request_generation).await?;

        let action = self.config.actions.get(key.config_key()).copied().unwrap_or_default();
        self.apply_action(sink, action).await
    }

    pub(super) async fn retry_scene(
        &mut self,
        sink: &mut ObsSink,
        key: ObsEventKey,
        retry_count: u8,
        generation: u64,
    ) -> Result<()> {
        if generation != self.scene_request_generation {
            return Ok(());
        }
        self.apply_scene_for_event(sink, key, retry_count, generation).await
    }

    pub(super) async fn apply_scene_for_event(
        &mut self,
        sink: &mut ObsSink,
        key: ObsEventKey,
        retry_count: u8,
        generation: u64,
    ) -> Result<()> {
        let scene = self
            .config
            .scenes
            .get(key.config_key())
            .map(|scene| scene.trim())
            .filter(|scene| !scene.is_empty())
            .map(ToOwned::to_owned);
        if let Some(scene) = scene {
            let request_id = self.next_request_id("SetCurrentProgramScene");
            self.pending_scene_requests
                .insert(request_id.clone(), PendingSceneRequest { key, retry_count, generation });
            send_json(
                sink,
                request_message(
                    "SetCurrentProgramScene",
                    &request_id,
                    Some(json!({ "sceneName": scene })),
                ),
            )
            .await?;
        }
        Ok(())
    }

    pub(super) async fn apply_action(
        &mut self,
        sink: &mut ObsSink,
        action: ObsActionConfig,
    ) -> Result<()> {
        match action {
            ObsActionConfig::None => Ok(()),
            ObsActionConfig::StartRecord => self.send_request(sink, "StartRecord", None).await,
            ObsActionConfig::StopRecord => {
                let wait_ms = self.config.record_stop_wait_ms.min(10_000);
                if wait_ms == 0 {
                    self.send_request(sink, "StopRecord", None).await
                } else {
                    self.pending_stop_deadline =
                        Some(TokioInstant::now() + Duration::from_millis(wait_ms));
                    Ok(())
                }
            }
        }
    }

    pub(super) async fn flush_pending_stop(&mut self, sink: &mut ObsSink) -> Result<()> {
        if self.pending_stop_deadline.take().is_some() {
            self.send_request(sink, "StopRecord", None).await?;
        }
        Ok(())
    }

    pub(super) async fn retry_play(
        &mut self,
        sink: &mut ObsSink,
        tx: mpsc::UnboundedSender<ObsCommand>,
    ) -> Result<()> {
        self.pending_stop_deadline = None;
        if self.is_recording {
            self.restart_recording = true;
            self.send_request(sink, "StopRecord", None).await?;
        }
        self.apply_event(sink, ObsEventKey::MusicSelect).await?;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let _ = tx.send(ObsCommand::ApplyEvent(ObsEventKey::Play));
        });
        Ok(())
    }

    pub(super) fn save_last_recording(&mut self, reason: ObsRecordingSaveReason) {
        if recording_mode_matches(self.config.recording_mode, reason) {
            self.save_requested = true;
            tracing::info!(?reason, "OBS recording keep requested");
        }
    }

    pub(super) async fn handle_response(
        &mut self,
        sink: &mut ObsSink,
        data: &Value,
        status_tx: &watch::Sender<ObsConnectionStatus>,
        tx: mpsc::UnboundedSender<ObsCommand>,
    ) -> Result<()> {
        let request_type = data.get("requestType").and_then(Value::as_str).unwrap_or_default();
        let request_id = data.get("requestId").and_then(Value::as_str).unwrap_or_default();
        let scene_request = self.pending_scene_requests.remove(request_id);
        if scene_request.is_none()
            && self.pending_requests.remove(request_id).as_deref() != Some(request_type)
        {
            return Ok(());
        }
        let status = data.get("requestStatus").unwrap_or(&Value::Null);
        if !status.get("result").and_then(Value::as_bool).unwrap_or(false) {
            let code = status.get("code").and_then(Value::as_i64).unwrap_or_default();
            let comment = status.get("comment").and_then(Value::as_str).unwrap_or_default();
            if code == 207
                && request_type == "SetCurrentProgramScene"
                && let Some(scene_request) = scene_request
            {
                if scene_request.generation != self.scene_request_generation {
                    tracing::debug!(request_type, code, "ignoring stale OBS scene request");
                    return Ok(());
                }
                if scene_request.retry_count < MAX_SCENE_NOT_READY_RETRIES {
                    let retry_count = scene_request.retry_count + 1;
                    let retry_delay = scene_not_ready_retry_delay(retry_count);
                    let retry_in_ms = retry_delay.as_millis() as u64;
                    publish_status(
                        status_tx,
                        ObsConnectionStatus::new(
                            ObsConnectionStatusKind::Connected,
                            Some(format!(
                                "OBS のシーン切替準備を待機中です。{retry_in_ms} ms 後に再試行します。"
                            )),
                            None,
                            Some(retry_in_ms),
                        ),
                    );
                    tracing::debug!(
                        request_type,
                        code,
                        retry_count,
                        retry_in_ms,
                        "OBS is not ready; retrying scene request"
                    );
                    tokio::spawn(async move {
                        tokio::time::sleep(retry_delay).await;
                        let _ = tx.send(ObsCommand::RetryScene {
                            key: scene_request.key,
                            retry_count,
                            generation: scene_request.generation,
                        });
                    });
                    return Ok(());
                }
            }

            if code == 207 && request_type != "SetCurrentProgramScene" {
                publish_status(
                    status_tx,
                    ObsConnectionStatus::new(
                        ObsConnectionStatusKind::Connected,
                        Some("OBS の準備完了を待機しています。".to_string()),
                        None,
                        None,
                    ),
                );
                tracing::debug!(request_type, code, comment, "OBS is not ready for request");
                return Ok(());
            }
            let error = if comment.is_empty() {
                format!("{request_type} が OBS に拒否されました (code {code})")
            } else {
                format!("{request_type}: {comment} (code {code})")
            };
            publish_status(
                status_tx,
                ObsConnectionStatus::new(
                    ObsConnectionStatusKind::Connected,
                    Some("OBS は接続済みですが、要求が拒否されました。".to_string()),
                    Some(error),
                    None,
                ),
            );
            tracing::error!(kind = "request", request_type, code, comment, "OBS request failed");
            return Ok(());
        }
        publish_status(
            status_tx,
            ObsConnectionStatus::new(ObsConnectionStatusKind::Connected, None, None, None),
        );
        let response_data = data.get("responseData").unwrap_or(&Value::Null);
        match request_type {
            "GetVersion" => {
                let obs_version =
                    response_data.get("obsVersion").and_then(Value::as_str).unwrap_or_default();
                let ws_version = response_data
                    .get("obsWebSocketVersion")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                tracing::info!(obs_version, ws_version, "OBS WebSocket identified");
            }
            "GetSceneList" => {
                let scenes = parse_scene_names(response_data);
                tracing::info!(count = scenes.len(), "OBS scene list loaded");
            }
            "GetRecordStatus" => {
                self.is_recording =
                    response_data.get("outputActive").and_then(Value::as_bool).unwrap_or(false);
            }
            "StopRecord" if self.restart_recording && !self.is_recording => {
                tokio::time::sleep(RESTART_RECORDING_DELAY).await;
                self.send_request(sink, "StartRecord", None).await?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) async fn handle_event(
        &mut self,
        sink: &mut ObsSink,
        data: &Value,
    ) -> Result<ConnectionAction> {
        let event_type = data.get("eventType").and_then(Value::as_str).unwrap_or_default();
        let event_data = data.get("eventData").unwrap_or(&Value::Null);
        match event_type {
            "ExitStarted" => {
                tracing::info!("OBS exit started");
                self.set_disconnect("OBS が終了しました。", true);
                Ok(ConnectionAction::Reconnect)
            }
            "AuthenticationFailure" | "AuthenticationFailed" => Ok(ConnectionAction::Pause {
                kind: ObsConnectionStatusKind::AuthenticationFailed,
                detail: "OBS WebSocket 認証に失敗しました。パスワードを確認して保存してください。"
                    .to_string(),
            }),
            "RecordStateChanged" => {
                self.handle_record_state_changed(sink, event_data).await?;
                Ok(ConnectionAction::Continue)
            }
            _ => Ok(ConnectionAction::Continue),
        }
    }

    pub(super) async fn handle_record_state_changed(
        &mut self,
        sink: &mut ObsSink,
        data: &Value,
    ) -> Result<()> {
        let state = data.get("outputState").and_then(Value::as_str).unwrap_or_default();
        let output_path = data.get("outputPath").and_then(Value::as_str).unwrap_or_default();
        if output_state_started(state) {
            self.is_recording = true;
            if let Some(path) = self.last_output_path.take() {
                if self.save_requested {
                    tracing::info!(path = %path.display(), "OBS recording kept");
                } else {
                    delete_recording_file(path, self.config.recording_mode, "previous recording");
                }
            }
            self.save_requested = false;
            return Ok(());
        }

        if output_state_stopped(state) {
            self.is_recording = false;
            if self.restart_recording {
                self.restart_recording = false;
                delete_recording_file(
                    PathBuf::from(output_path),
                    self.config.recording_mode,
                    "retry recording",
                );
                if self.config.rules.is_some() {
                    return Ok(());
                }
                tokio::time::sleep(RESTART_RECORDING_DELAY).await;
                self.send_request(sink, "StartRecord", None).await?;
                return Ok(());
            }
            if !output_path.is_empty() {
                self.last_output_path = Some(PathBuf::from(output_path));
            }
        }
        Ok(())
    }
}
