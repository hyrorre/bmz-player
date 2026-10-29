use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConnectionAction {
    Continue,
    Reconnect,
    Shutdown,
    Pause { kind: ObsConnectionStatusKind, detail: String },
}

pub async fn load_scenes(config: ObsConfig) -> Result<ObsSceneList> {
    tokio::time::timeout(LOAD_SCENES_TIMEOUT, load_scenes_inner(config))
        .await
        .context("OBS scene load timed out")?
}

async fn load_scenes_inner(config: ObsConfig) -> Result<ObsSceneList> {
    let url = obs_ws_url(&config);
    let timeout = tokio::time::sleep(LOAD_SCENES_TIMEOUT);
    tokio::pin!(timeout);
    let (ws, _) = tokio::select! {
        _ = &mut timeout => bail!("OBS scene load timed out"),
        result = connect_async(&url) => result.with_context(|| format!("failed to connect {url}"))?,
    };
    let (mut sink, mut stream) = ws.split();
    let mut state = ObsConnectionState::new(config);
    let mut version = None;
    let mut scenes = None;
    let mut recording_active = None;
    let mut sources = HashMap::new();
    let mut outstanding: HashMap<String, (String, Option<String>)> = HashMap::new();

    loop {
        tokio::select! {
            _ = &mut timeout => bail!("OBS scene load timed out"),
            message = stream.next() => {
                let Some(message) = message else {
                    bail!("OBS connection closed before scene list was received");
                };
                let message = message.context("failed to read OBS WebSocket message")?;
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_ref())
                            .context("failed to parse OBS WebSocket message")?;
                        match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
                            0 => {
                                let hello = value.get("d").unwrap_or(&Value::Null);
                                send_json(&mut sink, identify_message(&state.config, hello)).await?;
                            }
                            2 => {
                                for request in ["GetVersion", "GetSceneList", "GetRecordStatus"] {
                                    let id = state.next_request_id(request);
                                    outstanding.insert(id.clone(), (request.to_owned(), None));
                                    send_json(&mut sink, request_message(request, &id, None)).await?;
                                }
                            }
                            7 => {
                                let data = value.get("d").unwrap_or(&Value::Null);
                                let request_type = data
                                    .get("requestType")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default();
                                let id = data["requestId"].as_str().unwrap_or_default();
                                let Some((expected, scene)) = outstanding.remove(id) else { continue; };
                                if expected != request_type { continue; }
                                let status = data.get("requestStatus").unwrap_or(&Value::Null);
                                if !status.get("result").and_then(Value::as_bool).unwrap_or(false) {
                                    let comment = status
                                        .get("comment")
                                        .and_then(Value::as_str)
                                        .unwrap_or("OBS request failed");
                                    bail!("{request_type}: {comment}");
                                }
                                let response_data = data.get("responseData").unwrap_or(&Value::Null);
                                match request_type {
                                    "GetVersion" => {
                                        let obs_version = response_data
                                            .get("obsVersion")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default();
                                        let ws_version = response_data
                                            .get("obsWebSocketVersion")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default();
                                        version = Some(if ws_version.is_empty() {
                                            obs_version.to_string()
                                        } else if obs_version.is_empty() {
                                            ws_version.to_string()
                                        } else {
                                            format!("{obs_version} / obs-websocket {ws_version}")
                                        });
                                    }
                                    "GetSceneList" => {
                                        let names = parse_scene_names(response_data);
                                        for name in &names {
                                            let id = state.next_request_id("GetSceneItemList");
                                            outstanding.insert(id.clone(), ("GetSceneItemList".into(), Some(name.clone())));
                                            send_json(&mut sink, request_message("GetSceneItemList", &id, Some(json!({ "sceneName": name })))).await?;
                                        }
                                        scenes = Some(names);
                                    }
                                    "GetSceneItemList" => {
                                        if let Some(scene) = scene {
                                            let mut names = Vec::new();
                                            for item in response_data["sceneItems"].as_array().into_iter().flatten() {
                                                if let Some(name) = item["sourceName"].as_str()
                                                    && !names.iter().any(|n| n == name) {
                                                    names.push(name.to_owned());
                                                }
                                            }
                                            sources.insert(scene, names);
                                        }
                                    }
                                    "GetRecordStatus" => {
                                        recording_active = Some(response_data
                                            .get("outputActive")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false));
                                    }
                                    _ => {}
                                }
                                if let (Some(version), Some(scenes), Some(recording_active)) =
                                    (version.clone(), scenes.clone(), recording_active)
                                    && outstanding.is_empty()
                                {
                                    return Ok(ObsSceneList { version, scenes, recording_active, sources });
                                }
                            }
                            _ => {}
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(_) => bail!("OBS closed the WebSocket connection"),
                    _ => {}
                }
            }
        }
    }
}

pub(super) async fn run_obs_client(
    config: ObsConfig,
    mut rx: mpsc::UnboundedReceiver<ObsCommand>,
    tx: mpsc::UnboundedSender<ObsCommand>,
    status_tx: watch::Sender<ObsConnectionStatus>,
    startup: bool,
) {
    let mut reconnect_delay = INITIAL_RECONNECT_DELAY;
    // Keep recording retention and queued app events across WebSocket reconnects.
    let mut state = ObsConnectionState::new(config);
    state.main_started = !startup;
    let mut pending_commands = VecDeque::new();
    let mut last_reported_issue = None;
    loop {
        let url = obs_ws_url(&state.config);
        publish_status(
            &status_tx,
            ObsConnectionStatus::new(
                ObsConnectionStatusKind::Connecting,
                Some("OBS WebSocket へ接続しています。".to_string()),
                None,
                None,
            ),
        );
        match connect_with_pending_commands(&url, &mut rx, &mut pending_commands).await {
            Ok(Some(ws)) => {
                state.begin_connection();
                tracing::debug!(url, "OBS WebSocket transport connected");
                match run_connected(
                    &mut state,
                    ws,
                    &mut rx,
                    tx.clone(),
                    &mut pending_commands,
                    &status_tx,
                )
                .await
                {
                    Ok(ConnectionAction::Shutdown) => break,
                    Ok(ConnectionAction::Pause { kind, detail }) => {
                        pause_reconnect(&status_tx, kind, &url, detail, state.was_identified);
                        wait_for_shutdown(&mut rx).await;
                        break;
                    }
                    Ok(ConnectionAction::Reconnect | ConnectionAction::Continue) => {
                        if state.identified_this_connection {
                            reconnect_delay = INITIAL_RECONNECT_DELAY;
                            last_reported_issue = None;
                        }
                        let disconnect = state.last_disconnect.take().unwrap_or(ObsDisconnect {
                            detail: "OBS WebSocket 接続が終了しました。".to_string(),
                            expected: false,
                        });
                        report_disconnect(
                            &status_tx,
                            &mut last_reported_issue,
                            &url,
                            &disconnect,
                            reconnect_delay,
                            state.was_identified,
                        );
                    }
                    Err(error) => {
                        if state.identified_this_connection {
                            reconnect_delay = INITIAL_RECONNECT_DELAY;
                            last_reported_issue = None;
                        }
                        state.set_disconnect(
                            format!("OBS WebSocket 接続中にエラーが発生しました: {error}"),
                            false,
                        );
                        let disconnect = state.last_disconnect.take().expect("disconnect was set");
                        report_disconnect(
                            &status_tx,
                            &mut last_reported_issue,
                            &url,
                            &disconnect,
                            reconnect_delay,
                            state.was_identified,
                        );
                    }
                }
            }
            Ok(None) => break,
            Err(error) => {
                if report_connect_failure(
                    &status_tx,
                    &mut last_reported_issue,
                    &url,
                    &error,
                    reconnect_delay,
                    state.was_identified,
                ) {
                    wait_for_shutdown(&mut rx).await;
                    break;
                }
            }
        }

        let should_shutdown =
            wait_for_reconnect_or_shutdown(&mut rx, &mut pending_commands, reconnect_delay).await;
        if should_shutdown {
            break;
        }
        reconnect_delay = next_reconnect_delay(reconnect_delay);
    }
}

pub(super) async fn connect_with_pending_commands(
    url: &str,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
) -> std::result::Result<Option<ObsWebSocket>, Box<TungsteniteError>> {
    let connection = connect_async(url);
    tokio::pin!(connection);
    let timeout = tokio::time::sleep(LOAD_SCENES_TIMEOUT);
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => return Err(Box::new(TungsteniteError::Io(std::io::Error::new(ErrorKind::TimedOut, "OBS connect timeout")))),
            result = &mut connection => {
                return result.map(|(ws, _)| Some(ws)).map_err(Box::new);
            }
            command = rx.recv() => {
                match command {
                    Some(ObsCommand::Shutdown | ObsCommand::Close) | None => return Ok(None),
                    Some(command) => queue_offline_command(pending_commands, command),
                }
            }
        }
    }
}

pub(super) async fn run_connected(
    state: &mut ObsConnectionState,
    ws: ObsWebSocket,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    tx: mpsc::UnboundedSender<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
    status_tx: &watch::Sender<ObsConnectionStatus>,
) -> Result<ConnectionAction> {
    if state.config.rules.is_some() {
        return rules::run_rules_connected(state, ws, rx, tx, pending_commands, status_tx).await;
    }
    let reconnect = state.was_identified;
    let (mut sink, mut stream) = ws.split();
    match wait_for_identified(state, &mut sink, &mut stream, rx, pending_commands, status_tx)
        .await?
    {
        ConnectionAction::Continue => {}
        action => return Ok(action),
    }
    if reconnect {
        while let Some(command) = pending_commands.pop_front() {
            match command {
                ObsCommand::ApplyEvent(key) if key != ObsEventKey::PlayEnded => {
                    state.lifecycle.transition(key);
                }
                ObsCommand::SaveLastRecording(reason) => state.save_last_recording(reason),
                ObsCommand::Shutdown | ObsCommand::Close => return Ok(ConnectionAction::Shutdown),
                _ => {}
            }
        }
        if let Some(key) = state.lifecycle.scene {
            state.scene_request_generation = state.scene_request_generation.wrapping_add(1);
            state.apply_scene_for_event(&mut sink, key, 0, state.scene_request_generation).await?;
        }
    }
    match flush_pending_commands(state, &mut sink, tx.clone(), pending_commands).await? {
        ConnectionAction::Continue => {}
        action => {
            let _ = sink.send(Message::Close(None)).await;
            return Ok(action);
        }
    }
    loop {
        let deadline =
            [state.pending_stop_deadline, state.pending_restart.map(|(deadline, _)| deadline)]
                .into_iter()
                .flatten()
                .min();
        if let Some(deadline) = deadline {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    if state.pending_stop_deadline.is_some_and(|d| d <= TokioInstant::now()) {
                        state.flush_pending_stop(&mut sink).await?;
                    }
                    if state.pending_restart.is_some_and(|(d, _)| d <= TokioInstant::now())
                        && let Some((_, generation)) = state.pending_restart.take()
                        && generation == state.scene_request_generation
                        && state.lifecycle.scene == Some(ObsEventKey::Play) {
                        state.send_request(&mut sink, "StartRecord", None).await?;
                    }
                }
                command = rx.recv() => {
                    if matches!(handle_command(state, &mut sink, tx.clone(), command).await?, ConnectionAction::Shutdown) {
                        let _ = sink.send(Message::Close(None)).await;
                        return Ok(ConnectionAction::Shutdown);
                    }
                }
                message = stream.next() => {
                    match handle_stream_message(state, &mut sink, message, status_tx, tx.clone()).await? {
                        ConnectionAction::Continue => {}
                        action => return Ok(action),
                    }
                }
            }
        } else {
            tokio::select! {
                command = rx.recv() => {
                    if matches!(handle_command(state, &mut sink, tx.clone(), command).await?, ConnectionAction::Shutdown) {
                        let _ = sink.send(Message::Close(None)).await;
                        return Ok(ConnectionAction::Shutdown);
                    }
                }
                message = stream.next() => {
                    match handle_stream_message(state, &mut sink, message, status_tx, tx.clone()).await? {
                        ConnectionAction::Continue => {}
                        action => return Ok(action),
                    }
                }
            }
        }
    }
}

pub(super) async fn wait_for_identified(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    stream: &mut futures_util::stream::SplitStream<ObsWebSocket>,
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
    status_tx: &watch::Sender<ObsConnectionStatus>,
) -> Result<ConnectionAction> {
    let timeout = tokio::time::sleep(LOAD_SCENES_TIMEOUT);
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => bail!("OBS Identify timeout"),
            command = rx.recv() => {
                match command {
                    Some(ObsCommand::Shutdown | ObsCommand::Close) | None => return Ok(ConnectionAction::Shutdown),
                    Some(command) => queue_offline_command(pending_commands, command),
                }
            }
            message = stream.next() => {
                let Some(message) = message else {
                    state.set_disconnect("OBS WebSocket が識別前に切断されました。", false);
                    return Ok(ConnectionAction::Reconnect);
                };
                match message? {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(text.as_ref())
                            .context("failed to parse OBS message")?;
                        let data = value.get("d").unwrap_or(&Value::Null);
                        match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
                            0 => send_json(sink, identify_message(&state.config, data)).await?,
                            2 => {
                                state.mark_identified();
                                publish_status(
                                    status_tx,
                                    ObsConnectionStatus::new(
                                        ObsConnectionStatusKind::Connected,
                                        Some("OBS WebSocket に接続しました。".to_string()),
                                        None,
                                        None,
                                    ),
                                );
                                state.send_request(sink, "GetVersion", None).await?;
                                state.send_request(sink, "GetSceneList", None).await?;
                                state.send_request(sink, "GetRecordStatus", None).await?;
                                return Ok(ConnectionAction::Continue);
                            }
                            _ => {}
                        }
                    }
                    Message::Ping(payload) => sink.send(Message::Pong(payload)).await?,
                    Message::Close(close) => return Ok(connection_action_for_close(state, close)),
                    _ => {}
                }
            }
        }
    }
}

pub(super) async fn flush_pending_commands(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    tx: mpsc::UnboundedSender<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
) -> Result<ConnectionAction> {
    while let Some(command) = pending_commands.pop_front() {
        let action = handle_command(state, sink, tx.clone(), Some(command)).await?;
        if !matches!(action, ConnectionAction::Continue) {
            return Ok(action);
        }
    }
    Ok(ConnectionAction::Continue)
}

pub(super) async fn handle_command(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    tx: mpsc::UnboundedSender<ObsCommand>,
    command: Option<ObsCommand>,
) -> Result<ConnectionAction> {
    let Some(command) = command else {
        return Ok(ConnectionAction::Shutdown);
    };
    match command {
        ObsCommand::ApplyEvent(key) => {
            let events = state.lifecycle.transition(key);
            if key == ObsEventKey::PlayEnded && events.is_empty() {
                return Ok(ConnectionAction::Continue);
            }
            // Legacy mappings have only START and PLAY_ENDED. Other END events
            // are retained by the lifecycle model for the command executor.
            if key != ObsEventKey::PlayEnded && events.contains(&ObsTrigger::PlayEnd) {
                state.apply_event(sink, ObsEventKey::PlayEnded).await?;
            }
            state.apply_event(sink, key).await?;
        }
        ObsCommand::RetryScene { key, retry_count, generation } => {
            state.retry_scene(sink, key, retry_count, generation).await?
        }
        ObsCommand::RetryPlay => {
            state.lifecycle.retry();
            state.retry_play(sink, tx).await?;
        }
        ObsCommand::RetryPlayStart { generation } => {
            if generation == state.scene_request_generation
                && state.lifecycle.scene == Some(ObsEventKey::Play)
            {
                state.apply_event(sink, ObsEventKey::Play).await?;
            }
        }
        ObsCommand::SaveLastRecording(reason) => state.save_last_recording(reason),
        ObsCommand::Shutdown | ObsCommand::Close => return Ok(ConnectionAction::Shutdown),
    }
    Ok(ConnectionAction::Continue)
}

pub(super) async fn handle_stream_message(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    message: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>,
    status_tx: &watch::Sender<ObsConnectionStatus>,
    tx: mpsc::UnboundedSender<ObsCommand>,
) -> Result<ConnectionAction> {
    let Some(message) = message else {
        state.set_disconnect("OBS WebSocket が切断されました。", false);
        return Ok(ConnectionAction::Reconnect);
    };
    match message? {
        Message::Text(text) => handle_text_message(state, sink, text.as_ref(), status_tx, tx).await,
        Message::Ping(payload) => {
            sink.send(Message::Pong(payload)).await?;
            Ok(ConnectionAction::Continue)
        }
        Message::Close(close) => Ok(connection_action_for_close(state, close)),
        _ => Ok(ConnectionAction::Continue),
    }
}

pub(super) async fn handle_text_message(
    state: &mut ObsConnectionState,
    sink: &mut ObsSink,
    text: &str,
    status_tx: &watch::Sender<ObsConnectionStatus>,
    tx: mpsc::UnboundedSender<ObsCommand>,
) -> Result<ConnectionAction> {
    let value: Value = serde_json::from_str(text).context("failed to parse OBS message")?;
    let data = value.get("d").unwrap_or(&Value::Null);
    match value.get("op").and_then(Value::as_i64).unwrap_or(-1) {
        0 => {
            send_json(sink, identify_message(&state.config, data)).await?;
            Ok(ConnectionAction::Continue)
        }
        2 => {
            state.mark_identified();
            publish_status(
                status_tx,
                ObsConnectionStatus::new(
                    ObsConnectionStatusKind::Connected,
                    Some("OBS WebSocket に接続しました。".to_string()),
                    None,
                    None,
                ),
            );
            state.send_request(sink, "GetVersion", None).await?;
            state.send_request(sink, "GetSceneList", None).await?;
            state.send_request(sink, "GetRecordStatus", None).await?;
            Ok(ConnectionAction::Continue)
        }
        5 => state.handle_event(sink, data).await,
        7 => {
            state.handle_response(sink, data, status_tx, tx).await?;
            Ok(ConnectionAction::Continue)
        }
        _ => Ok(ConnectionAction::Continue),
    }
}

pub(super) async fn wait_for_reconnect_or_shutdown(
    rx: &mut mpsc::UnboundedReceiver<ObsCommand>,
    pending_commands: &mut VecDeque<ObsCommand>,
    delay: Duration,
) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);
    loop {
        tokio::select! {
            _ = &mut sleep => return false,
            command = rx.recv() => {
                match command {
                    Some(ObsCommand::Shutdown | ObsCommand::Close) | None => return true,
                    Some(command) => queue_offline_command(pending_commands, command),
                }
            }
        }
    }
}

fn queue_offline_command(pending: &mut VecDeque<ObsCommand>, command: ObsCommand) {
    match command {
        ObsCommand::ApplyEvent(key) if key != ObsEventKey::PlayEnded => {
            pending.retain(|c| !matches!(c, ObsCommand::ApplyEvent(_)));
            pending.push_back(ObsCommand::ApplyEvent(key));
        }
        ObsCommand::SaveLastRecording(reason) => {
            pending.retain(
                |c| !matches!(c, ObsCommand::SaveLastRecording(existing) if *existing == reason),
            );
            pending.push_back(ObsCommand::SaveLastRecording(reason));
        }
        _ => {}
    }
}
