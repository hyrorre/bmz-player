//! OrajaHelperClient-compatible JSON Lines. All pipe I/O belongs to the worker.
use super::*;
use bmz_gameplay::{result::PlayResult, score::JudgeCounts};
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct HelperClient {
    sender: Option<std::sync::mpsc::SyncSender<(Instant, Value)>>,
    enabled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    selection: SelectionDebounce,
}

#[derive(Default)]
struct SelectionDebounce {
    pending: Option<([u8; 32], Instant, bool)>,
}

impl SelectionDebounce {
    fn observe(&mut self, hash: Option<[u8; 32]>, now: Instant) -> bool {
        let Some(hash) = hash else {
            self.pending = None;
            return false;
        };
        if self.pending.as_ref().is_none_or(|(previous, _, _)| *previous != hash) {
            self.pending = Some((hash, now, false));
            return false;
        }
        let (_, changed_at, sent) = self.pending.as_mut().expect("selection checked above");
        if *sent || now.saturating_duration_since(*changed_at) < Duration::from_secs(1) {
            return false;
        }
        *sent = true;
        true
    }
}

impl HelperClient {
    fn disable(&mut self) {
        if let Some(enabled) = self.enabled.take() {
            enabled.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        self.sender = None;
        self.selection = SelectionDebounce::default();
    }

    fn send(&mut self, payload: Value) {
        if !cfg!(windows) {
            return;
        }
        let sender = self.sender.get_or_insert_with(|| {
            let (sender, receiver) = std::sync::mpsc::sync_channel(64);
            let enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            self.enabled = Some(enabled.clone());
            if let Err(error) = std::thread::Builder::new()
                .name("oraja-helper-pipe".into())
                .spawn(move || pipe_worker(receiver, enabled))
            {
                tracing::warn!(%error, "failed to start oraja_helper sender");
            }
            sender
        });
        if let Err(error) = sender.try_send((Instant::now(), payload)) {
            let level = queue_failure_level(&error);
            let (reason, (_, payload)) = match error {
                std::sync::mpsc::TrySendError::Full(item) => ("full", item),
                std::sync::mpsc::TrySendError::Disconnected(item) => ("disconnected", item),
            };
            let event = payload.get("event").and_then(Value::as_str).unwrap_or("unknown");
            if level == tracing::Level::DEBUG {
                tracing::debug!(event, reason, "oraja_helper queue unavailable; event discarded");
            } else {
                tracing::warn!(event, reason, "oraja_helper queue unavailable; event discarded");
            }
        }
    }
}

fn queue_failure_level(error: &std::sync::mpsc::TrySendError<(Instant, Value)>) -> tracing::Level {
    match error {
        std::sync::mpsc::TrySendError::Full((_, payload))
            if payload.get("event").and_then(Value::as_str) == Some("song_select") =>
        {
            tracing::Level::DEBUG
        }
        _ => tracing::Level::WARN,
    }
}

#[cfg(windows)]
fn pipe_worker(
    receiver: std::sync::mpsc::Receiver<(Instant, Value)>,
    enabled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        tracing::warn!("failed to create oraja_helper I/O runtime");
        return;
    };
    for (queued_at, payload) in receiver {
        if !enabled.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if queued_at.elapsed() > Duration::from_secs(2) {
            continue;
        }
        let mut body = payload.to_string();
        body.push('\n');
        runtime.block_on(send_line(r"\\.\pipe\oraja_helper", &body));
    }
}

#[cfg(windows)]
async fn send_line(path: &str, body: &str) {
    use tokio::io::AsyncWriteExt;
    use tokio::net::windows::named_pipe::ClientOptions;
    // The helper creates a new server instance after processing each line.
    // During that gap the pipe may be absent as well as busy.
    let send = async {
        let mut pipe = loop {
            match ClientOptions::new().read(false).write(true).open(path) {
                Ok(pipe) => break pipe,
                Err(error) if matches!(error.raw_os_error(), Some(2 | 231)) => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => return Err(error),
            }
        };
        pipe.write_all(body.as_bytes()).await
    };
    match tokio::time::timeout(Duration::from_secs(1), send).await {
        Ok(Ok(())) => {}
        outcome => tracing::debug!(?outcome, "oraja_helper event not delivered"),
    }
}

#[cfg(not(windows))]
fn pipe_worker(
    _: std::sync::mpsc::Receiver<(Instant, Value)>,
    _: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
}

fn base(
    event: &str,
    scene: &str,
    metadata: &bmz_chart::model::ChartMetadata,
    sha256: &[u8; 32],
    md5: &[u8; 16],
) -> Value {
    json!({
        "event": event, "scene": scene, "source": "bmz-player",
        "title": format!("{} {}", metadata.title, metadata.subtitle).trim(),
        "artist": format!("{} {}", metadata.artist, metadata.subartist).trim(),
        "sha256": crate::storage::common::hash_to_hex(sha256),
        "md5": crate::storage::common::hash_to_hex(md5),
    })
}

fn option(option: crate::select_options::ArrangeOption) -> (&'static str, Option<u8>) {
    use crate::select_options::ArrangeOption::*;
    let id = match option {
        Normal => Some(0),
        Mirror => Some(1),
        Random => Some(2),
        RRandom => Some(3),
        SRandom => Some(4),
        Spiral => Some(5),
        HRandom => Some(6),
        AllScratch => Some(7),
        RandomEx => Some(8),
        SRandomEx => Some(9),
        FRandom | MFRandom => None,
    };
    (if option == Normal { "正規" } else { option.as_str() }, id)
}

/// BMZ patterns list the source lane for each destination (scratch=0, P2 key1=8).
/// The helper expects source key numbers, with scratches last and excluded when fixed.
fn placement(arrange: &AppliedArrange, mode: KeyMode, side: usize) -> String {
    let option = if side == 0 { arrange.arrange } else { arrange.arrange_2p };
    if matches!(
        option,
        crate::select_options::ArrangeOption::Normal | crate::select_options::ArrangeOption::Mirror
    ) || arrange.pattern.is_none()
    {
        return String::new();
    }
    // A per-note side has no single lane placement even if the other side does.
    if arrange.pattern_is_partial
        && matches!(
            option,
            crate::select_options::ArrangeOption::SRandom
                | crate::select_options::ArrangeOption::HRandom
                | crate::select_options::ArrangeOption::AllScratch
                | crate::select_options::ArrangeOption::SRandomEx
                | crate::select_options::ArrangeOption::Spiral
        )
    {
        return String::new();
    }
    let keys = match mode {
        KeyMode::K5 | KeyMode::K10 => 5,
        KeyMode::K7 | KeyMode::K14 => 7,
        KeyMode::K4 => 4,
        KeyMode::K6 => 6,
        KeyMode::K8 => 8,
        KeyMode::K9 => 9,
    };
    let start = if side == 0 { 1 } else { 8 };
    let source = |destination: usize| -> usize {
        arrange
            .pattern
            .as_ref()
            .and_then(|p| p.get(destination))
            .copied()
            .map_or(destination, usize::from)
    };
    let key_number = |lane| match lane {
        0 => keys + 1,
        15 => 2 * (keys + 1),
        8..=14 if matches!(mode, KeyMode::K10 | KeyMode::K14) => lane - 8 + keys + 2,
        _ => lane,
    };
    let mut result: String =
        (start..start + keys).map(|lane| key_number(source(lane)).to_string()).collect();
    if matches!(mode, KeyMode::K5 | KeyMode::K7 | KeyMode::K10 | KeyMode::K14) {
        let scratch = if side == 0 { 0 } else { 15 };
        if source(scratch) != scratch {
            result.push_str(&key_number(source(scratch)).to_string());
        }
    }
    result
}

fn add_options(payload: &mut Value, arrange: &AppliedArrange, mode: KeyMode) {
    let (name, id) = option(arrange.arrange);
    payload["option"] = json!(name);
    payload["optionId"] = json!(id);
    payload["randomSeed"] = json!(arrange.seed);
    payload["randomPlacement"] = json!(placement(arrange, mode, 0));
    if matches!(mode, KeyMode::K10 | KeyMode::K14) {
        let (name, id) = option(arrange.arrange_2p);
        payload["option2P"] = json!(name);
        payload["option2PId"] = json!(id);
        payload["randomSeed2P"] = json!(arrange.seed_2p);
        payload["randomPlacement2P"] = json!(placement(arrange, mode, 1));
        payload["doubleOption"] =
            json!(crate::skin_extension::double_option_index(arrange.double_option));
    }
}

fn judges(counts: &JudgeCounts) -> Value {
    json!({ "epg": counts.fast_pgreat, "lpg": counts.slow_pgreat,
        "egr": counts.fast_great, "lgr": counts.slow_great,
        "egd": counts.fast_good, "lgd": counts.slow_good,
        "ebd": counts.fast_bad, "lbd": counts.slow_bad,
        "epr": counts.fast_poor, "lpr": counts.slow_poor,
        "ems": counts.fast_empty_poor, "lms": counts.slow_empty_poor })
}

fn add_result(payload: &mut Value, result: &PlayResult) {
    payload["score"] = json!(result.score.ex_score());
    payload["scoreRate"] = json!(if result.total_notes == 0 {
        0.0
    } else {
        f64::from(result.score.ex_score()) * 50.0 / f64::from(result.total_notes)
    });
    payload["clearLamp"] = json!(result.clear_type.as_str());
    payload["clearLampId"] = json!(result.clear_type as u8);
    payload["missCount"] = json!(result.record_bp());
    payload["judges"] = judges(&result.score.judges);
}

impl WinitApp {
    pub(super) fn publish_helper_selection(&mut self, scene: AppSceneKind) {
        if !self.boot.app_config.oraja_helper_enabled {
            self.integrations.oraja_helper.disable();
            return;
        }
        if scene != AppSceneKind::Select {
            self.integrations.oraja_helper.selection = SelectionDebounce::default();
            return;
        }
        let hash =
            self.selected_chart_row().and_then(|row| row.chart.as_ref()).map(|chart| chart.sha256);
        if !self.integrations.oraja_helper.selection.observe(hash, Instant::now()) {
            return;
        }
        let Some(chart) = self.selected_chart_row().and_then(|row| row.chart.as_ref()) else {
            return;
        };
        let payload = json!({"event": "song_select", "scene": "select", "source": "bmz-player",
            "title": format!("{} {}", chart.title, chart.subtitle).trim(),
            "artist": format!("{} {}", chart.artist, chart.subartist).trim(),
            "sha256": crate::storage::common::hash_to_hex(&chart.sha256),
            "md5": crate::storage::common::hash_to_hex(&chart.md5)});
        self.integrations.oraja_helper.send(payload);
    }

    pub(super) fn publish_helper_play(&mut self) {
        if !self.boot.app_config.oraja_helper_enabled {
            return;
        }
        let Some(active) = &self.play.active_play else {
            return;
        };
        let session = &active.running.session;
        if !helper_play_allowed(
            self.viewer_mode,
            self.play.practice_session.is_some(),
            active.running.score_save_disabled,
            session.autoplay.as_ref().is_some_and(|a| a.is_full()),
            session.replay_player.is_some(),
        ) {
            return;
        }
        let chart = &session.chart;
        let mut payload = base(
            "song_play",
            "play",
            &chart.metadata,
            &chart.identity.file_sha256,
            &chart.identity.file_md5,
        );
        add_options(&mut payload, &active.running.applied_arrange, session.primary_key_mode);
        self.integrations.oraja_helper.send(payload);
    }

    pub(super) fn publish_helper_play_end(&mut self, quick_retry: bool) {
        if !self.boot.app_config.oraja_helper_enabled {
            return;
        }
        let Some(active) = &self.play.active_play else {
            return;
        };
        let session = &active.running.session;
        if !helper_play_allowed(
            self.viewer_mode,
            self.play.practice_session.is_some(),
            active.running.score_save_disabled,
            session.autoplay.as_ref().is_some_and(|a| a.is_full()),
            session.replay_player.is_some(),
        ) {
            return;
        }
        let result = active
            .running
            .gameplay
            .result
            .as_ref()
            .map(|r| r.snapshot.helper_result())
            .or_else(|| active.running.finished.as_ref().map(|f| &f.result));
        let Some(result) = result else {
            return;
        };
        let chart = &session.chart;
        let mut payload = base(
            "song_play_end",
            "play",
            &chart.metadata,
            &chart.identity.file_sha256,
            &chart.identity.file_md5,
        );
        add_options(&mut payload, &active.running.applied_arrange, session.primary_key_mode);
        add_result(&mut payload, result);
        payload["playedNotes"] = json!(result.score.past_notes);
        payload["totalNotes"] = json!(result.total_notes);
        payload["elapsedSeconds"] = json!(
            active
                .running
                .gameplay
                .result
                .as_ref()
                .map(|r| r.play_duration_ms)
                .or(active.running.play_duration_ms)
                .unwrap_or(0)
                / 1000
        );
        payload["playEndMetrics"] = json!(true);
        payload["quickRetry"] = json!(quick_retry);
        self.integrations.oraja_helper.send(payload);
    }

    pub(super) fn publish_helper_result(
        &mut self,
        finished: &FinishedPlaySession,
        score_save_disabled: bool,
    ) {
        if !self.boot.app_config.oraja_helper_enabled {
            return;
        }
        if let Some(payload) = helper_result_payload(
            finished,
            self.viewer_mode,
            self.play.practice_session.is_some(),
            score_save_disabled,
        ) {
            self.integrations.oraja_helper.send(payload);
        }
    }
}

fn helper_play_allowed(
    viewer: bool,
    practice: bool,
    score_save_disabled: bool,
    autoplay: bool,
    replay: bool,
) -> bool {
    !(viewer || practice || score_save_disabled || autoplay || replay)
}

fn helper_result_payload(
    finished: &FinishedPlaySession,
    viewer: bool,
    practice: bool,
    score_save_disabled: bool,
) -> Option<Value> {
    if !helper_play_allowed(
        viewer,
        practice,
        score_save_disabled || finished.applied_arrange.score_persistence_disabled(),
        finished.result.autoplay,
        finished.replay_playback,
    ) {
        return None;
    }
    let s = &finished.summary;
    let mut payload = json!({"event": "song_result", "scene": "result", "source": "bmz-player",
            "title": format!("{} {}", s.title, s.subtitle).trim(),
            "artist": format!("{} {}", s.artist, s.subartist).trim(),
            "sha256": s.chart_sha256.map(|hash| crate::storage::common::hash_to_hex(&hash)).unwrap_or_default(),
            "md5": s.chart_md5.map(|hash| crate::storage::common::hash_to_hex(&hash)).unwrap_or_default()});
    add_options(&mut payload, &finished.applied_arrange, s.key_mode);
    add_result(&mut payload, &finished.result);
    Some(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select_options::ArrangeOption;

    #[test]
    fn full_selection_queue_is_debug_but_result_and_disconnection_warn() {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        sender.try_send((Instant::now(), json!({"event": "song_play"}))).unwrap();
        for (event, expected) in [
            ("song_select", tracing::Level::DEBUG),
            ("song_play_end", tracing::Level::WARN),
            ("song_result", tracing::Level::WARN),
        ] {
            let error = sender.try_send((Instant::now(), json!({"event": event}))).unwrap_err();
            assert_eq!(queue_failure_level(&error), expected);
        }
        drop(receiver);
        let error = sender.try_send((Instant::now(), json!({"event": "song_select"}))).unwrap_err();
        assert_eq!(queue_failure_level(&error), tracing::Level::WARN);
    }

    #[test]
    fn selection_waits_for_stability_and_cancels_on_change_or_exit() {
        let mut selection = SelectionDebounce::default();
        let start = Instant::now();
        let a = Some([1; 32]);
        let b = Some([2; 32]);
        assert!(!selection.observe(a, start));
        assert!(!selection.observe(a, start + Duration::from_millis(999)));
        assert!(!selection.observe(b, start + Duration::from_millis(999)));
        assert!(!selection.observe(b, start + Duration::from_millis(1998)));
        assert!(selection.observe(b, start + Duration::from_millis(1999)));
        assert!(!selection.observe(b, start + Duration::from_secs(3)));
        assert!(!selection.observe(None, start + Duration::from_secs(4)));
        assert!(!selection.observe(b, start + Duration::from_secs(5)));
        assert!(selection.observe(b, start + Duration::from_secs(6)));
    }

    #[test]
    fn helper_freq_eligibility_is_shared_by_play_end_and_result() {
        for (freq, expected) in [(-1, false), (0, true)] {
            let mut options = PlayStartOptions::default();
            crate::screens::play_start::apply_freq_option(&mut options, freq);
            let allowed =
                helper_play_allowed(false, false, options.score_save_disabled, false, false);
            let finished = debug_boot_finished_play_session();
            assert_eq!(allowed, expected, "song_play and song_play_end at FREQ {freq}");
            assert_eq!(
                helper_result_payload(&finished, false, false, options.score_save_disabled)
                    .is_some(),
                expected,
                "song_result at FREQ {freq}"
            );
        }
    }

    #[test]
    fn helper_course_stage_result_keeps_chart_and_attempt_data() {
        let mut finished = debug_boot_finished_play_session();
        finished.summary.chart_sha256 = Some([0xab; 32]);
        finished.applied_arrange.seed = Some(42);
        finished.result.total_notes = 10;
        finished.result.score = Default::default();
        finished.result.score.judges.fast_pgreat = 3;
        let payload = helper_result_payload(&finished, false, false, false).unwrap();
        assert_eq!(payload["event"], "song_result");
        assert_eq!(payload["sha256"], "ab".repeat(32));
        assert_eq!(payload["randomSeed"], 42);
        assert_eq!(payload["score"], 6);
        assert_eq!(payload["judges"]["epg"], 3);
        assert!(helper_result_payload(&finished, false, false, true).is_none());
        finished.result.autoplay = true;
        assert!(helper_result_payload(&finished, false, false, false).is_none());
    }

    #[test]
    fn disabling_clears_selection_and_stops_old_worker() {
        let enabled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (sender, _receiver) = std::sync::mpsc::sync_channel(1);
        let mut client = HelperClient {
            sender: Some(sender),
            enabled: Some(enabled.clone()),
            selection: SelectionDebounce { pending: Some(([1; 32], Instant::now(), false)) },
        };
        client.disable();
        assert!(!enabled.load(std::sync::atomic::Ordering::Relaxed));
        assert!(client.sender.is_none());
        assert!(client.selection.pending.is_none());
    }

    #[test]
    fn helper_placements_use_beatoraja_source_numbers() {
        let mut arrange = AppliedArrange::default();
        assert_eq!(placement(&arrange, KeyMode::K7, 0), "");
        assert_eq!(placement(&arrange, KeyMode::K14, 1), "");
        let mut pattern: Vec<u8> = (0..16).collect();
        pattern[1..8].reverse();
        arrange.pattern = Some(pattern.clone());
        arrange.arrange = ArrangeOption::Mirror;
        assert_eq!(placement(&arrange, KeyMode::K7, 0), "");
        arrange.arrange = ArrangeOption::Random;
        assert_eq!(placement(&arrange, KeyMode::K7, 0), "7654321");
        arrange.arrange_2p = ArrangeOption::Mirror;
        assert_eq!(placement(&arrange, KeyMode::K14, 1), "");
        arrange.arrange_2p = ArrangeOption::Random;
        assert_eq!(placement(&arrange, KeyMode::K14, 1), "9101112131415");
        assert_eq!(placement(&arrange, KeyMode::K10, 1), "7891011");
        pattern.swap(0, 7);
        arrange.pattern = Some(pattern);
        assert_eq!(placement(&arrange, KeyMode::K7, 0), "76543281");
        arrange.arrange_2p = ArrangeOption::SRandom;
        arrange.pattern_is_partial = true;
        assert_eq!(placement(&arrange, KeyMode::K14, 1), "");
        arrange.arrange = ArrangeOption::SRandom;
        arrange.pattern = None;
        assert_eq!(placement(&arrange, KeyMode::K7, 0), "");
    }

    #[test]
    fn helper_results_keep_raw_judgements_and_unprocessed_bp() {
        let mut finished = debug_boot_finished_play_session();
        finished.result.clear_type = bmz_core::clear::ClearType::Failed;
        finished.result.total_notes = 100;
        finished.result.score.judges = JudgeCounts {
            fast_pgreat: 10,
            slow_pgreat: 20,
            fast_great: 3,
            slow_great: 4,
            fast_good: 1,
            slow_good: 2,
            fast_bad: 3,
            slow_bad: 4,
            fast_poor: 5,
            slow_poor: 6,
            fast_empty_poor: 7,
            slow_empty_poor: 8,
        };
        let mut payload = json!({});
        add_result(&mut payload, &finished.result);
        assert_eq!(payload["score"], 67);
        assert_eq!(payload["scoreRate"], 33.5);
        assert_eq!(payload["clearLampId"], 1);
        assert_eq!(payload["judges"]["epr"], 5);
        assert_eq!(payload["judges"]["ems"], 7);
        assert_eq!(payload["missCount"], finished.result.record_bp());
        finished.result.total_notes = 0;
        add_result(&mut payload, &finished.result);
        assert_eq!(payload["scoreRate"], 0.0);
    }

    #[test]
    fn helper_metadata_roundtrips_unicode_and_newlines() {
        let metadata = bmz_chart::model::ChartMetadata {
            title: "テスト\n\"曲\"".into(),
            subtitle: "副題".into(),
            ..Default::default()
        };
        let payload = base("song_play", "play", &metadata, &[0xab; 32], &[0xcd; 16]);
        let body = payload.to_string();
        assert!(!body.contains('\n'));
        let decoded: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(decoded["title"], "テスト\n\"曲\" 副題");
        assert_eq!(decoded["sha256"], "ab".repeat(32));
        assert_eq!(decoded["md5"], "cd".repeat(16));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn helper_pipe_reconnects_for_each_json_line() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::net::windows::named_pipe::ServerOptions;
        let temp = tempfile::tempdir().unwrap();
        let path = format!(
            r"\\.\pipe\bmz-helper-test-{}",
            temp.path().file_name().unwrap().to_string_lossy()
        );
        for event in ["song_select", "song_play", "song_play_end", "song_result"] {
            let server = ServerOptions::new()
                .access_inbound(true)
                .access_outbound(false)
                .create(&path)
                .unwrap();
            let body = format!("{}\n", json!({"event": event, "title": "テスト"}));
            let receive = async {
                server.connect().await.unwrap();
                let mut line = String::new();
                BufReader::new(server).read_line(&mut line).await.unwrap();
                assert_eq!(line, body);
            };
            tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(send_line(&path, &body), receive);
            })
            .await
            .unwrap();
        }
        // The Python receiver closes the old pipe before creating the next one.
        let delayed_receive = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let server = ServerOptions::new()
                .access_inbound(true)
                .access_outbound(false)
                .create(&path)
                .unwrap();
            server.connect().await.unwrap();
            let mut line = String::new();
            BufReader::new(server).read_line(&mut line).await.unwrap();
            assert_eq!(line, "{}\n");
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(send_line(&path, "{}\n"), delayed_receive);
        })
        .await
        .unwrap();
        // A missing receiver is an ordinary no-op, with no deferred resend.
        send_line(&path, "{}\n").await;
    }
}
