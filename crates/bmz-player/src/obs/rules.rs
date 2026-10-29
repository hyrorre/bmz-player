use super::*;
use crate::config::app_config::ObsRuleAction;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_QUEUED_RULES: usize = 2048;

#[cfg(test)]
mod tests;

struct QueuedRule {
    action: ObsRuleAction,
    generation: Option<u64>,
    retry_count: u8,
    ready_at: TokioInstant,
}

struct PendingRule {
    request_id: String,
    request_type: &'static str,
    lookup: bool,
    rule: QueuedRule,
    deadline: TokioInstant,
}

enum RetryRecording {
    DrainEnd,
    Stopping(TokioInstant),
    RestartAt(TokioInstant),
    Starting,
}

#[derive(Default)]
struct RuleQueue {
    generation: u64,
    queue: VecDeque<QueuedRule>,
    pending: Option<PendingRule>,
    shutdown: Option<TokioInstant>,
    retry: Option<RetryRecording>,
}

impl RuleQueue {
    fn begin_retry(&mut self, state: &mut ObsConnectionState) {
        self.advance();
        self.retry = None;
        let mut events = state.lifecycle.retry();
        state.pending_stop_deadline = None;
        if state.is_recording {
            events.retain(|event| *event != ObsTrigger::PlayStart);
            self.retry = Some(RetryRecording::DrainEnd);
            state.restart_recording = true;
        }
        self.enqueue(&state.config, &events, false);
    }

    fn advance(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.queue.retain(|rule| !rule.action.is_visual() || rule.generation.is_none());
        // A late lookup must never issue SetSceneItemEnabled for an old screen.
        // Already submitted setters complete before the next setter on this socket.
        if self.pending.as_ref().is_some_and(|p| p.lookup && p.rule.generation.is_some()) {
            self.pending = None;
        }
    }

    fn enqueue(&mut self, config: &ObsConfig, events: &[ObsTrigger], visual_only: bool) {
        for trigger in events {
            for rule in config.rules.iter().flatten().take(512).filter(|r| r.trigger == *trigger) {
                if self.queue.len() >= MAX_QUEUED_RULES {
                    tracing::warn!("OBS command queue limit reached");
                    return;
                }
                if !rule.action.is_valid() || (visual_only && !rule.action.is_visual()) {
                    continue;
                }
                self.queue.push_back(QueuedRule {
                    action: rule.action.clone(),
                    generation: (!matches!(trigger, ObsTrigger::MainStart | ObsTrigger::MainEnd))
                        .then_some(self.generation),
                    retry_count: 0,
                    ready_at: TokioInstant::now(),
                });
            }
        }
    }

    fn stale(&self, rule: &QueuedRule) -> bool {
        rule.action.is_visual() && rule.generation.is_some_and(|g| g != self.generation)
    }

    async fn send(
        &mut self,
        state: &mut ObsConnectionState,
        sink: &mut ObsSink,
        rule: QueuedRule,
        request_type: &'static str,
        data: Option<Value>,
        lookup: bool,
    ) -> Result<()> {
        let request_id = state.next_request_id(request_type);
        send_json(sink, request_message(request_type, &request_id, data)).await?;
        self.pending = Some(PendingRule {
            request_id,
            request_type,
            lookup,
            rule,
            deadline: TokioInstant::now() + REQUEST_TIMEOUT,
        });
        Ok(())
    }

    async fn pump(&mut self, state: &mut ObsConnectionState, sink: &mut ObsSink) -> Result<()> {
        if self.pending.is_some() {
            return Ok(());
        }
        match self.retry {
            Some(RetryRecording::Stopping(deadline)) if deadline > TokioInstant::now() => {
                return Ok(());
            }
            Some(RetryRecording::Stopping(_)) => {
                // A missing STOPPED event must not hold the command queue forever.
                state.restart_recording = false;
                self.retry = None;
                self.enqueue(&state.config, &[ObsTrigger::PlayStart], false);
            }
            Some(RetryRecording::RestartAt(deadline)) if deadline > TokioInstant::now() => {
                return Ok(());
            }
            Some(RetryRecording::RestartAt(_)) => {
                self.retry = Some(RetryRecording::Starting);
                let rule = QueuedRule {
                    action: ObsRuleAction::StartRecord,
                    generation: Some(self.generation),
                    retry_count: 0,
                    ready_at: TokioInstant::now(),
                };
                return self.send(state, sink, rule, "StartRecord", None, false).await;
            }
            _ => {}
        }
        loop {
            // A following command/event flushes a delayed stop before it executes.
            if state.pending_stop_deadline.is_some()
                && (!self.queue.is_empty()
                    || state.pending_stop_deadline.is_some_and(|d| d <= TokioInstant::now())
                    || self.shutdown.is_some())
            {
                state.pending_stop_deadline = None;
                let rule = QueuedRule {
                    action: ObsRuleAction::StopRecord,
                    generation: None,
                    retry_count: 0,
                    ready_at: TokioInstant::now(),
                };
                return self.send(state, sink, rule, "StopRecord", None, false).await;
            }
            let Some(rule) = self.queue.pop_front() else {
                if matches!(self.retry, Some(RetryRecording::DrainEnd)) {
                    state.pending_stop_deadline = None;
                    if !state.is_recording {
                        self.retry = Some(RetryRecording::RestartAt(
                            TokioInstant::now() + RESTART_RECORDING_DELAY,
                        ));
                        return Ok(());
                    }
                    self.retry =
                        Some(RetryRecording::Stopping(TokioInstant::now() + REQUEST_TIMEOUT));
                    state.restart_recording = true;
                    let rule = QueuedRule {
                        action: ObsRuleAction::StopRecord,
                        generation: Some(self.generation),
                        retry_count: 0,
                        ready_at: TokioInstant::now(),
                    };
                    return self.send(state, sink, rule, "StopRecord", None, false).await;
                }
                return Ok(());
            };
            if self.stale(&rule) {
                continue;
            }
            if rule.ready_at > TokioInstant::now() {
                self.queue.push_front(rule);
                return Ok(());
            }
            match &rule.action {
                ObsRuleAction::SwitchScene { scene_name } => {
                    let data = json!({ "sceneName": scene_name });
                    return self
                        .send(state, sink, rule, "SetCurrentProgramScene", Some(data), false)
                        .await;
                }
                ObsRuleAction::ShowSource { scene_name, source_name }
                | ObsRuleAction::HideSource { scene_name, source_name } => {
                    let data = json!({ "sceneName": scene_name, "sourceName": source_name, "searchOffset": 0 });
                    return self.send(state, sink, rule, "GetSceneItemId", Some(data), true).await;
                }
                ObsRuleAction::StartRecord => {
                    if !state.is_recording {
                        return self.send(state, sink, rule, "StartRecord", None, false).await;
                    }
                }
                ObsRuleAction::StopRecord => {
                    let delay = if self.shutdown.is_some() {
                        0
                    } else {
                        state.config.record_stop_wait_ms.min(10_000)
                    };
                    if delay == 0 {
                        return self.send(state, sink, rule, "StopRecord", None, false).await;
                    }
                    state.pending_stop_deadline =
                        Some(TokioInstant::now() + Duration::from_millis(delay));
                }
            }
        }
    }

    async fn response(
        &mut self,
        state: &mut ObsConnectionState,
        sink: &mut ObsSink,
        data: &Value,
        status_tx: &watch::Sender<ObsConnectionStatus>,
    ) -> Result<()> {
        let Some(pending) = self.pending.as_ref() else {
            return Ok(());
        };
        if data["requestId"].as_str() != Some(pending.request_id.as_str())
            || data["requestType"].as_str() != Some(pending.request_type)
        {
            return Ok(());
        }
        let Some(mut pending) = self.pending.take() else {
            return Ok(());
        };
        if self.stale(&pending.rule) {
            return Ok(());
        }
        let restarting = matches!(self.retry, Some(RetryRecording::Starting))
            && pending.request_type == "StartRecord";
        if restarting {
            self.retry = None;
            self.enqueue(&state.config, &[ObsTrigger::PlayStart], false);
        }
        if data["requestStatus"]["result"].as_bool() != Some(true) {
            if restarting {
                self.queue.retain(|r| {
                    r.generation != Some(self.generation) || r.action != ObsRuleAction::StartRecord
                });
            }
            let code = data["requestStatus"]["code"].as_i64().unwrap_or_default();
            if code == 207
                && pending.request_type == "SetCurrentProgramScene"
                && pending.rule.retry_count < MAX_SCENE_NOT_READY_RETRIES
            {
                pending.rule.retry_count += 1;
                pending.rule.ready_at =
                    TokioInstant::now() + scene_not_ready_retry_delay(pending.rule.retry_count);
                self.queue.push_front(pending.rule);
            } else {
                rule_error(
                    status_tx,
                    format!(
                        "{}: {} ({code})",
                        pending.request_type,
                        data["requestStatus"]["comment"].as_str().unwrap_or_default()
                    ),
                );
            }
            return Ok(());
        }
        if pending.request_type == "StartRecord" {
            state.is_recording = true;
        }
        if pending.request_type == "StopRecord" {
            state.is_recording = false;
            if matches!(self.retry, Some(RetryRecording::Stopping(_) | RetryRecording::DrainEnd)) {
                // StopRecord returns outputPath as well as emitting STOPPED. Whichever
                // arrives first completes the retry stop; retention runs exactly once.
                state.handle_record_state_changed(sink, &json!({
                    "outputState": "OBS_WEBSOCKET_OUTPUT_STOPPED",
                    "outputPath": data["responseData"]["outputPath"].as_str().unwrap_or_default(),
                })).await?;
                if !matches!(self.retry, Some(RetryRecording::DrainEnd)) {
                    self.retry = Some(RetryRecording::RestartAt(
                        TokioInstant::now() + RESTART_RECORDING_DELAY,
                    ));
                }
            }
        }
        if pending.lookup {
            let Some(id) = data["responseData"]["sceneItemId"].as_i64().filter(|id| *id >= 0)
            else {
                rule_error(status_tx, "GetSceneItemId: sceneItemId".into());
                return Ok(());
            };
            let (scene_name, enabled) = match &pending.rule.action {
                ObsRuleAction::ShowSource { scene_name, .. } => (scene_name, true),
                ObsRuleAction::HideSource { scene_name, .. } => (scene_name, false),
                _ => return Ok(()),
            };
            let data =
                json!({ "sceneName": scene_name, "sceneItemId": id, "sceneItemEnabled": enabled });
            self.send(state, sink, pending.rule, "SetSceneItemEnabled", Some(data), false).await?;
        }
        Ok(())
    }
}

fn rule_error(status_tx: &watch::Sender<ObsConnectionStatus>, error: String) {
    tracing::warn!(%error, "OBS command failed");
    publish_status(
        status_tx,
        ObsConnectionStatus::new(ObsConnectionStatusKind::Connected, None, Some(error), None),
    );
}

pub(super) async fn run_rules_connected(
    state: &mut ObsConnectionState,
    ws: ObsWebSocket,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    tx: mpsc::UnboundedSender<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
    status_tx: &watch::Sender<ObsConnectionStatus>,
) -> Result<ConnectionAction> {
    let (mut sink, mut stream) = ws.split();
    match wait_for_identified(state, &mut sink, &mut stream, rx, pending_commands, status_tx)
        .await?
    {
        ConnectionAction::Continue => {}
        action => return Ok(action),
    }
    // Retain only the latest offline scene, never replay historical record actions.
    while let Ok(command) = rx.try_recv() {
        pending_commands.push_back(command);
    }
    let mut latest_scene = state.lifecycle.scene;
    while let Some(command) = pending_commands.pop_front() {
        match command {
            ObsCommand::Shutdown | ObsCommand::Close => return Ok(ConnectionAction::Shutdown),
            ObsCommand::ApplyEvent(key) if key != ObsEventKey::PlayEnded => {
                latest_scene = Some(key)
            }
            ObsCommand::SaveLastRecording(reason) => state.save_last_recording(reason),
            _ => {}
        }
    }
    let mut queue = RuleQueue::default();
    let initial = !state.main_started;
    if initial {
        state.main_started = true;
        queue.enqueue(&state.config, &[ObsTrigger::MainStart], false);
    }
    state.lifecycle.scene = latest_scene;
    if let Some(scene) = latest_scene {
        queue.enqueue(&state.config, &[scene.start_trigger()], !initial);
    }
    let status_deadline = TokioInstant::now() + REQUEST_TIMEOUT;
    let mut deferred_retry = None;
    // A single worker owns the socket, ordered requests and deadlines. No UI waits.
    loop {
        let waiting_status = state.pending_requests.values().any(|kind| kind == "GetRecordStatus");
        if waiting_status && TokioInstant::now() >= status_deadline {
            state.pending_requests.retain(|_, kind| kind != "GetRecordStatus");
        }
        if !waiting_status
            && let Some(generation) = deferred_retry.take()
            && generation == queue.generation
        {
            queue.begin_retry(state);
        }
        if !waiting_status || TokioInstant::now() >= status_deadline {
            queue.pump(state, &mut sink).await?;
        }
        if queue.shutdown.is_some_and(|d| d <= TokioInstant::now())
            || (queue.shutdown.is_some()
                && queue.pending.is_none()
                && queue.queue.is_empty()
                && state.pending_stop_deadline.is_none())
        {
            let _ =
                tokio::time::timeout(Duration::from_millis(100), sink.send(Message::Close(None)))
                    .await;
            return Ok(ConnectionAction::Shutdown);
        }
        let mut deadline = TokioInstant::now() + Duration::from_secs(3600);
        let retry_deadline = match queue.retry {
            Some(RetryRecording::Stopping(d) | RetryRecording::RestartAt(d)) => Some(d),
            _ => None,
        };
        let blocked = queue.pending.is_some() || retry_deadline.is_some() || waiting_status;
        for candidate in [
            queue.pending.as_ref().map(|p| p.deadline),
            queue.shutdown,
            waiting_status.then_some(status_deadline),
            state.pending_stop_deadline.filter(|_| !blocked),
            retry_deadline,
            queue.queue.front().filter(|_| !blocked).map(|r| r.ready_at),
        ]
        .into_iter()
        .flatten()
        {
            deadline = deadline.min(candidate);
        }
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                if queue.pending.as_ref().is_some_and(|p| p.deadline <= TokioInstant::now())
                    && let Some(pending) = queue.pending.take() {
                        rule_error(status_tx, format!("{}: timeout", pending.request_type));
                        if matches!(queue.retry, Some(RetryRecording::Starting)) {
                            queue.retry = None;
                            queue.enqueue(&state.config, &[ObsTrigger::PlayStart], false);
                            queue.queue.retain(|r| r.generation != Some(queue.generation) || r.action != ObsRuleAction::StartRecord);
                        }
                }
            }
            command = rx.recv(), if queue.shutdown.is_none() => {
                match command {
                    Some(ObsCommand::ApplyEvent(key)) => {
                        let events = state.lifecycle.transition(key);
                        if !events.is_empty() {
                            queue.advance(); queue.retry = None; state.restart_recording = false;
                            if state.pending_stop_deadline.is_some() {
                                state.pending_stop_deadline = Some(TokioInstant::now());
                            }
                            queue.enqueue(&state.config, &events, false);
                        }
                    }
                    Some(ObsCommand::RetryPlay) => {
                        if waiting_status {
                            deferred_retry = Some(queue.generation);
                        } else {
                            queue.begin_retry(state);
                        }
                    }
                    Some(ObsCommand::SaveLastRecording(reason)) => state.save_last_recording(reason),
                    Some(ObsCommand::Shutdown) | None => {
                        queue.advance();
                        queue.queue.clear();
                        if queue.pending.as_ref().is_some_and(|pending| pending.lookup) {
                            queue.pending = None;
                        }
                        queue.retry = None; state.restart_recording = false;
                        let mut events = state.lifecycle.end_scene();
                        events.push(ObsTrigger::MainEnd);
                        queue.enqueue(&state.config, &events, false);
                        queue.shutdown = Some(TokioInstant::now() + SHUTDOWN_TIMEOUT);
                    }
                    Some(ObsCommand::Close) => return Ok(ConnectionAction::Shutdown),
                    _ => {},
                }
            }
            message = stream.next() => {
                let Some(message) = message else { return Ok(if queue.shutdown.is_some() { ConnectionAction::Shutdown } else { ConnectionAction::Reconnect }); };
                match message? {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_ref())?;
                        if value["op"] == 7 {
                            let data = &value["d"];
                            queue.response(state, &mut sink, data, status_tx).await?;
                            // Preserve the existing recording/retention state machine.
                            if matches!(data["requestType"].as_str(), Some("GetVersion" | "GetSceneList" | "GetRecordStatus")) {
                                state.handle_response(&mut sink, data, status_tx, tx.clone()).await?;
                            }
                        } else {
                            let stopped_retry = value["op"] == 5 && value["d"]["eventType"] == "RecordStateChanged"
                                && output_state_stopped(value["d"]["eventData"]["outputState"].as_str().unwrap_or_default())
                                && matches!(queue.retry, Some(RetryRecording::Stopping(_)));
                            match handle_text_message(state, &mut sink, text.as_ref(), status_tx, tx.clone()).await? {
                                ConnectionAction::Continue => {},
                                action => return Ok(if queue.shutdown.is_some() { ConnectionAction::Shutdown } else { action }),
                            }
                            if stopped_retry {
                                queue.retry = Some(RetryRecording::RestartAt(TokioInstant::now() + RESTART_RECORDING_DELAY));
                            }
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(close) => return Ok(if queue.shutdown.is_some() { ConnectionAction::Shutdown } else { connection_action_for_close(state, close) }),
                    _ => {},
                }
            }
        }
    }
}
