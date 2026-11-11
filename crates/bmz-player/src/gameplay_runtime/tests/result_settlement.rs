//! The worker's settled result, end-to-end through the real runtime and save path.
use super::*;
use crate::screens::play_finish::{
    FinishResultMode, FinishSessionResultOnceRequest, finish_session_result_once,
};
use crate::storage::{
    common::configure_connection,
    migration::{NETWORK_MIGRATIONS, SCORE_MIGRATIONS, run_migrations},
    network_db::NetworkDatabase,
    score_db::ScoreDatabase,
};
use bmz_core::clear::GaugeType;

const END_US: i64 = 1_000_000;

fn single_note_chart() -> Arc<PlayableChart> {
    let mut chart = (*chart()).clone();
    chart.lane_notes = std::array::from_fn(|_| Vec::new());
    chart.long_notes.clear();
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(192),
        time: TimeUs(END_US),
        sound: Some(SoundId(1)),
        layered_sounds: Vec::new(),
        damage: None,
    });
    chart.total_notes = 1;
    chart.end_time = TimeUs(END_US);
    Arc::new(chart)
}

fn single_note_session(input: SharedInputBackend) -> GameSession {
    let mut session = prepared_with_chart(input, single_note_chart());
    session.offsets.input_offset_us = 0;
    session
}

fn key(kind: InputKind, at_ms: i64) -> DeviceInputEvent {
    DeviceInputEvent {
        device: DeviceId(0),
        control: keyboard_control("Z"),
        kind,
        timestamp: DeviceTimestamp::MonotonicNs((at_ms as u128) * 1_000_000),
        bounce_policy: InputBouncePolicy::Bypass,
    }
}

struct Driver {
    client: GameplayClient,
    input: SharedInputBackend,
    probe: Arc<RuntimeProbe>,
}

impl Driver {
    /// Moves the audio clock and queues input in one owner command, then waits
    /// until the worker has finished that gameplay update.
    fn advance(&mut self, at_us: i64, events: Vec<DeviceInputEvent>) {
        let input = self.input.clone();
        assert!(self.client.edit(move |session| {
            session.audio_clock.current_frame.store(at_us as u64, Ordering::Release);
            for event in events {
                input.push_shared_event(event);
            }
        }));
        let probe = self.probe.clone();
        wait_until(|| probe.time.load(Ordering::Acquire) >= at_us);
    }

    fn published_result(&mut self) -> Arc<RuntimeResult> {
        wait_until(|| {
            self.client.poll();
            self.client.result.is_some()
        });
        self.client.result.clone().unwrap()
    }
}

fn finish(client: &GameplayClient) -> crate::screens::play_finish::FinishedPlaySession {
    let mut score = rusqlite::Connection::open_in_memory().unwrap();
    configure_connection(&score).unwrap();
    run_migrations(&mut score, SCORE_MIGRATIONS).unwrap();
    let mut network = rusqlite::Connection::open_in_memory().unwrap();
    configure_connection(&network).unwrap();
    run_migrations(&mut network, NETWORK_MIGRATIONS).unwrap();
    let root = std::env::temp_dir().join("bmz-runtime-settled-result");
    let paths = crate::paths::ProfilePaths {
        root_dir: root.clone(),
        profile_toml: root.join("profile.toml"),
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
        replay_dir: root.join("replay"),
    };
    let mut finished = finish_session_result_once(
        &mut None,
        &mut ScoreDatabase::from_connection(score),
        &mut NetworkDatabase::from_connection(network),
        FinishSessionResultOnceRequest {
            profile_paths: &paths,
            replay_config: &crate::config::profile_config::ReplayConfig {
                auto_save: false,
                compress: false,
                slot_rules: crate::config::profile_config::default_slot_rules(),
            },
            ir_config: &Default::default(),
            session: client,
            played_at: 1_700_000_000,
            applied_arrange: &AppliedArrange::default(),
            source_ln_profile: crate::ln_policy::ChartLnProfile::from_chart(&single_note_chart()),
            chart_length_ms: None,
            play_duration_ms: None,
            target_ex_score: None,
            target_name: "",
            target: Default::default(),
            score_key: crate::storage::score_db::ScoreKey::new(
                [0; 32],
                crate::ln_policy::LnScorePolicy::AutoLn,
            ),
            // Keep the test free of replay files; persistence rules are covered elsewhere.
            practice_mode: true,
            finish_mode: FinishResultMode::Normal,
        },
    )
    .unwrap();
    finished.summary.graph = Arc::new(
        crate::screens::result_model::ResultGraphCollector::default().snapshot_for_source(client),
    );
    finished
}

fn last_point(
    graph: &bmz_render::snapshot::ResultGraphSnapshot,
    gauge_type: GaugeType,
) -> bmz_render::snapshot::ResultGaugeGraphPoint {
    *graph.gauge_points.iter().rev().find(|point| point.gauge_type == gauge_type as i32).unwrap()
}

#[test]
fn settled_runtime_result_records_final_gauges_and_stays_fixed_until_finished() {
    let input = SharedInputBackend::default();
    let session = single_note_session(input.clone());
    let windows = session.judge.window_set;
    let settle_margin = windows.result_settle_margin_us();
    let empty_poor_ms = windows.note.empty_poor_slow_us / 2_000;
    assert!(empty_poor_ms > 0);
    // The final change and the settle point both fall before the 1500ms sample.
    assert!(END_US + settle_margin + 10_000 < 1_500_000);
    let probe = Arc::new(RuntimeProbe::default());
    let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
    let render_config = config(&session, probe.clone());
    let mut client = GameplayClient::new(session);
    client.start(audio, render_config).unwrap();
    let mut driver = Driver { client, input, probe };

    let end_ms = END_US / 1_000;
    driver.advance(END_US, vec![key(InputKind::Press, end_ms)]);
    driver.advance(END_US + 10_000, vec![key(InputKind::Release, end_ms + 10)]);
    // This empty POOR lowers every gauge after the 1000ms periodic sample.
    driver.advance(
        END_US + empty_poor_ms * 1_000,
        vec![key(InputKind::Press, end_ms + empty_poor_ms)],
    );
    driver.advance(
        END_US + empty_poor_ms * 1_000 + 10_000,
        vec![key(InputKind::Release, end_ms + empty_poor_ms + 10)],
    );
    assert!(driver.client.result.is_none());
    let settled_at = END_US + settle_margin + 1;
    driver.advance(settled_at, Vec::new());
    let settled = driver.published_result();
    assert_eq!(settled.settled_at, TimeUs(settled_at));
    let settled_fingerprint = driver.probe.fingerprint.lock().unwrap().clone();

    let finished = finish(&driver.client);
    let graph = &finished.summary.graph;
    assert!(!finished.gauge_carry.is_empty());
    for carry in &finished.gauge_carry {
        let last = last_point(graph, carry.gauge_type);
        assert_eq!((last.time_ms, last.value), ((settled_at / 1_000) as i32, carry.value));
    }
    // The selected gauge's number and the Result graph's end agree.
    let selected = last_point(graph, finished.summary.gauge_type);
    assert_eq!(selected.value, finished.summary.gauge_value);
    assert_eq!(finished.result.gauge_value, finished.summary.gauge_value);
    // The 1000ms periodic sample predates the empty POOR, so it differs.
    let sampled = graph
        .gauge_points
        .iter()
        .find(|point| {
            point.time_ms == 1_000 && point.gauge_type == finished.summary.gauge_type as i32
        })
        .unwrap();
    assert!(sampled.value > selected.value);

    // An old-timestamped input, an offset change and the later Finished
    // transition must not alter the settled score, gauges or replay.
    driver.advance(settled_at + 10_000, vec![key(InputKind::Press, end_ms + empty_poor_ms)]);
    driver.advance(settled_at + 20_000, vec![key(InputKind::Release, end_ms + 100)]);
    assert_eq!(*driver.probe.fingerprint.lock().unwrap(), settled_fingerprint);
    assert!(driver.client.edit(|session| session.offsets.input_offset_us = -200_000));
    driver.advance(settled_at + 30_000, vec![key(InputKind::Press, (settled_at + 30_000) / 1_000)]);
    let after_offset = driver.probe.fingerprint.lock().unwrap().clone();
    assert_ne!(after_offset, settled_fingerprint, "the offset itself is part of the fingerprint");
    assert_eq!(
        after_offset.replace("input_offset_us: -200000", "input_offset_us: 0"),
        settled_fingerprint
    );

    // Play keeps its existing 5s exit wait after the result settles.
    driver.advance(END_US + bmz_gameplay::session::SESSION_END_MARGIN_US, Vec::new());
    driver.client.poll();
    assert_eq!(driver.client.session.state, PlayState::Playing);
    driver.advance(END_US + bmz_gameplay::session::SESSION_END_MARGIN_US + 1_000, Vec::new());
    wait_until(|| {
        driver.client.poll();
        driver.client.session.state == PlayState::Finished
    });
    let terminal = driver.published_result();
    assert!(!Arc::ptr_eq(&terminal, &settled));
    assert_eq!(terminal.settled_at, settled.settled_at);
    assert_eq!(format!("{:?}", terminal.snapshot), format!("{:?}", settled.snapshot));
    assert_eq!(format!("{:?}", terminal.graph), format!("{:?}", settled.graph));
    // Only the exit-time play duration keeps following the terminal instant.
    assert!(terminal.play_duration_ms > settled.play_duration_ms);
    let refinished = finish(&driver.client);
    assert_eq!(format!("{:?}", refinished.result), format!("{:?}", finished.result));
    assert_eq!(refinished.summary.graph, finished.summary.graph);
    driver.client.shutdown();
}

#[test]
fn practice_stop_after_the_last_note_still_publishes_the_round_result() {
    // The UI can still observe notes left while the worker has just processed
    // the last one. Practice pauses the clock right after its stop request, so
    // the owner must end the round instead of leaving it Playing with no result.
    let input = SharedInputBackend::default();
    let session = single_note_session(input.clone());
    let probe = Arc::new(RuntimeProbe::default());
    let audio = AudioEngineHandle::new(AudioEngine::new(1_000_000));
    let render_config = config(&session, probe.clone());
    let mut client = GameplayClient::new(session);
    client.start(audio, render_config).unwrap();
    let mut driver = Driver { client, input, probe };

    let end_ms = END_US / 1_000;
    driver.advance(END_US, vec![key(InputKind::Press, end_ms)]);
    driver.advance(END_US + 10_000, vec![key(InputKind::Release, end_ms + 10)]);
    assert!(driver.client.result.is_none());

    let stopped_at = TimeUs(END_US + 20_000);
    assert!(driver.client.edit(move |session| {
        assert!(session.judge.is_exhausted(&session.chart));
        session.request_practice_round_end();
        session.audio_clock.pause_at(stopped_at);
    }));
    let result = driver.published_result();
    assert_eq!(result.settled_at, stopped_at);
    wait_until(|| {
        driver.client.poll();
        driver.client.session.state == PlayState::Finished
    });
    let graph = crate::screens::result_model::ResultGraphCollector::default()
        .snapshot_for_source(&driver.client);
    assert!(!graph.gauge_points.is_empty());
    driver.client.shutdown();
}
