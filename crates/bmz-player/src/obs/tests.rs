use super::*;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

#[test]
fn obs_authentication_matches_obs_websocket_v5_algorithm() {
    let auth = obs_authentication("pass", "salt", "challenge");

    assert_eq!(auth, "EabUNw4z9EKKpEOC0yvqBO8dJPSIcTb82eo+adWKOvk=");
}

#[test]
fn request_message_omits_empty_request_data() {
    let message = request_message("GetVersion", "GetVersion-1", None);

    assert_eq!(message["op"], 6);
    assert_eq!(message["d"]["requestType"], "GetVersion");
    assert_eq!(message["d"]["requestId"], "GetVersion-1");
    assert!(message["d"].get("requestData").is_none());
}

#[test]
fn parse_scene_names_matches_lr2oraja_order() {
    let names = parse_scene_names(&json!({
        "scenes": [
            { "sceneName": "Top" },
            { "sceneName": "Play" },
            { "sceneName": "Result" }
        ]
    }));

    assert_eq!(names, ["Result", "Play", "Top"]);
}

#[test]
fn recording_mode_filters_save_reasons() {
    assert!(recording_mode_matches(
        ObsRecordingMode::OnScreenshot,
        ObsRecordingSaveReason::OnScreenshot
    ));
    assert!(recording_mode_matches(ObsRecordingMode::OnReplay, ObsRecordingSaveReason::OnReplay));
    assert!(!recording_mode_matches(
        ObsRecordingMode::OnReplay,
        ObsRecordingSaveReason::OnScreenshot
    ));
    assert!(!recording_mode_matches(ObsRecordingMode::KeepAll, ObsRecordingSaveReason::OnReplay));
}

#[test]
fn obs_ws_url_accepts_plain_host_or_full_url() {
    let mut config = ObsConfig::default();
    assert_eq!(obs_ws_url(&config), "ws://localhost:4455");
    config.host = "192.0.2.1".to_string();
    config.port = 4456;
    assert_eq!(obs_ws_url(&config), "ws://192.0.2.1:4456");
    config.host = "ws://example.test:4455".to_string();
    assert_eq!(obs_ws_url(&config), "ws://example.test:4455");
}

#[test]
fn connection_refused_is_treated_as_waiting_for_obs() {
    let error = TungsteniteError::Io(std::io::Error::new(
        ErrorKind::ConnectionRefused,
        "OBS is not running",
    ));
    let (status_tx, status_rx) = watch::channel(ObsConnectionStatus::default());
    let mut last_reported_issue = None;

    assert_eq!(classify_connection_failure(&error), ObsConnectionFailureKind::ServerUnavailable);
    assert!(!report_connect_failure(
        &status_tx,
        &mut last_reported_issue,
        "ws://localhost:4455",
        &error,
        INITIAL_RECONNECT_DELAY,
        false,
    ));
    assert_eq!(status_rx.borrow().kind, ObsConnectionStatusKind::WaitingForServer);
    assert!(status_rx.borrow().last_error.is_none());
}

#[test]
fn authentication_close_pauses_reconnects() {
    let mut state = ObsConnectionState::new(ObsConfig::default());
    let action = connection_action_for_close(
        &mut state,
        Some(CloseFrame { code: CloseCode::Library(4009), reason: "authentication failed".into() }),
    );

    assert!(matches!(
        action,
        ConnectionAction::Pause { kind: ObsConnectionStatusKind::AuthenticationFailed, .. }
    ));
}

#[tokio::test]
async fn pending_events_wait_for_identified_before_sending_requests() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let ws = accept_async(stream).await?;
        let (mut sink, mut stream) = ws.split();

        sink.send(Message::Text(json!({ "op": 0, "d": {} }).to_string())).await?;
        let identify = stream.next().await.context("OBS client closed before Identify")??;
        let Message::Text(identify) = identify else {
            bail!("expected Identify text message");
        };
        let identify: Value = serde_json::from_str(identify.as_ref())?;
        assert_eq!(identify["op"], 1);

        sink.send(Message::Text(json!({ "op": 2, "d": {} }).to_string())).await?;
        let mut requests = Vec::new();
        for _ in 0..5 {
            let message = stream.next().await.context("OBS client closed before request")??;
            let Message::Text(message) = message else {
                bail!("expected OBS request text message");
            };
            let message: Value = serde_json::from_str(message.as_ref())?;
            requests.push(
                message["d"]["requestType"]
                    .as_str()
                    .context("OBS request type missing")?
                    .to_string(),
            );
        }
        assert_eq!(
            requests,
            [
                "GetVersion",
                "GetSceneList",
                "GetRecordStatus",
                "GetRecordDirectory",
                "SetCurrentProgramScene"
            ]
        );

        sink.send(Message::Close(None)).await?;
        Ok::<(), anyhow::Error>(())
    });

    let (client, _) = connect_async(format!("ws://{address}")).await?;
    let mut config = ObsConfig::default();
    config.scenes.insert(ObsEventKey::MusicSelect.config_key().to_string(), "Select".to_string());
    let mut state = ObsConnectionState::new(config);
    let (tx, mut rx) = mpsc::unbounded_channel();
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::MusicSelect))?;
    let (status_tx, _) = watch::channel(ObsConnectionStatus::default());

    assert_eq!(
        run_connected(&mut state, client, &mut rx, tx, &status_tx).await?,
        ConnectionAction::Reconnect
    );
    server.await??;
    Ok(())
}

#[tokio::test]
async fn scene_request_retries_when_obs_is_temporarily_not_ready() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let ws = accept_async(stream).await?;
        let (mut sink, mut stream) = ws.split();

        sink.send(Message::Text(json!({ "op": 0, "d": {} }).to_string())).await?;
        let identify = stream.next().await.context("OBS client closed before Identify")??;
        assert!(matches!(identify, Message::Text(_)));

        sink.send(Message::Text(json!({ "op": 2, "d": {} }).to_string())).await?;
        let mut scene_request_id = None;
        for _ in 0..5 {
            let message = stream.next().await.context("OBS client closed before request")??;
            let Message::Text(message) = message else {
                bail!("expected OBS request text message");
            };
            let message: Value = serde_json::from_str(message.as_ref())?;
            if message["d"]["requestType"] == "SetCurrentProgramScene" {
                scene_request_id = message["d"]["requestId"].as_str().map(ToOwned::to_owned);
            }
        }
        let scene_request_id = scene_request_id.context("scene request was not sent")?;
        sink.send(Message::Text(
            json!({
                "op": 7,
                "d": {
                    "requestType": "SetCurrentProgramScene",
                    "requestId": scene_request_id,
                    "requestStatus": {
                        "result": false,
                        "code": 207,
                        "comment": "OBS is not ready to perform the request."
                    }
                }
            })
            .to_string(),
        ))
        .await?;

        let retry = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .context("scene retry timed out")?
            .context("OBS client closed before scene retry")??;
        let Message::Text(retry) = retry else {
            bail!("expected retried scene request text message");
        };
        let retry: Value = serde_json::from_str(retry.as_ref())?;
        assert_eq!(retry["d"]["requestType"], "SetCurrentProgramScene");
        assert_ne!(retry["d"]["requestId"], scene_request_id);

        sink.send(Message::Text(
            json!({
                "op": 7,
                "d": {
                    "requestType": "SetCurrentProgramScene",
                    "requestId": retry["d"]["requestId"],
                    "requestStatus": { "result": true, "code": 100 }
                }
            })
            .to_string(),
        ))
        .await?;
        sink.send(Message::Close(None)).await?;
        Ok::<(), anyhow::Error>(())
    });

    let (client, _) = connect_async(format!("ws://{address}")).await?;
    let mut config = ObsConfig::default();
    config.scenes.insert(ObsEventKey::MusicSelect.config_key().to_string(), "Select".to_string());
    let mut state = ObsConnectionState::new(config);
    let (tx, mut rx) = mpsc::unbounded_channel();
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::MusicSelect))?;
    let (status_tx, status_rx) = watch::channel(ObsConnectionStatus::default());

    assert_eq!(
        run_connected(&mut state, client, &mut rx, tx, &status_tx).await?,
        ConnectionAction::Reconnect
    );
    server.await??;
    assert_eq!(status_rx.borrow().kind, ObsConnectionStatusKind::Connected);
    assert!(status_rx.borrow().last_error.is_none());
    Ok(())
}

#[tokio::test]
async fn reconnect_wait_observes_play_end_before_shutdown() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::Play)).unwrap();
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::PlayEnded)).unwrap();
    tx.send(ObsCommand::Shutdown).unwrap();
    let mut state = ObsConnectionState::new(ObsConfig::default());

    let shutdown =
        wait_for_reconnect_or_shutdown(&mut rx, &mut state, Duration::from_secs(1)).await;

    assert!(shutdown);
    assert_eq!(state.lifecycle.scene, Some(ObsEventKey::Play));
    assert!(state.lifecycle.has_ended_play());
}
