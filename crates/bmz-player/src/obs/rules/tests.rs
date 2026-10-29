use super::*;
use crate::config::app_config::ObsRule;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

async fn sockets() -> Result<(ObsSink, WebSocketStream<TcpStream>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("ws://{}", listener.local_addr()?);
    let (client, server) = tokio::join!(connect_async(url), async {
        let (tcp, _) = tokio::time::timeout(Duration::from_secs(4), listener.accept()).await??;
        Ok::<_, anyhow::Error>(accept_async(tcp).await?)
    });
    Ok((client?.0.split().0, server?))
}

async fn request(server: &mut WebSocketStream<TcpStream>) -> Result<Value> {
    let message = tokio::time::timeout(Duration::from_secs(1), server.next())
        .await?
        .context("request missing")??;
    Ok(serde_json::from_str(message.to_text()?)?)
}

fn response(request: &Value, success: bool, data: Value) -> Value {
    json!({ "requestId": request["d"]["requestId"], "requestType": request["d"]["requestType"],
        "requestStatus": { "result": success, "code": if success {100} else {600} }, "responseData": data })
}

fn config(actions: Vec<ObsRuleAction>) -> ObsConfig {
    ObsConfig {
        record_stop_wait_ms: 0,
        rules: Some(
            actions
                .into_iter()
                .map(|action| ObsRule { trigger: ObsTrigger::PlayStart, action })
                .collect(),
        ),
        ..ObsConfig::default()
    }
}

#[tokio::test]
async fn commands_wait_for_matching_responses_and_resolve_sources_in_each_scene() -> Result<()> {
    let config = config(vec![
        ObsRuleAction::SwitchScene { scene_name: "Gameplay".into() },
        ObsRuleAction::ShowSource { scene_name: "Overlay".into(), source_name: "Same".into() },
        ObsRuleAction::HideSource { scene_name: "Other".into(), source_name: "Same".into() },
        ObsRuleAction::StartRecord,
        ObsRuleAction::StopRecord,
    ]);
    let mut state = ObsConnectionState::new(config.clone());
    let mut queue = RuleQueue::default();
    queue.enqueue(&config, &[ObsTrigger::PlayStart], false);
    let (mut sink, mut server) = sockets().await?;
    let (status, _) = watch::channel(ObsConnectionStatus::default());
    for (kind, id) in [
        ("SetCurrentProgramScene", 0),
        ("GetSceneItemId", 11),
        ("SetSceneItemEnabled", 0),
        ("GetSceneItemId", 22),
        ("SetSceneItemEnabled", 0),
        ("StartRecord", 0),
        ("StopRecord", 0),
    ] {
        queue.pump(&mut state, &mut sink).await?;
        let req = request(&mut server).await?;
        assert_eq!(req["d"]["requestType"], kind);
        if kind == "SetSceneItemEnabled" {
            let scene = req["d"]["requestData"]["sceneName"].as_str().unwrap();
            assert_eq!(
                req["d"]["requestData"]["sceneItemId"],
                if scene == "Overlay" { 11 } else { 22 }
            );
            assert_eq!(req["d"]["requestData"]["sceneItemEnabled"], scene == "Overlay");
        }
        let mut wrong = response(&req, true, json!({}));
        wrong["requestId"] = json!("old-response");
        queue.response(&mut state, &mut sink, &wrong, &status).await?;
        queue.pump(&mut state, &mut sink).await?;
        assert!(tokio::time::timeout(Duration::from_millis(5), server.next()).await.is_err());
        queue
            .response(
                &mut state,
                &mut sink,
                &response(&req, true, json!({"sceneItemId": id})),
                &status,
            )
            .await?;
    }
    assert!(queue.pending.is_none());
    assert!(queue.queue.is_empty());
    Ok(())
}

#[tokio::test]
async fn late_lookup_cannot_change_new_screen_and_failure_continues_commands() -> Result<()> {
    let config = config(vec![
        ObsRuleAction::ShowSource { scene_name: "Overlay".into(), source_name: "Old".into() },
        ObsRuleAction::StartRecord,
    ]);
    let mut state = ObsConnectionState::new(config.clone());
    let mut queue = RuleQueue::default();
    queue.enqueue(&config, &[ObsTrigger::PlayStart], false);
    let (mut sink, mut server) = sockets().await?;
    let (status, status_rx) = watch::channel(ObsConnectionStatus::default());
    queue.pump(&mut state, &mut sink).await?;
    let lookup = request(&mut server).await?;
    queue.advance();
    queue
        .response(
            &mut state,
            &mut sink,
            &response(&lookup, true, json!({"sceneItemId": 99})),
            &status,
        )
        .await?;
    queue.pump(&mut state, &mut sink).await?;
    let start = request(&mut server).await?;
    assert_eq!(start["d"]["requestType"], "StartRecord");
    queue.response(&mut state, &mut sink, &response(&start, false, json!({})), &status).await?;
    assert!(status_rx.borrow().last_error.is_some());
    assert!(queue.pending.is_none());
    Ok(())
}

#[tokio::test]
async fn missing_sources_and_deleted_items_do_not_block_independent_commands() -> Result<()> {
    for failed_kind in ["GetSceneItemId", "SetSceneItemEnabled", "SetCurrentProgramScene"] {
        let action = if failed_kind == "SetCurrentProgramScene" {
            ObsRuleAction::SwitchScene { scene_name: "Missing".into() }
        } else {
            ObsRuleAction::HideSource { scene_name: "Scene".into(), source_name: "Deleted".into() }
        };
        let cfg = config(vec![action, ObsRuleAction::StartRecord]);
        let mut state = ObsConnectionState::new(cfg.clone());
        let mut queue = RuleQueue::default();
        let (mut sink, mut server) = sockets().await?;
        let (status, _) = watch::channel(ObsConnectionStatus::default());
        queue.enqueue(&cfg, &[ObsTrigger::PlayStart], false);
        queue.pump(&mut state, &mut sink).await?;
        let mut req = request(&mut server).await?;
        if failed_kind == "SetSceneItemEnabled" {
            queue
                .response(
                    &mut state,
                    &mut sink,
                    &response(&req, true, json!({"sceneItemId": 5})),
                    &status,
                )
                .await?;
            req = request(&mut server).await?;
        }
        assert_eq!(req["d"]["requestType"], failed_kind);
        queue.response(&mut state, &mut sink, &response(&req, false, json!({})), &status).await?;
        queue.pump(&mut state, &mut sink).await?;
        assert_eq!(request(&mut server).await?["d"]["requestType"], "StartRecord");
    }
    Ok(())
}

#[tokio::test]
async fn scene_not_ready_retries_are_bounded_and_new_generations_cancel_retries() -> Result<()> {
    let cfg = config(vec![
        ObsRuleAction::SwitchScene { scene_name: "Scene".into() },
        ObsRuleAction::StartRecord,
    ]);
    let mut state = ObsConnectionState::new(cfg.clone());
    let mut queue = RuleQueue::default();
    queue.enqueue(&cfg, &[ObsTrigger::PlayStart], false);
    let (mut sink, mut server) = sockets().await?;
    let (status, _) = watch::channel(ObsConnectionStatus::default());
    for _ in 0..=MAX_SCENE_NOT_READY_RETRIES {
        queue.pump(&mut state, &mut sink).await?;
        let req = request(&mut server).await?;
        assert_eq!(req["d"]["requestType"], "SetCurrentProgramScene");
        let mut not_ready = response(&req, false, json!({}));
        not_ready["requestStatus"]["code"] = json!(207);
        queue.response(&mut state, &mut sink, &not_ready, &status).await?;
        if let Some(next) = queue.queue.front_mut() {
            next.ready_at = TokioInstant::now();
        }
    }
    queue.pump(&mut state, &mut sink).await?;
    assert_eq!(request(&mut server).await?["d"]["requestType"], "StartRecord");
    queue.advance();
    assert!(queue.queue.iter().all(|rule| !rule.action.is_visual()));
    Ok(())
}

#[tokio::test]
async fn delayed_stop_waits_and_is_flushed_before_following_start() -> Result<()> {
    let mut cfg = config(vec![ObsRuleAction::StopRecord]);
    cfg.record_stop_wait_ms = 10_000;
    let mut state = ObsConnectionState::new(cfg.clone());
    let mut queue = RuleQueue::default();
    queue.enqueue(&cfg, &[ObsTrigger::PlayStart], false);
    let (mut sink, mut server) = sockets().await?;
    let (status, _) = watch::channel(ObsConnectionStatus::default());
    queue.pump(&mut state, &mut sink).await?;
    assert!(state.pending_stop_deadline.is_some());
    assert!(tokio::time::timeout(Duration::from_millis(10), server.next()).await.is_err());
    let cfg = config(vec![ObsRuleAction::StartRecord]);
    queue.enqueue(&cfg, &[ObsTrigger::PlayStart], false);
    queue.pump(&mut state, &mut sink).await?;
    let stop = request(&mut server).await?;
    assert_eq!(stop["d"]["requestType"], "StopRecord");
    queue.response(&mut state, &mut sink, &response(&stop, true, json!({})), &status).await?;
    queue.pump(&mut state, &mut sink).await?;
    assert_eq!(request(&mut server).await?["d"]["requestType"], "StartRecord");
    Ok(())
}

#[tokio::test]
async fn recording_retention_keeps_only_requested_modes_and_defers_deletion_until_next_start()
-> Result<()> {
    for mode in
        [ObsRecordingMode::KeepAll, ObsRecordingMode::OnScreenshot, ObsRecordingMode::OnReplay]
    {
        for reason in [
            None,
            Some(ObsRecordingSaveReason::OnScreenshot),
            Some(ObsRecordingSaveReason::OnReplay),
        ] {
            let temp = tempfile::tempdir()?;
            let path = temp.path().join("recording.mkv");
            std::fs::write(&path, b"mock recording")?;
            let mut cfg = config(vec![]);
            cfg.recording_mode = mode;
            let mut state = ObsConnectionState::new(cfg);
            let (mut sink, _) = sockets().await?;
            state
                .handle_record_state_changed(
                    &mut sink,
                    &json!({"outputState": "OBS_WEBSOCKET_OUTPUT_STOPPED", "outputPath": path}),
                )
                .await?;
            assert!(path.is_file(), "stopping retains the file for the Result decision");
            if let Some(reason) = reason {
                state.save_last_recording(reason);
            }
            state
                .handle_record_state_changed(
                    &mut sink,
                    &json!({"outputState": "OBS_WEBSOCKET_OUTPUT_STARTED"}),
                )
                .await?;
            let keep = mode == ObsRecordingMode::KeepAll
                || reason.is_some_and(|r| recording_mode_matches(mode, r));
            assert_eq!(path.is_file(), keep, "{mode:?}, {reason:?}");
            assert!(!state.save_requested);
        }
    }
    Ok(())
}

#[tokio::test]
async fn retry_discards_interrupted_recording_then_restarts_once_after_delay() -> Result<()> {
    for mode in
        [ObsRecordingMode::KeepAll, ObsRecordingMode::OnScreenshot, ObsRecordingMode::OnReplay]
    {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("retry.mkv");
        std::fs::write(&path, b"retry")?;
        let mut cfg = config(vec![ObsRuleAction::StartRecord]);
        cfg.recording_mode = mode;
        let mut state = ObsConnectionState::new(cfg);
        state.is_recording = true;
        let mut queue = RuleQueue { retry: Some(RetryRecording::DrainEnd), ..RuleQueue::default() };
        let (mut sink, mut server) = sockets().await?;
        let (status, _) = watch::channel(ObsConnectionStatus::default());
        queue.pump(&mut state, &mut sink).await?;
        let stop = request(&mut server).await?;
        assert_eq!(stop["d"]["requestType"], "StopRecord");
        queue
            .response(
                &mut state,
                &mut sink,
                &response(&stop, true, json!({"outputPath": path})),
                &status,
            )
            .await?;
        assert_eq!(path.is_file(), mode == ObsRecordingMode::KeepAll);
        queue.pump(&mut state, &mut sink).await?;
        assert!(tokio::time::timeout(Duration::from_millis(10), server.next()).await.is_err());
        queue.retry = Some(RetryRecording::RestartAt(TokioInstant::now()));
        queue.pump(&mut state, &mut sink).await?;
        let start = request(&mut server).await?;
        assert_eq!(start["d"]["requestType"], "StartRecord");
        queue.response(&mut state, &mut sink, &response(&start, true, json!({})), &status).await?;
        queue.pump(&mut state, &mut sink).await?;
        assert!(queue.pending.is_none());
        assert!(queue.queue.is_empty());
        assert!(tokio::time::timeout(Duration::from_millis(10), server.next()).await.is_err());
    }
    Ok(())
}
