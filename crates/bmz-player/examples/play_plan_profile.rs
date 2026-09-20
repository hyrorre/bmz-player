//! CPU-only Play draw-plan probe. No window, GPU, audio, config writes, or score DB.
//! Usage: cargo run --release -p bmz-player --example play_plan_profile -- SKIN [PROFILE_TOML]
//! Reports synthetic 7K workloads; these timings are not application FPS.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use bmz_core::{lane::KeyMode, time::TimeUs};
use bmz_player::config::profile_config::ProfileConfig;
use bmz_player::skin_loader::{
    BeatorajaSkinDecodeRequest, SkinKind, decode_beatoraja_skin_request, set_decoded_skin_context,
};
use bmz_render::renderer::{RenderSurfaceStatus, Renderer};
use bmz_render::scene::AppSceneSnapshot;
use bmz_render::skin::{SkinDocumentTexture, default_skin_manifest};
use bmz_render::snapshot::{NoteVisualKind, VisibleNote};

fn main() -> Result<()> {
    ensure!(!cfg!(debug_assertions), "run this probe with --release");
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().context("usage: play_plan_profile SKIN [PROFILE_TOML]")?)
        .canonicalize()?;
    let profile = args
        .next()
        .map(|path| -> Result<ProfileConfig> {
            Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
        })
        .transpose()?;
    ensure!(args.next().is_none(), "unexpected extra argument");
    let dump_plans = std::env::var_os("BMZ_PLAN_DUMP").is_some();
    let empty = BTreeMap::new();
    let options = profile.as_ref().map_or(&empty, |profile| &profile.skin.play7_options);
    let files = profile.as_ref().map_or(&empty, |profile| &profile.skin.play7_files);
    let library_root = path
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "skins"))
        .unwrap_or_else(|| path.parent().expect("skin has a parent"))
        .to_path_buf();
    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        skin_path: &path,
        kind: SkinKind::Play,
        options,
        files,
        runtime_state: &Default::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &[library_root],
    })?;
    println!(
        "{}",
        serde_json::json!({
            "skin": path,
            "images": decoded.document.image.len(),
            "destinations": decoded.document.all_destinations(&decoded.document.enabled_options()).len(),
            "lua_runtime": decoded.lua_runtime.is_some(),
            "runtime_callbacks": decoded.lua_runtime.as_ref().map(|runtime| {
                (0..runtime.callback_count()).filter_map(|id| runtime.callback_path(id)).collect::<Vec<_>>()
            }),
            "warmup_frames": 300,
            "measured_frames": 3000,
        })
    );
    let textures = decoded
        .sources
        .iter()
        .map(|source| SkinDocumentTexture {
            source_id: source.source_id.clone(),
            texture: source.texture,
            source_size: source.size,
        })
        .collect();
    let mut renderer = Renderer::default();
    set_decoded_skin_context(
        &mut renderer,
        SkinKind::Play,
        default_skin_manifest(),
        decoded.document,
        decoded.lua_runtime,
        textures,
        false,
    );
    // Asset decode finishes before the timed loop. Only source IDs/sizes are needed.
    drop(decoded.sources);
    drop(decoded.fonts);
    drop(decoded.audio_assets);

    for notes_per_lane in [1, 8, 32] {
        // Re-enter Play so each workload starts with a fresh dynamic-timer state.
        renderer.render_scene_status(AppSceneSnapshot::Select(Default::default()))?;
        let AppSceneSnapshot::Play(mut snapshot) = bmz_render::sample::sample_play_scene() else {
            unreachable!()
        };
        snapshot.key_mode = KeyMode::K7;
        snapshot.resources_loaded = true;
        snapshot.resource_load_progress = 1.0;
        snapshot.duration = TimeUs(120_000_000);
        snapshot.total_notes = 2000;
        // Keep the sample's two LN/HCN bodies and two guide lines in every case.
        for notes in &mut snapshot.visible_notes {
            notes.clear();
        }
        for &lane in KeyMode::K7.active_lanes() {
            for index in 0..notes_per_lane {
                snapshot.visible_notes[lane.index()].push(VisibleNote {
                    lane,
                    time: TimeUs(10_000_000 + index as i64 * 1000),
                    y: (index as f32 + 0.5) / notes_per_lane as f32,
                    alpha: 1.0,
                    kind: NoteVisualKind::Tap,
                    processed_judge: None,
                });
            }
        }
        let mut samples = Vec::with_capacity(3000);
        let mut commands = 0;
        for frame in 0..3300 {
            snapshot.time = TimeUs(10_000_000 + frame * 1000);
            snapshot.play_elapsed_time = snapshot.time;
            snapshot.operating_time_ms = (snapshot.time.0 / 1000) as i32;
            snapshot.combo = (frame / 10) as u32;
            snapshot.ex_score = snapshot.combo * 2;
            snapshot.past_notes = snapshot.combo;
            snapshot.keyon_ms.fill(Some((frame % 150) as i32));
            for judgement in &mut snapshot.recent_judgements {
                judgement.time = TimeUs(snapshot.time.0 - (frame % 150) * 1000);
            }
            // plan_us excludes the snapshot clone and previous-plan destruction.
            let status = renderer
                .render_scene_status(black_box(AppSceneSnapshot::Play(snapshot.clone())))?;
            ensure!(status == RenderSurfaceStatus::SkippedNoSurface, "unexpected GPU rendering");
            let timings = renderer.last_frame_timings().context("missing plan timings")?;
            black_box(renderer.last_plan());
            if frame >= 300 {
                samples.push(timings.plan_us as u64);
                commands += timings.commands;
            }
            if dump_plans && [300, 1500, 3299].contains(&frame) {
                println!(
                    "{}",
                    serde_json::json!({
                        "visible_taps": notes_per_lane * KeyMode::K7.active_lanes().len(),
                        "frame": frame,
                        "plan": format!("{:?}", renderer.last_plan()),
                    })
                );
            }
        }
        samples.sort_unstable();
        let percentile = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
        println!(
            "{}",
            serde_json::json!({
                "visible_taps": notes_per_lane * KeyMode::K7.active_lanes().len(),
                "long_notes": snapshot.visible_long_notes.len(),
                "plan_mean_us": samples.iter().sum::<u64>() as f64 / samples.len() as f64,
                "plan_p50_us": percentile(50),
                "plan_p95_us": percentile(95),
                "plan_p99_us": percentile(99),
                "plan_max_us": samples.last(),
                "mean_commands": commands as f64 / samples.len() as f64,
            })
        );
    }
    Ok(())
}
