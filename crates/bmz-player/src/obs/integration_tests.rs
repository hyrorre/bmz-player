use super::*;
use crate::config::app_config::{ObsRule, ObsRuleAction};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

type MockSocket = WebSocketStream<TcpStream>;
type Worker = tokio::task::JoinHandle<Result<(ObsConnectionState, ConnectionAction)>>;

async fn request(server: &mut MockSocket) -> Result<Value> {
    let message = tokio::time::timeout(Duration::from_secs(4), server.next())
        .await?
        .context("mock request missing")??;
    let value: Value = serde_json::from_str(message.to_text()?)?;
    assert_eq!(value["op"], 6);
    Ok(value)
}

async fn respond(server: &mut MockSocket, request: &Value, data: Value) -> Result<()> {
    server
        .send(Message::Text(
            json!({ "op": 7, "d": {
                "requestId": request["d"]["requestId"], "requestType": request["d"]["requestType"],
                "requestStatus": { "result": true, "code": 100 }, "responseData": data,
            }})
            .to_string(),
        ))
        .await?;
    Ok(())
}

async fn start(
    mut state: ObsConnectionState,
    mut pending: VecDeque<ObsCommand>,
    recording: bool,
) -> Result<(mpsc::UnboundedSender<ObsCommand>, Worker, MockSocket)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("ws://{}", listener.local_addr()?);
    let (client, server) = tokio::join!(connect_async(url), async {
        let (tcp, _) = tokio::time::timeout(Duration::from_secs(4), listener.accept()).await??;
        Ok::<_, anyhow::Error>(accept_async(tcp).await?)
    });
    let client = client?.0;
    let mut server = server?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let worker_tx = tx.clone();
    let worker = tokio::spawn(async move {
        let (status, _) = watch::channel(ObsConnectionStatus::default());
        state.begin_connection();
        let action =
            run_connected(&mut state, client, &mut rx, worker_tx, &mut pending, &status).await?;
        Ok((state, action))
    });
    server.send(Message::Text(json!({ "op": 0, "d": {} }).to_string())).await?;
    let identify = server.next().await.context("Identify missing")??;
    assert_eq!(serde_json::from_str::<Value>(identify.to_text()?)?["op"], 1);
    server.send(Message::Text(json!({ "op": 2, "d": {} }).to_string())).await?;
    for _ in 0..3 {
        let req = request(&mut server).await?;
        let data = match req["d"]["requestType"].as_str().unwrap() {
            "GetVersion" => json!({ "obsVersion": "mock", "obsWebSocketVersion": "5" }),
            "GetSceneList" => json!({ "scenes": [] }),
            "GetRecordStatus" => json!({ "outputActive": recording }),
            other => bail!("unexpected initial request: {other}"),
        };
        respond(&mut server, &req, data).await?;
    }
    Ok((tx, worker, server))
}

fn config(rules: Vec<ObsRule>) -> ObsConfig {
    ObsConfig { rules: Some(rules), record_stop_wait_ms: 0, ..ObsConfig::default() }
}

fn scene_rule(trigger: ObsTrigger) -> ObsRule {
    ObsRule { trigger, action: ObsRuleAction::SwitchScene { scene_name: format!("{trigger:?}") } }
}

async fn expect_scene(server: &mut MockSocket, trigger: ObsTrigger) -> Result<()> {
    let req = request(server).await?;
    assert_eq!(req["d"]["requestType"], "SetCurrentProgramScene");
    assert_eq!(req["d"]["requestData"]["sceneName"], format!("{trigger:?}"));
    respond(server, &req, json!({})).await
}

#[tokio::test]
async fn worker_emits_main_and_screen_lifecycle_in_order_without_duplicate_play_end() -> Result<()>
{
    let state =
        ObsConnectionState::new(config(ObsTrigger::ALL.into_iter().map(scene_rule).collect()));
    let (tx, worker, mut server) = start(state, VecDeque::new(), false).await?;
    expect_scene(&mut server, ObsTrigger::MainStart).await?;
    for (scene, expected) in [
        (ObsEventKey::MusicSelect, vec![ObsTrigger::MusicSelectStart]),
        (ObsEventKey::Decide, vec![ObsTrigger::MusicSelectEnd, ObsTrigger::DecideStart]),
        (ObsEventKey::Play, vec![ObsTrigger::DecideEnd, ObsTrigger::PlayStart]),
        (ObsEventKey::PlayEnded, vec![ObsTrigger::PlayEnd]),
        (ObsEventKey::Result, vec![ObsTrigger::ResultStart]),
        (ObsEventKey::CourseResult, vec![ObsTrigger::ResultEnd, ObsTrigger::CourseResultStart]),
    ] {
        tx.send(ObsCommand::ApplyEvent(scene))?;
        for trigger in expected {
            expect_scene(&mut server, trigger).await?;
        }
        if scene == ObsEventKey::PlayEnded {
            tx.send(ObsCommand::ApplyEvent(scene))?;
        }
    }
    tx.send(ObsCommand::Shutdown)?;
    expect_scene(&mut server, ObsTrigger::CourseResultEnd).await?;
    expect_scene(&mut server, ObsTrigger::MainEnd).await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), worker).await???.1,
        ConnectionAction::Shutdown
    );
    Ok(())
}

#[tokio::test]
async fn reconnect_syncs_only_current_visual_rules_without_replaying_startup_or_recording()
-> Result<()> {
    let cfg = config(vec![
        scene_rule(ObsTrigger::MainStart),
        ObsRule { trigger: ObsTrigger::PlayStart, action: ObsRuleAction::StartRecord },
        scene_rule(ObsTrigger::ResultStart),
        ObsRule { trigger: ObsTrigger::ResultStart, action: ObsRuleAction::StartRecord },
    ]);
    let pending = VecDeque::from([ObsCommand::ApplyEvent(ObsEventKey::Play)]);
    let (_, worker, mut server) = start(ObsConnectionState::new(cfg), pending, false).await?;
    expect_scene(&mut server, ObsTrigger::MainStart).await?;
    let req = request(&mut server).await?;
    assert_eq!(req["d"]["requestType"], "StartRecord");
    respond(&mut server, &req, json!({})).await?;
    server.send(Message::Close(None)).await?;
    let (state, action) = worker.await??;
    assert_eq!(action, ConnectionAction::Reconnect);
    let pending = VecDeque::from([
        ObsCommand::ApplyEvent(ObsEventKey::Decide),
        ObsCommand::RetryPlay,
        ObsCommand::ApplyEvent(ObsEventKey::Result),
    ]);
    let (tx, worker, mut server) = start(state, pending, true).await?;
    expect_scene(&mut server, ObsTrigger::ResultStart).await?;
    assert!(tokio::time::timeout(Duration::from_millis(25), server.next()).await.is_err());
    tx.send(ObsCommand::Close)?;
    assert_eq!(worker.await??.1, ConnectionAction::Shutdown);
    Ok(())
}

#[tokio::test]
async fn timed_out_source_lookup_does_not_apply_late_visibility_and_next_command_runs() -> Result<()>
{
    let cfg = config(vec![
        ObsRule {
            trigger: ObsTrigger::MainStart,
            action: ObsRuleAction::ShowSource {
                scene_name: "Overlay".into(),
                source_name: "Source".into(),
            },
        },
        ObsRule { trigger: ObsTrigger::MainStart, action: ObsRuleAction::StartRecord },
    ]);
    let (tx, worker, mut server) =
        start(ObsConnectionState::new(cfg), VecDeque::new(), false).await?;
    let lookup = request(&mut server).await?;
    assert_eq!(lookup["d"]["requestType"], "GetSceneItemId");
    let next = request(&mut server).await?;
    assert_eq!(next["d"]["requestType"], "StartRecord");
    respond(&mut server, &lookup, json!({ "sceneItemId": 123 })).await?;
    respond(&mut server, &next, json!({})).await?;
    assert!(tokio::time::timeout(Duration::from_millis(25), server.next()).await.is_err());
    tx.send(ObsCommand::Close)?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn main_end_is_sent_and_shutdown_is_bounded_when_obs_never_responds() -> Result<()> {
    let cfg =
        config(vec![ObsRule { trigger: ObsTrigger::MainEnd, action: ObsRuleAction::StopRecord }]);
    let (tx, worker, mut server) =
        start(ObsConnectionState::new(cfg), VecDeque::new(), true).await?;
    tx.send(ObsCommand::Shutdown)?;
    let req = request(&mut server).await?;
    assert_eq!(req["d"]["requestType"], "StopRecord");
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), worker).await???.1,
        ConnectionAction::Shutdown
    );
    Ok(())
}

#[tokio::test]
async fn scene_picker_loads_scene_scoped_sources_and_ignores_unknown_response_ids() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let cfg =
        ObsConfig { host: format!("ws://{}", listener.local_addr()?), ..ObsConfig::default() };
    let task = tokio::spawn(load_scenes(cfg));
    let (tcp, _) = tokio::time::timeout(Duration::from_secs(4), listener.accept()).await??;
    let mut server = accept_async(tcp).await?;
    server.send(Message::Text(json!({"op": 0, "d": {}}).to_string())).await?;
    server.next().await.context("Identify missing")??;
    server.send(Message::Text(json!({"op": 2, "d": {}}).to_string())).await?;
    for _ in 0..5 {
        let req = request(&mut server).await?;
        let data = match req["d"]["requestType"].as_str().unwrap() {
            "GetVersion" => json!({"obsVersion": "mock"}),
            "GetSceneList" => json!({"scenes": [{"sceneName": "A"}, {"sceneName": "B"}]}),
            "GetRecordStatus" => json!({"outputActive": false}),
            "GetSceneItemList" => {
                json!({"sceneItems": [{"sourceName": "Same"}, {"sourceName": "Same"}, {"sourceName": req["d"]["requestData"]["sceneName"]}]})
            }
            other => bail!("unexpected request {other}"),
        };
        let mut stale = req.clone();
        stale["d"]["requestId"] = json!("unknown");
        respond(&mut server, &stale, json!({})).await?;
        respond(&mut server, &req, data).await?;
    }
    let list = task.await??;
    assert_eq!(list.scenes, ["B", "A"]);
    assert_eq!(list.sources["A"], ["Same", "A"]);
    assert_eq!(list.sources["B"], ["Same", "B"]);
    Ok(())
}

#[tokio::test]
async fn legacy_delayed_retry_start_cannot_override_a_new_result_screen() -> Result<()> {
    let mut cfg = ObsConfig::default();
    cfg.scenes.insert("MUSICSELECT".into(), "Select".into());
    cfg.scenes.insert("PLAY".into(), "Play".into());
    cfg.scenes.insert("RESULT".into(), "Result".into());
    let mut state = ObsConnectionState::new(cfg);
    state.lifecycle.transition(ObsEventKey::Play);
    let (tx, worker, mut server) = start(state, VecDeque::new(), false).await?;
    tx.send(ObsCommand::RetryPlay)?;
    let select = request(&mut server).await?;
    assert_eq!(select["d"]["requestData"]["sceneName"], "Select");
    respond(&mut server, &select, json!({})).await?;
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::Result))?;
    let result = request(&mut server).await?;
    assert_eq!(result["d"]["requestData"]["sceneName"], "Result");
    respond(&mut server, &result, json!({})).await?;
    assert!(tokio::time::timeout(Duration::from_millis(1200), server.next()).await.is_err());
    tx.send(ObsCommand::Close)?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn transition_during_retry_stop_cancels_delayed_recording_restart() -> Result<()> {
    let mut state = ObsConnectionState::new(config(vec![
        ObsRule { trigger: ObsTrigger::PlayStart, action: ObsRuleAction::StartRecord },
        scene_rule(ObsTrigger::ResultStart),
    ]));
    state.main_started = true;
    state.lifecycle.transition(ObsEventKey::Play);
    let (tx, worker, mut server) = start(state, VecDeque::new(), true).await?;
    tx.send(ObsCommand::RetryPlay)?;
    let stop = request(&mut server).await?;
    assert_eq!(stop["d"]["requestType"], "StopRecord");
    respond(&mut server, &stop, json!({})).await?;
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::Result))?;
    expect_scene(&mut server, ObsTrigger::ResultStart).await?;
    assert!(tokio::time::timeout(Duration::from_millis(600), server.next()).await.is_err());
    tx.send(ObsCommand::Close)?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn transition_without_commands_flushes_a_delayed_stop() -> Result<()> {
    let mut cfg =
        config(vec![ObsRule { trigger: ObsTrigger::PlayEnd, action: ObsRuleAction::StopRecord }]);
    cfg.record_stop_wait_ms = 10_000;
    let mut state = ObsConnectionState::new(cfg);
    state.main_started = true;
    state.lifecycle.transition(ObsEventKey::Play);
    let (tx, worker, mut server) = start(state, VecDeque::new(), true).await?;
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::PlayEnded))?;
    assert!(tokio::time::timeout(Duration::from_millis(25), server.next()).await.is_err());
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::Result))?;
    let stop = request(&mut server).await?;
    assert_eq!(stop["d"]["requestType"], "StopRecord");
    respond(&mut server, &stop, json!({})).await?;
    tx.send(ObsCommand::Close)?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn shutdown_discards_queued_starts_and_gives_main_end_a_send_opportunity() -> Result<()> {
    let cfg = config(vec![
        ObsRule {
            trigger: ObsTrigger::MainStart,
            action: ObsRuleAction::ShowSource {
                scene_name: "Overlay".into(),
                source_name: "Missing".into(),
            },
        },
        ObsRule { trigger: ObsTrigger::MainStart, action: ObsRuleAction::StartRecord },
        ObsRule { trigger: ObsTrigger::MainEnd, action: ObsRuleAction::StopRecord },
    ]);
    let (tx, worker, mut server) =
        start(ObsConnectionState::new(cfg), VecDeque::new(), false).await?;
    let lookup = request(&mut server).await?;
    assert_eq!(lookup["d"]["requestType"], "GetSceneItemId");
    tx.send(ObsCommand::Shutdown)?;
    let end = request(&mut server).await?;
    assert_eq!(end["d"]["requestType"], "StopRecord");
    respond(&mut server, &lookup, json!({"sceneItemId": 55})).await?;
    respond(&mut server, &end, json!({})).await?;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), worker).await???.1,
        ConnectionAction::Shutdown
    );
    Ok(())
}

async fn record_stopped(server: &mut MockSocket) -> Result<()> {
    server
        .send(Message::Text(
            json!({ "op": 5, "d": {
                "eventType": "RecordStateChanged",
                "eventData": { "outputState": "OBS_WEBSOCKET_OUTPUT_STOPPED", "outputPath": "" },
            }})
            .to_string(),
        ))
        .await?;
    Ok(())
}

#[tokio::test]
async fn legacy_retry_restarts_recording_when_stop_finishes_after_half_the_select_hop() -> Result<()>
{
    let mut cfg = ObsConfig::default();
    cfg.scenes.insert("MUSICSELECT".into(), "Select".into());
    cfg.scenes.insert("PLAY".into(), "Play".into());
    let mut state = ObsConnectionState::new(cfg);
    state.lifecycle.transition(ObsEventKey::Play);
    // RetryPlay may race the GetRecordStatus response inside the worker.
    state.is_recording = true;
    let (tx, worker, mut server) = start(state, VecDeque::new(), true).await?;
    tx.send(ObsCommand::RetryPlay)?;
    let stop = request(&mut server).await?;
    assert_eq!(stop["d"]["requestType"], "StopRecord");
    let select = request(&mut server).await?;
    assert_eq!(select["d"]["requestData"]["sceneName"], "Select");
    respond(&mut server, &select, json!({})).await?;
    // STOPPED lands after 500 ms, so the restart deadline passes the 1 s Play hop.
    tokio::time::sleep(Duration::from_millis(700)).await;
    respond(&mut server, &stop, json!({})).await?;
    record_stopped(&mut server).await?;
    let play = request(&mut server).await?;
    assert_eq!(play["d"]["requestData"]["sceneName"], "Play");
    respond(&mut server, &play, json!({})).await?;
    let restart = request(&mut server).await?;
    assert_eq!(restart["d"]["requestType"], "StartRecord");
    tx.send(ObsCommand::Close)?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn leaving_play_before_retry_restart_does_not_repeat_play_end() -> Result<()> {
    let mut state = ObsConnectionState::new(config(vec![
        scene_rule(ObsTrigger::PlayStart),
        scene_rule(ObsTrigger::PlayEnd),
        scene_rule(ObsTrigger::MusicSelectStart),
    ]));
    state.main_started = true;
    state.lifecycle.transition(ObsEventKey::Play);
    let (tx, worker, mut server) = start(state, VecDeque::new(), true).await?;
    tx.send(ObsCommand::RetryPlay)?;
    expect_scene(&mut server, ObsTrigger::PlayEnd).await?;
    let stop = request(&mut server).await?;
    assert_eq!(stop["d"]["requestType"], "StopRecord");
    respond(&mut server, &stop, json!({})).await?;
    tx.send(ObsCommand::ApplyEvent(ObsEventKey::MusicSelect))?;
    // The deferred attempt never emitted PLAY_START, so it must not emit PLAY_END.
    expect_scene(&mut server, ObsTrigger::MusicSelectStart).await?;
    assert!(tokio::time::timeout(Duration::from_millis(700), server.next()).await.is_err());
    tx.send(ObsCommand::Close)?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn scene_picker_keeps_scenes_when_one_source_list_fails() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let cfg =
        ObsConfig { host: format!("ws://{}", listener.local_addr()?), ..ObsConfig::default() };
    let task = tokio::spawn(load_scenes(cfg));
    let (tcp, _) = tokio::time::timeout(Duration::from_secs(4), listener.accept()).await??;
    let mut server = accept_async(tcp).await?;
    server.send(Message::Text(json!({"op": 0, "d": {}}).to_string())).await?;
    server.next().await.context("Identify missing")??;
    server.send(Message::Text(json!({"op": 2, "d": {}}).to_string())).await?;
    for _ in 0..5 {
        let req = request(&mut server).await?;
        let data = match req["d"]["requestType"].as_str().unwrap() {
            "GetVersion" => json!({"obsVersion": "mock"}),
            "GetSceneList" => json!({"scenes": [{"sceneName": "A"}, {"sceneName": "Gone"}]}),
            "GetRecordStatus" => json!({"outputActive": false}),
            "GetSceneItemList" if req["d"]["requestData"]["sceneName"] == "Gone" => {
                server
                    .send(Message::Text(
                        json!({ "op": 7, "d": {
                            "requestId": req["d"]["requestId"],
                            "requestType": "GetSceneItemList",
                            "requestStatus": { "result": false, "code": 600, "comment": "missing" },
                        }})
                        .to_string(),
                    ))
                    .await?;
                continue;
            }
            "GetSceneItemList" => json!({"sceneItems": [{"sourceName": "Overlay"}]}),
            other => bail!("unexpected request {other}"),
        };
        respond(&mut server, &req, data).await?;
    }
    let list = task.await??;
    assert_eq!(list.scenes, ["Gone", "A"]);
    assert_eq!(list.sources["A"], ["Overlay"]);
    assert!(!list.sources.contains_key("Gone"));
    Ok(())
}
