//! CPU-only Select / Result / CourseResult planning probe; no GPU or DB writes.
//! Usage: scene_plan_profile select|result|course SKIN idle|scroll|panel [PROFILE]
//! BMZ_SCENE_PROFILE_FRAMES sets measured frames (default 2000).
//! BMZ_SCENE_PROFILE_STAGES sets course length (default 4, max 10).
//! BMZ_SCENE_PROFILE_VERIFY hashes every rendered primitive outside the timer.
//! BMZ_SCENE_PROFILE_DUMP writes representative plans for mismatch diagnosis.
//! These synthetic workloads do not measure application FPS or snapshot building.

use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    hint::black_box,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use bmz_core::{judge::Judge, time::TimeUs};
use bmz_player::{
    config::profile_config::ProfileConfig,
    skin_loader::{
        BeatorajaSkinDecodeRequest, SkinKind, decode_beatoraja_skin_request,
        set_decoded_skin_context,
    },
};
use bmz_render::{
    plan::DrawCommand,
    renderer::Renderer,
    scene::{AppSceneSnapshot, CourseStageResultSkinSnapshot},
    skin::{SkinDocumentTexture, default_skin_manifest},
    snapshot::{
        ResultEarlyLateGraphBucket, ResultGaugeGraphPoint, ResultGraphSnapshot,
        ResultJudgeGraphBucket, ResultNoteGraphBucket, ResultTimingPoint,
    },
};

fn env_count(name: &str, default: usize, max: usize) -> Result<usize> {
    let value =
        std::env::var(name).ok().map(|v| v.parse::<usize>()).transpose()?.unwrap_or(default);
    ensure!((1..=max).contains(&value), "{name} must be 1..={max}");
    Ok(value)
}

fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "run with --release");
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::stderr)
        .without_time()
        .with_ansi(false)
        .init();
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        (3..=4).contains(&args.len()),
        "usage: scene_plan_profile SCENE SKIN WORKLOAD [PROFILE]"
    );
    let mode = args[0].as_str();
    ensure!(["select", "result", "course"].contains(&mode), "unknown scene");
    let workload = args[2].as_str();
    ensure!(["idle", "scroll", "panel"].contains(&workload), "unknown workload");
    ensure!(mode == "select" || workload != "scroll", "scroll is Select-only");
    let path = PathBuf::from(&args[1]).canonicalize()?;
    let profile: Option<ProfileConfig> = args
        .get(3)
        .map(|p| -> Result<_> { Ok(toml::from_str(&std::fs::read_to_string(p)?)?) })
        .transpose()?;
    let frames = env_count("BMZ_SCENE_PROFILE_FRAMES", 2000, 1_000_000)?;
    let mut plan_hash = std::env::var_os("BMZ_SCENE_PROFILE_VERIFY").map(|_| DefaultHasher::new());
    let dump_dir = std::env::var_os("BMZ_SCENE_PROFILE_DUMP").map(PathBuf::from);
    if let Some(dir) = &dump_dir {
        std::fs::create_dir_all(dir)?;
    }
    let stages = if mode == "course" { env_count("BMZ_SCENE_PROFILE_STAGES", 4, 10)? } else { 1 };
    let empty = BTreeMap::new();
    let (options, files) = profile.as_ref().map_or((&empty, &empty), |p| match mode {
        "select" => (&p.skin.select_options, &p.skin.select_files),
        "course" => (&p.skin.course_result_options, &p.skin.course_result_files),
        _ => (&p.skin.result_options, &p.skin.result_files),
    });
    let kind = if mode == "select" { SkinKind::Select } else { SkinKind::Result };
    let mut scene = make_scene(mode, stages);
    let mut load_state = bmz_skin::LuaLoadRuntimeState::default();
    if let AppSceneSnapshot::Result(s) = &scene {
        for (i, title) in s.course_titles.iter().enumerate() {
            load_state.text_values.insert(150 + i as i32, title.clone());
        }
        load_state.number_values.insert(
            bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_COUNT,
            s.course_result.stage_count as i32,
        );
        for (i, stage) in s.course_result.stages.iter().enumerate() {
            for (base, value) in [
                (bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_EX_BASE, stage.ex_score as i32),
                (bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE, stage.gauge as i32),
                (bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_BP_BASE, stage.bp as i32),
                (
                    bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE,
                    stage.rate_basis_points as i32,
                ),
            ] {
                load_state.number_values.insert(base + i as i32, value);
            }
        }
        if mode == "course" {
            let songs = (0..stages)
                .map(|i| {
                    serde_json::json!({
                        "stage": i + 1, "score": 5400, "gauge": 84, "miss": 0, "rate": 0.9,
                    })
                })
                .collect::<Vec<_>>();
            load_state.virtual_io_files.insert(
                "skin/WMII_FHD/result/courseData.json".into(),
                serde_json::json!({"songs": songs}).to_string(),
            );
        }
    }
    let library = path
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "skins"))
        .or(path.parent())
        .context("missing parent")?
        .to_path_buf();
    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        skin_path: &path,
        kind,
        options,
        files,
        runtime_state: &load_state,
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &[library],
    })?;
    ensure!(
        match mode {
            "select" => decoded.document.skin_type == 5,
            "course" => matches!(decoded.document.skin_type, 7 | 15),
            _ => decoded.document.skin_type == 7,
        },
        "unexpected skin type: {}",
        decoded.document.skin_type
    );
    println!(
        "{}",
        serde_json::json!({
            "skin": path, "scene": mode, "workload": workload, "stages": stages,
            "profile": args.get(3), "frames": frames, "warmup": 300,
            "skin_type": decoded.document.skin_type,
            "destinations": decoded.document.all_destinations(&decoded.document.enabled_options()).len(),
            "images": decoded.document.image.len(), "values": decoded.document.value.len(),
            "texts": decoded.document.text.len(), "sources": decoded.sources.len(),
            "videos": decoded.sources.iter().filter(|s| s.is_video).map(|s| &s.path).collect::<Vec<_>>(),
            "callbacks": decoded.lua_runtime.as_ref().map(|r| (0..r.callback_count()).filter_map(|i| r.callback_path(i)).collect::<Vec<_>>()),
        })
    );
    let textures = decoded
        .sources
        .iter()
        .map(|s| SkinDocumentTexture {
            source_id: s.source_id.clone(),
            texture: s.texture,
            source_size: s.size,
        })
        .collect();
    let mut renderer = Renderer::default();
    set_decoded_skin_context(
        &mut renderer,
        kind,
        default_skin_manifest(),
        decoded.document,
        decoded.lua_runtime,
        textures,
        false,
    );
    drop(decoded.sources);
    drop(decoded.fonts);
    drop(decoded.audio_assets);
    let mut samples = Vec::with_capacity(frames);
    let mut commands = 0;
    for frame in 0..frames + 300 {
        update_scene(&mut scene, frame, workload);
        // Clone is excluded: app-side snapshot construction is measured by the
        // real window's scene profiler, not by this synthetic fixture.
        let next = scene.clone();
        let begin = Instant::now();
        renderer.prepare_scene(black_box(next));
        let elapsed = begin.elapsed().as_nanos() as u64;
        let plan = renderer.last_plan().context("missing plan")?;
        if frame >= 300 {
            samples.push(elapsed);
            commands += plan.commands.len();
        }
        black_box(plan);
        if let Some(dir) = &dump_dir
            && (matches!(frame, 0 | 299 | 300) || frame == frames + 299)
        {
            let mut plan = plan.clone();
            for command in &mut plan.commands {
                if let DrawCommand::RectBatch { cache, .. } = command {
                    *cache = None;
                }
            }
            std::fs::write(dir.join(format!("{frame}.txt")), format!("{plan:#?}"))?;
        }
        if let Some(hash) = &mut plan_hash {
            format!("{:?}", plan.clear).hash(hash);
            for command in &plan.commands {
                // Cache identities and batch boundaries may change without
                // changing the ordered primitives sent to the renderer.
                if let DrawCommand::RectBatch { rects, .. } = command {
                    for rect in rects.iter() {
                        format!("{:?}", DrawCommand::Rect { rect: rect.rect, color: rect.color })
                            .hash(hash);
                    }
                } else {
                    format!("{command:?}").hash(hash);
                }
            }
        }
    }
    samples.sort_unstable();
    println!(
        "{}",
        serde_json::json!({
            "scene": mode, "workload": workload, "stages": stages,
            "mean_us": samples.iter().sum::<u64>() as f64 / frames as f64 / 1000.0,
            "p50_us": samples[(frames * 50).div_ceil(100) - 1] as f64 / 1000.0,
            "p95_us": samples[(frames * 95).div_ceil(100) - 1] as f64 / 1000.0,
            "p99_us": samples[(frames * 99).div_ceil(100) - 1] as f64 / 1000.0,
            "mean_commands": commands as f64 / frames as f64,
            "plan_hash": plan_hash.map(|hash| format!("{:016x}", hash.finish())),
        })
    );
    Ok(())
}

fn make_scene(mode: &str, stages: usize) -> AppSceneSnapshot {
    if mode == "select" {
        let AppSceneSnapshot::Select(mut s) = bmz_render::sample::sample_select_scene() else {
            unreachable!()
        };
        let template = s.rows[0].clone();
        s.rows = (0..25)
            .map(|i| {
                let mut row = template.clone();
                row.index = i;
                row.title = format!("Sample 日本語 chart {i:02}");
                row.total_notes = 3000;
                row.ex_score = Some(5400);
                row.max_combo = Some(2400);
                row
            })
            .collect();
        s.chart_count = 1000;
        s.selected_index = 12;
        s.selected_chart_id = Some(13);
        s.selected_title = s.rows[12].title.clone();
        s.current_fps = 120;
        return AppSceneSnapshot::Select(s);
    }
    let AppSceneSnapshot::Result(mut s) = bmz_render::sample::sample_result_scene() else {
        unreachable!()
    };
    s.graph = Arc::new(make_graph(stages));
    s.duration_ms = 120_000 * stages as i32;
    s.total_notes = 3000 * stages as u32;
    s.ex_score = 5400 * stages as u32;
    s.ex_score_rate = 0.9;
    s.judge_counts.pgreat = 2400 * stages as u32;
    s.judge_counts.great = 600 * stages as u32;
    s.judge_counts.good = 0;
    s.judge_counts.bad = 0;
    s.judge_counts.poor = 0;
    s.current_fps = 120;
    if mode == "course" {
        s.title = format!("Sample {stages}-stage course");
        s.genre = "COURSE".into();
        s.course_result.stage_count = stages as u32;
        for i in 0..stages {
            s.course_titles[i] = format!("Stage {} 日本語", i + 1);
            s.course_result.stages[i] = CourseStageResultSkinSnapshot {
                ex_score: 5400,
                gauge: 84.0,
                bp: 0,
                rate_basis_points: 9000,
            };
        }
    }
    AppSceneSnapshot::Result(s)
}

fn make_graph(stages: usize) -> ResultGraphSnapshot {
    let mut g = ResultGraphSnapshot::default();
    for second in 0..120 * stages {
        g.judge_graph_buckets.push(ResultJudgeGraphBucket { values: [0, 20, 5, 0, 0, 0] });
        g.note_graph_buckets.push(ResultNoteGraphBucket { values: [0, 1, 2, 0, 0, 22, 0] });
        g.early_late_graph_buckets
            .push(ResultEarlyLateGraphBucket { values: [0, 20, 3, 0, 0, 0, 2, 0, 0, 0] });
        g.judge_graph_density.push(25);
        for half in 0..2 {
            for gauge_type in 0..6 {
                g.gauge_points.push(ResultGaugeGraphPoint {
                    time_ms: (second * 1000 + half * 500) as i32,
                    value: 60.0 + (second % 40) as f32,
                    max: 100.0,
                    border: 80.0,
                    gauge_type,
                    course_section_start: second % 120 == 0 && half == 0 && second > 0,
                });
            }
        }
        for note in 0..25 {
            let delta_ms = ((second * 17 + note * 7) % 81) as i64 - 40;
            let judge = if note < 20 { Judge::PGreat } else { Judge::Great };
            g.timing_points.push(ResultTimingPoint {
                time_ms: (second * 1000 + note * 40) as i32,
                delta_us: delta_ms * 1000,
                judge,
            });
            g.timing_distribution.add(delta_ms as i32);
        }
    }
    g.refresh_timing_metrics();
    g
}

fn update_scene(scene: &mut AppSceneSnapshot, frame: usize, workload: &str) {
    let elapsed = TimeUs(5_000_000 + frame as i64 * 1_000_000 / 120);
    match scene {
        AppSceneSnapshot::Select(s) => {
            s.time = elapsed;
            s.operating_time_ms = (elapsed.0 / 1000) as i32;
            s.selection_time = if workload == "scroll" {
                TimeUs((frame % 24) as i64 * 1_000_000 / 120)
            } else {
                elapsed
            };
            s.bar_scroll_direction = if workload == "scroll" { 1 } else { 0 };
            s.bar_scroll_progress =
                if workload == "scroll" { (frame % 24) as f32 / 24.0 } else { 0.0 };
            if workload == "panel" {
                s.option_panel = 1;
                s.option_panel_time = elapsed;
            }
        }
        AppSceneSnapshot::Result(s) => {
            s.elapsed_time = elapsed;
            s.result_panel = i32::from(workload == "panel");
        }
        _ => unreachable!(),
    }
}
