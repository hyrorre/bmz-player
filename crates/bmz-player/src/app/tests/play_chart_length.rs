use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::screens::play_session::{
    PlaySessionOptions, PreparedPlaySession, build_practice_prepared_from_preloaded,
    build_prepared_play_session_from_preloaded, preload_play_session_for_chart_with_callbacks,
};
use bmz_chart::import::{BmsRandomSource, import_bms_chart_with_random_source};
use bmz_render::skin::{SkinDocumentRenderExt, SkinDocumentTexture, SkinImageSize, SkinRenderItem};

const RANDOM_CHART: &str = "#TITLE Length branches\n#BPM 120\n#WAV01 absent.wav\n#00011:01\n#02011:01\n#RANDOM 2\n#IF 1\n#09011:01\n#ENDIF\n#IF 2\n#12011:01\n#ENDIF\n#ENDRANDOM\n";

fn register(boot: &mut bootstrap::BootstrappedApp, name: &str, bms: &str) -> i64 {
    let path = boot.app_paths.data_dir.join(format!("{name}.bms"));
    std::fs::write(&path, bms).unwrap();
    let imported = import_bms_chart_with_random_source(
        &path,
        BmsRandomSource::Choices { random: vec![1], switches: vec![] },
        true,
    )
    .unwrap();
    boot.library_db
        .upsert_chart_import(&crate::storage::library_db::ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: bms.len() as u64,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap()
}

fn skin_file(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root).unwrap();
    let path = root.join("length.luaskin");
    std::fs::write(&path, r#"
        local s = require('main_state')
        local calls = 0
        return {type=0, w=1280, h=720,
            text={
                {id='load', constantText=s.number(1163)..':'..s.number(1164)},
                {id='named', constantText=s.number('chartlength_minute')..':'..s.number('chartlength_second')},
                {id='minute', numberRef=1163}, {id='second', numberRef=1164}
            },
            value={
                {id='sprite_m',src='digits',w=100,h=10,divx=10,digit=2,ref=1163},
                {id='sprite_s',src='digits',w=100,h=10,divx=10,digit=2,ref=1164},
                {id='callback_m',value=function() calls=calls+1; return s.number(1163) end},
                {id='callback_s',value=function() calls=calls+1; return s.number('chartlength_second') end}
            },
            destination={
                {id='load',dst={{x=0,y=0,w=200,h=20}}},
                {id='named',dst={{x=0,y=20,w=200,h=20}}},
                {id='minute',dst={{x=0,y=40,w=20,h=20}}},
                {id='second',dst={{x=20,y=40,w=20,h=20}}},
                {id='sprite_m',dst={{x=0,y=60,w=10,h=10}}},
                {id='sprite_s',dst={{x=20,y=60,w=10,h=10}}}
            }
        }
    "#).unwrap();
    path
}

fn queue(pipeline: &SkinPipelineRuntime, path: &Path, generation: u64, chart: PlaySkinChartState) {
    let mut runtime = bmz_skin::LuaLoadRuntimeState {
        runtime_mode: bmz_skin::LuaSkinRuntimeMode::Compat,
        ..Default::default()
    };
    // This must be overwritten on the worker, not captured in the document cache.
    apply_chart_length_lua_load_state(&mut runtime, Some(60_000));
    spawn_skin_decode(
        pipeline,
        SkinDecodeRequest::new(
            generation,
            path.to_owned(),
            SkinKind::Play,
            Default::default(),
            Default::default(),
            runtime,
        )
        .with_play_chart_state(Some(chart))
        .with_library_roots(vec![path.parent().unwrap().to_path_buf()]),
    );
}

fn facts(length_ms: i64) -> Arc<crate::screens::play_snapshot::PlaySkinChartFacts> {
    Arc::new(crate::screens::play_snapshot::PlaySkinChartFacts { length_ms, ..Default::default() })
}

fn receive(pipeline: &SkinPipelineRuntime) -> PendingSkinResult {
    pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(15)).unwrap()
}

struct DrawNumbers<'a>(&'a bmz_render::skin::SkinDrawState);
impl bmz_skin::LuaMainState for DrawNumbers<'_> {
    fn number(&self, id: i32) -> i64 {
        bmz_render::skin::lua_main_state_number(id, self.0)
    }
    fn option(&self, _: i32) -> bool {
        false
    }
    fn float(&self, _: i32) -> f64 {
        0.0
    }
    fn text(&self, _: i32) -> String {
        String::new()
    }
    fn timer(&self, _: i32) -> Option<i32> {
        None
    }
    fn event_index(&self, _: i32) -> i32 {
        0
    }
    fn gauge_type(&self) -> i32 {
        0
    }
    fn time_us(&self) -> i32 {
        0
    }
}

fn snapshot(play: &PreparedPlaySession, time: i64) -> RenderSnapshot {
    build_render_snapshot_with_target_and_bga_frames_cached(
        &play.session,
        TimeUs(time),
        &[],
        None,
        None,
        None,
        &Default::default(),
        &play.render_snapshot_cache,
    )
}

fn offline_decode(
    path: &Path,
    play: &PreparedPlaySession,
    profile: &ProfileConfig,
) -> crate::skin_loader::DecodedSkin {
    let loaded = bmz_skin::load_lua_skin_with_runtime_state(
        path,
        &Default::default(),
        &Default::default(),
        &bmz_skin::LuaLoadRuntimeState {
            runtime_mode: bmz_skin::LuaSkinRuntimeMode::Compat,
            ..offline_skin_load_state(play, profile, None)
        },
    )
    .unwrap();
    crate::skin_loader::DecodedSkin {
        document: loaded.document,
        lua_runtime: loaded.lua_runtime,
        load_dependencies: loaded.dependencies,
        kind: SkinKind::Play,
        fonts: vec![],
        sources: vec![],
        audio_assets: vec![],
        stats: Default::default(),
    }
}

fn assert_skin(
    mut decoded: crate::skin_loader::DecodedSkin,
    snapshot: &RenderSnapshot,
    expected: [i32; 2],
) {
    let state = skin_video::play_skin_video_draw_state(snapshot, None, None, 0);
    let label = format!("{}:{}", expected[0], expected[1]);
    assert_eq!(decoded.document.text[0].constant_text, label);
    assert_eq!(decoded.document.text[1].constant_text, label);
    for (id, expected) in [(1163, expected[0]), (1164, expected[1])] {
        assert_eq!(bmz_render::skin::lua_main_state_number(id, &state), i64::from(expected));
        assert_eq!(decoded.load_dependencies.number_values.get(&id), Some(&expected));
    }
    let mut runtime = decoded.lua_runtime.take().expect("persistent callbacks");
    for (index, expected) in [(2, expected[0]), (3, expected[1])] {
        let callback = decoded.document.value[index]
            .value_expr
            .strip_prefix("bmz:lua_value_callback:")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        runtime.begin_frame();
        assert_eq!(
            runtime.evaluate_number(callback, &DrawNumbers(&state)),
            Some(f64::from(expected))
        );
    }
    let sources = HashMap::from([(
        "digits".into(),
        SkinDocumentTexture {
            source_id: "digits".into(),
            texture: bmz_render::skin::SkinTextureId(9999),
            source_size: SkinImageSize { width: 100.0, height: 10.0 },
        },
    )]);
    let items = decoded.document.static_render_items(&sources, &state, &Default::default());
    let texts = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, [label.clone(), label, expected[0].to_string(), expected[1].to_string()]);
    let digits = items
        .iter()
        .filter_map(|item| match item {
            SkinRenderItem::Image { uv, .. } => Some((uv.x * 10.0).round() as i32),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected_digits = expected
        .into_iter()
        .flat_map(|number| {
            number.to_string().bytes().map(|digit| i32::from(digit - b'0')).collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(digits, expected_digits, "numeric sprite refs must use the same values");
}

#[test]
fn transformed_length_reaches_lua_before_audio_completion() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let id = register(
        &mut boot,
        "margin",
        "#TITLE Margin\n#BPM 120\n#WAV01 absent.wav\n#00011:01\n#09011:01\n",
    );
    assert_eq!(boot.library_db.list_charts_by_ids(&[id]).unwrap()[0].length_ms, 180_000);
    let pipeline = SkinPipelineRuntime::new();
    let state = PlaySkinChartState::default();
    queue(&pipeline, &skin_file(&data.paths.data_dir), 7, state.clone());
    assert!(pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_millis(20)).is_err());
    let (release_tx, release_rx) = mpsc::channel();
    let db_path = boot.app_paths.library_db.clone();
    let preloaded = thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let db = LibraryDatabase::open(&db_path).unwrap();
            let held = std::cell::Cell::new(false);
            preload_play_session_for_chart_with_callbacks(
                &db,
                id,
                Default::default(),
                1.0,
                |chart| state.finish(Ok(Arc::clone(&chart.render_snapshot_cache.skin_chart))),
                |_, _| {
                    if !held.replace(true) {
                        release_rx.recv_timeout(Duration::from_secs(15)).unwrap();
                    }
                },
            )
            .unwrap()
        });
        // Audio progress is deliberately held until CPU skin decode has completed.
        let decoded = receive(&pipeline);
        release_tx.send(()).unwrap();
        (decoded, worker.join().unwrap())
    });
    let (decoded, preloaded) = preloaded;
    let play = build_prepared_play_session_from_preloaded(
        preloaded,
        &boot.profile_config,
        Default::default(),
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    let snapshot = snapshot(&play, 40_000_000);
    assert_eq!(snapshot.skin_chart_length_ms, Some(181_000));
    assert_eq!(snapshot.duration, TimeUs(181_000_000));
    assert_skin(decoded.result.unwrap(), &snapshot, [3, 1]);
}

#[test]
fn play_skin_reads_chart_facts_while_loading() {
    let data = ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let path = data.paths.data_dir.join("facts.luaskin");
    std::fs::write(
        &path,
        r#"
        local s = require('main_state')
        local flags = {}
        for _, id in ipairs({153, 172, 173, 176, 177, 183}) do
            if s.option(id) then flags[#flags + 1] = id end
        end
        local label = table.concat({
            s.text(12), s.text(16), s.text(13), table.concat(flags, ','),
            s.number(90), s.number(91), s.number(92), s.number(96)
        }, '|')
        return {type=0, w=1280, h=720,
            text={{id='facts', constantText=label}},
            destination={{id='facts', dst={{x=0,y=0,w=200,h=20}}}}
        }
    "#,
    )
    .unwrap();
    let pipeline = SkinPipelineRuntime::new();
    let state = PlaySkinChartState::default();
    state.finish(Ok(Arc::new(crate::screens::play_snapshot::PlaySkinChartFacts {
        length_ms: 60_000,
        title: "長い曲名".into(),
        subtitle: "[HYPER]".into(),
        artist: "作者".into(),
        subartist: "obj".into(),
        genre: "ジャンル".into(),
        difficulty_name: "HYPER".into(),
        play_level: "12".into(),
        judge_rank: Some(90),
        has_long_notes: true,
        min_bpm: 120.0,
        max_bpm: 240.0,
        main_bpm: 150.0,
    })));
    queue(&pipeline, &path, 3, state);
    let decoded = receive(&pipeline).result.unwrap();
    assert_eq!(
        decoded.document.text[0].constant_text,
        "長い曲名 [HYPER]|作者 obj|ジャンル|153,173,177,183|240|120|150|12",
    );
}

#[test]
fn random_branch_decide_retry_replay_and_course_keep_attempt_lengths_separate() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let first = register(&mut boot, "random", RANDOM_CHART);
    let next = register(
        &mut boot,
        "next",
        "#TITLE Next\n#BPM 120\n#WAV01 absent.wav\n#00111:01\n#06011:01\n",
    );
    assert_eq!(boot.library_db.list_charts_by_ids(&[first]).unwrap()[0].length_ms, 180_000);
    let path = skin_file(&data.paths.data_dir);
    let pipeline = SkinPipelineRuntime::new();
    let options = PlaySessionOptions { bms_random_choices: Some(vec![2]), ..Default::default() };
    // The same worker is used by Decide's lookahead and a direct start.
    let pending = PendingPlayPreload::spawn(
        41,
        first,
        options.clone(),
        PlayPreloadSource::Import {
            library_db_path: boot.app_paths.library_db.clone(),
            normalization_output_gain: 1.0,
        },
    );
    queue(&pipeline, &path, 41, pending.skin_chart_state.clone());
    let prepared = pending.rx.recv_timeout(Duration::from_secs(15)).unwrap().result.unwrap();
    assert_eq!(prepared.preloaded.applied_arrange.bms_random_choices, [2]);
    let cached = prepared.preloaded.prepared_chart();
    let play = build_prepared_play_session_from_preloaded(
        prepared.preloaded,
        &boot.profile_config,
        options.clone(),
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    assert_eq!(snapshot(&play, 0).skin_chart_length_ms, Some(241_000));
    assert_skin(receive(&pipeline).result.unwrap(), &snapshot(&play, 0), [4, 1]);
    assert_skin(
        offline_decode(&path, &play, &boot.profile_config),
        &snapshot(&play, 40_000_000),
        [4, 1],
    );

    // Same-arrangement Retry reuses the already selected chart and its properties.
    let retry = PendingPlayPreload::spawn(
        42,
        first,
        options.clone(),
        PlayPreloadSource::Cached { chart: Box::new(cached), normalization_gain: 1.0 },
    );
    queue(&pipeline, &path, 42, retry.skin_chart_state.clone());
    let retried = retry.rx.recv_timeout(Duration::from_secs(15)).unwrap().result.unwrap();
    let play = build_prepared_play_session_from_preloaded(
        retried.preloaded,
        &boot.profile_config,
        options.clone(),
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    assert_skin(receive(&pipeline).result.unwrap(), &snapshot(&play, 200_000_000), [4, 1]);

    // Replay's recorded branch remains fixed when a random seed is supplied.
    let replay_options = PlaySessionOptions {
        replay_player: Some(Default::default()),
        bms_random_seed: Some(999),
        ..options
    };
    let replay = PendingPlayPreload::spawn(
        43,
        first,
        replay_options.clone(),
        PlayPreloadSource::Import {
            library_db_path: boot.app_paths.library_db.clone(),
            normalization_output_gain: 1.0,
        },
    );
    queue(&pipeline, &path, 43, replay.skin_chart_state.clone());
    let replayed = replay.rx.recv_timeout(Duration::from_secs(15)).unwrap().result.unwrap();
    let play = build_prepared_play_session_from_preloaded(
        replayed.preloaded,
        &boot.profile_config,
        replay_options,
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    assert_skin(receive(&pipeline).result.unwrap(), &snapshot(&play, -1_000_000), [4, 1]);

    // A next-course stage has a new mailbox and no margin (first note at 2s).
    let course = PendingPlayPreload::spawn(
        44,
        next,
        Default::default(),
        PlayPreloadSource::Import {
            library_db_path: boot.app_paths.library_db.clone(),
            normalization_output_gain: 1.0,
        },
    );
    queue(&pipeline, &path, 44, course.skin_chart_state.clone());
    pending.skin_chart_state.finish(Ok(facts(999_000))); // Late old publication cannot replace either value.
    assert_eq!(pending.skin_chart_state.wait().unwrap().length_ms, 241_000);
    let next = course.rx.recv_timeout(Duration::from_secs(15)).unwrap().result.unwrap();
    let play = build_prepared_play_session_from_preloaded(
        next.preloaded,
        &boot.profile_config,
        Default::default(),
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    let next_snapshot = snapshot(&play, 0);
    assert_eq!(next_snapshot.skin_chart_length_ms, Some(120_000));
    let decoded = receive(&pipeline);
    assert_eq!(decoded.generation, 44);
    assert_skin(decoded.result.unwrap(), &next_snapshot, [2, 0]);
}

#[test]
fn offline_and_practice_preserve_full_length_and_range_end_independently() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let id = register(&mut boot, "practice", RANDOM_CHART);
    let options = PlaySessionOptions {
        session_mode: SessionMode::Practice,
        bms_random_choices: Some(vec![2]),
        ..Default::default()
    };
    let preloaded = preload_play_session_for_chart_with_callbacks(
        &boot.library_db,
        id,
        options.clone(),
        1.0,
        |_| {},
        |_, _| {},
    )
    .unwrap();
    let play = build_practice_prepared_from_preloaded(
        preloaded,
        &boot.profile_config,
        &crate::screens::practice::PracticeProperty {
            start_time_ms: 30_000,
            end_time_ms: 45_000,
            playback_rate_percent: 150,
            ..Default::default()
        },
        options,
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    // Existing Practice range padding is preserved, independently of 1163/1164.
    assert_eq!(play.session.chart.end_time, TimeUs(45_500_000));
    assert_eq!(play.session.scored_total_notes, 1);
    assert_eq!(play.render_snapshot_cache.skin_chart.length_ms, 241_000);
    let snapshot = snapshot(&play, 31_000_000);
    assert_eq!(snapshot.duration, TimeUs(45_500_000));
    let decoded = offline_decode(&skin_file(&data.paths.data_dir), &play, &boot.profile_config);
    assert_skin(decoded, &snapshot, [4, 1]);
    let draw = skin_video::play_skin_video_draw_state(&snapshot, None, None, 0);
    assert_eq!(bmz_render::skin::lua_main_state_number(164, &draw), 15);
    assert_eq!(play.playback_rate_percent, 150);
}

#[test]
fn skin_length_includes_last_background_placement_without_changing_play_end() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let id = register(
        &mut boot,
        "background",
        "#TITLE Background tail\n#BPM 120\n#WAV01 absent.wav\n#BMP01 absent.png\n#00011:01\n#09011:01\n#10001:01\n#11004:01\n#12006:01\n#1300A:01\n",
    );
    let preloaded = preload_play_session_for_chart_with_callbacks(
        &boot.library_db,
        id,
        Default::default(),
        1.0,
        |_| {},
        |_, _| {},
    )
    .unwrap();
    let mut profile = boot.profile_config;
    profile.play.wait_all_notes_result = true;
    let play = build_prepared_play_session_from_preloaded(
        preloaded,
        &profile,
        Default::default(),
        Box::new(bmz_gameplay::input::backend::NullInputBackend),
    );
    let snapshot = snapshot(&play, 0);
    assert_eq!(snapshot.duration, TimeUs(181_000_000));
    // BGM and base/layer BGA placements count, Poor-only events do not.
    assert_eq!(snapshot.skin_chart_length_ms, Some(221_000));
    assert_eq!(play.session.result_wait_end_time, Some(TimeUs(261_000_000)));
    assert_skin(
        offline_decode(&skin_file(&data.paths.data_dir), &play, &profile),
        &snapshot,
        [3, 41],
    );
}

#[test]
fn absent_chart_returns_lua_sentinel_and_hides_numeric_objects() {
    let data = ProfileTestDir::new();
    let mut state = bmz_skin::LuaLoadRuntimeState::default();
    apply_chart_length_lua_load_state(&mut state, None);
    let loaded = bmz_skin::load_lua_skin_with_runtime_state(
        &skin_file(&data.paths.data_dir),
        &Default::default(),
        &Default::default(),
        &state,
    )
    .unwrap();
    assert_eq!(loaded.document.text[0].constant_text, "-2147483648:-2147483648");
    let draw = skin_video::play_skin_video_draw_state(&RenderSnapshot::default(), None, None, 0);
    for id in [1163, 1164] {
        assert_eq!(bmz_render::skin::lua_main_state_number(id, &draw), i64::from(i32::MIN));
    }
    let mut numeric = loaded.document;
    numeric.text.retain(|text| text.number_ref.is_some());
    let sources = HashMap::from([(
        "digits".into(),
        SkinDocumentTexture {
            source_id: "digits".into(),
            texture: bmz_render::skin::SkinTextureId(9999),
            source_size: SkinImageSize { width: 100.0, height: 10.0 },
        },
    )]);
    assert!(numeric.static_render_items(&sources, &draw, &Default::default()).is_empty());
}

#[test]
fn cancelled_and_failed_chart_preloads_release_skin_decode() {
    let data = ProfileTestDir::new();
    let boot = data.boot();
    let pipeline = SkinPipelineRuntime::new();
    let path = skin_file(&data.paths.data_dir);
    let cancelled = PlaySkinChartState::default();
    queue(&pipeline, &path, 1, cancelled.clone());
    cancelled.finish(Err("cancelled".into()));
    cancelled.finish(Ok(facts(60_000)));
    assert!(receive(&pipeline).result.is_err());
    let failed = PendingPlayPreload::spawn(
        2,
        -1,
        Default::default(),
        PlayPreloadSource::Import {
            library_db_path: boot.app_paths.library_db.clone(),
            normalization_output_gain: 1.0,
        },
    );
    queue(&pipeline, &path, 2, failed.skin_chart_state.clone());
    assert!(receive(&pipeline).result.is_err());
    assert!(failed.rx.recv_timeout(Duration::from_secs(15)).unwrap().result.is_err());
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires an isolated Winit event loop and offscreen GPU; run alone with --ignored"]
fn play_chart_length_app_decode_reuse_and_generation_binding() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let _tracing = tracing::subscriber::set_default(
        tracing_subscriber::fmt().with_max_level(tracing::Level::WARN).with_test_writer().finish(),
    );
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
    let first = register(&mut boot, "first", &format!("{RANDOM_CHART}#00018:01\n"));
    let next = register(
        &mut boot,
        "next",
        "#TITLE Next\n#BPM 120\n#WAV01 absent.wav\n#00118:01\n#06011:01\n",
    );
    let path = skin_file(&data.paths.data_dir).to_string_lossy().into_owned();
    boot.profile_config.skin.play5 = path.clone();
    boot.profile_config.skin.play7 = path;
    let (maintenance_tx, _) = tokio::sync::watch::channel(false);
    let mut app = WinitApp::new(
        boot,
        AppOptions { viewer_play: true, ..Default::default() },
        Instant::now(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        event_loop.create_proxy(),
        LogBuffer::default(),
        maintenance_tx,
        None,
    )
    .unwrap();
    app.renderer
        .attach_offscreen(bmz_render::renderer::SurfaceSize { width: 32, height: 32 })
        .unwrap();
    app.skin.default_skin_manifest = Some(bmz_render::skin::default_skin_manifest());
    app.viewer_mode = false;
    app.viewer_waiting = false;
    let options = PlayStartOptions { bms_random_choices: Some(vec![2]), ..Default::default() };
    app.begin_decide_for_chart(first, options.clone());
    let signature = app.skin.last_play_skin_signature.clone().unwrap();
    // Decide may already have drained a fast Play upload. Queue explicitly so
    // the reuse assertion always covers an in-flight request.
    app.skin.last_play_skin_signature = None;
    app.spawn_play_skin_decode_for(signature.0, signature.1, signature.5.clone());
    assert!(app.skin.skin_pipeline.is_pending(SkinKind::Play));
    let generation = app.skin.skin_pipeline.generation(SkinKind::Play);
    app.spawn_play_skin_decode_for(signature.0, signature.1, signature.5);
    assert_eq!(
        app.skin.skin_pipeline.generation(SkinKind::Play),
        generation,
        "same in-flight request must not restart Lua decode"
    );
    app.ensure_skin_ready(SkinKind::Play);
    assert_eq!(app.renderer.play_skin_document().unwrap().text[0].constant_text, "4:1");
    app.enter_play_scene(first, options, app.decide_snapshot_for_chart(first));
    assert_eq!(app.play.last_play_snapshot.as_ref().unwrap().skin_chart_length_ms, Some(241_000));

    app.invalidate_play_preload();
    assert!(app.skin.skin_pipeline.generation(SkinKind::Play) > generation);
    app.start_play_preload(next, Default::default());
    app.prepare_play_skin_for_scene(next, &Default::default());
    assert_eq!(app.renderer.play_skin_document().unwrap().text[0].constant_text, "2:0");
    app.enter_play_scene(next, Default::default(), app.decide_snapshot_for_chart(next));
    assert_eq!(app.play.last_play_snapshot.as_ref().unwrap().skin_chart_length_ms, Some(120_000));

    // Also exercise the branch where media completion was already buffered.
    let pending = app.play.pending_play_preload.take().unwrap();
    app.play.preloaded_play_session =
        Some(pending.rx.recv_timeout(Duration::from_secs(15)).unwrap().result.unwrap());
    app.skin.last_play_skin_signature = None;
    app.prepare_play_skin_for_scene(next, &Default::default());
    assert_eq!(app.renderer.play_skin_document().unwrap().text[0].constant_text, "2:0");
    app.invalidate_play_preload();
}

#[test]
fn stopping_skin_workers_releases_pending_chart_property_waits() {
    let data = ProfileTestDir::new();
    let mut pipeline = SkinPipelineRuntime::new();
    let state = PlaySkinChartState::default();
    queue(&pipeline, &skin_file(&data.paths.data_dir), 1, state.clone());
    pipeline.shutdown_workers();
    assert_eq!(state.wait().unwrap_err(), "skin decode cancelled");
}
