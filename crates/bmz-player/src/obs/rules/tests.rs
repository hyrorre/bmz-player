use super::*;
use crate::config::app_config::ObsRule;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;

async fn sockets() -> Result<(ObsSink, WebSocketStream<TcpStream>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("ws://{}", listener.local_addr()?);
    let (client, server) = tokio::join!(connect_async(url), async {
        let (tcp, _) = listener.accept().await?;
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
