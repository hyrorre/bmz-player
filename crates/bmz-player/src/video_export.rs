//! Deterministic, device-independent Play scene video generation.
use crate::{cli::VideoExportOptions, paths::AppPaths};
use anyhow::{Context, Result, ensure};
use bmz_core::time::TimeUs;
use bmz_render::{
    renderer::{Renderer, SurfaceSize},
    scene::AppSceneSnapshot,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

mod encoder;
mod media;
mod prepare;
mod simulation;
#[cfg(test)]
mod tests;

fn random_id() -> Result<uuid::Uuid> {
    let mut bytes = [0; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| anyhow::anyhow!("random source failed: {error}"))?;
    Ok(uuid::Uuid::from_bytes(bytes))
}

pub fn run(options: VideoExportOptions, paths: &AppPaths, profile_id: Option<&str>) -> Result<()> {
    ensure!(
        options.overwrite
            || (!options.output.exists() && !options.output.with_extension("export.json").exists()),
        "output already exists; use --overwrite"
    );
    let ffmpeg_version = encoder::preflight(&options)?;
    crate::stdio::stderr_line(format_args!("Preparing offline Play export..."));
    let prepared = prepare::prepare(&options, paths, profile_id)?;
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: options.width, height: options.height })?;
    let manifest =
        crate::skin_loader::load_default_skin_into_renderer_from_paths(&mut renderer, paths)?;
    let selection = crate::skin_loader::play_skin_selection_for_session(
        &prepared.profile.skin,
        prepared.play.session.chart.metadata.key_mode,
        if prepared.replay.is_some() {
            crate::select_options::SessionMode::Normal
        } else {
            crate::select_options::SessionMode::Autoplay
        },
    );
    let skin_path = if selection.path.is_empty() {
        crate::skin_loader::default_play_skin_document_path_from_paths(paths, selection.key_mode)
    } else {
        paths.resolve_path_ref(selection.path)?
    };
    let runtime_state =
        crate::app::offline_skin_load_state(&prepared.play, &prepared.profile, prepared.best);
    let mut decoded = crate::skin_loader::decode_beatoraja_skin_request(
        crate::skin_loader::BeatorajaSkinDecodeRequest {
            skin_path: &skin_path,
            kind: crate::skin_loader::SkinKind::Play,
            options: selection.options,
            files: selection.files,
            runtime_state: &runtime_state,
            library_roots: &paths.skin_library_roots(),
            document_cache: None,
            source_cache: None,
            texture_cache: None,
            font_cache: None,
            installed_fonts: None,
        },
    )?;
    let mut media = media::Media::load(
        &prepared.play.session.chart,
        &options.chart.canonicalize()?,
        &decoded,
        &mut renderer,
    )?;
    let system_handle =
        bmz_audio::command::AudioEngineHandle::new(bmz_audio::engine::AudioEngine::new(48_000));
    let skin_audio = crate::skin_audio::SkinAudioRuntime::install(
        system_handle.clone(),
        &decoded.document,
        std::mem::take(&mut decoded.audio_assets),
    );
    let timing = simulation::SceneTiming::from_document(&decoded.document);
    crate::skin_loader::install_decoded_skin(&mut renderer, decoded, manifest)?;
    let fadeout_ms = renderer
        .play_skin_document()
        .map_or(0, |d| d.fadeout)
        .max(renderer.play_skin_timer_animation_duration_ms(2));
    let fadeout_ms = if fadeout_ms <= 0 {
        bmz_render::snapshot::DEFAULT_PLAY_FADEOUT_DURATION_MS
    } else {
        fadeout_ms
    };
    let fc_ms = renderer
        .play_skin_timer_animation_duration_ms(48)
        .max(renderer.play_skin_timer_animation_duration_ms(49));
    let mut metadata = serde_json::json!({
        "format_version": 1, "bmz_version": env!("CARGO_PKG_VERSION"), "ffmpeg": ffmpeg_version,
        "options": options, "profile_id": prepared.profile.id, "skin": skin_path,
        "play_settings": prepared.profile.play, "judge_settings": prepared.profile.judge,
        "lane_settings": prepared.profile.lane, "audio_settings": prepared.profile.audio_mix,
        "skin_settings": prepared.profile.skin,
        "chart_sha256": crate::storage::common::hash_to_hex(&prepared.play.session.chart.identity.file_sha256),
        "random_seed": prepared.seed, "replay": prepared.replay,
        "applied_arrange": {
            "p1": prepared.play.applied_arrange.arrange.as_str(), "p2": prepared.play.applied_arrange.arrange_2p.as_str(),
            "seed_p1": prepared.play.applied_arrange.seed, "seed_p2": prepared.play.applied_arrange.seed_2p,
            "bms_random_choices": prepared.play.applied_arrange.bms_random_choices,
            "bms_switch_choices": prepared.play.applied_arrange.bms_switch_choices,
            "lane_pattern": prepared.play.applied_arrange.pattern,
        },
        "interval": "play_skin_entry_through_exit", "sample_rate": 48000,
        "physical_input_offset_us": 0,
    });
    let mut simulation = simulation::Simulation::new(
        prepared,
        timing,
        fadeout_ms,
        fc_ms,
        system_handle,
        skin_audio,
        paths,
    )?;
    let mut encoder = encoder::Encoder::new(&options)?;
    let started = Instant::now();
    let mut last_progress = Instant::now();
    let mut frame = 0;
    loop {
        let scene_us = options.fps.time_us(frame);
        simulation.advance_to(options.fps.samples(frame), |pcm| encoder.audio(pcm))?;
        if simulation.exit_us.is_some_and(|end| scene_us >= end) && frame > 0 {
            break;
        }
        let mut snapshot = simulation.snapshot(scene_us, &media.catalog);
        snapshot.current_fps = options.fps.numerator / options.fps.denominator;
        let chart_us = snapshot.time.0;
        media.update(
            &simulation.prepared.play.session.chart,
            &mut snapshot,
            &mut renderer,
            chart_us,
            scene_us,
        )?;
        renderer.render_scene(AppSceneSnapshot::Play(snapshot))?;
        simulation.acknowledge_frame();
        encoder.frame(&renderer.read_offscreen_rgba()?)?;
        frame += 1;
        if last_progress.elapsed().as_secs() >= 1 {
            let seconds = scene_us as f64 / 1_000_000.0;
            let speed = seconds / started.elapsed().as_secs_f64().max(0.001);
            let remaining = (simulation.estimated_exit_us() as f64 / 1_000_000.0 - seconds)
                .max(0.0)
                / speed.max(0.001);
            crate::stdio::stderr_line(format_args!(
                "{frame} frames | {seconds:.2}s | {speed:.2}x | ETA {remaining:.0}s"
            ));
            last_progress = Instant::now();
        }
    }
    metadata["frames"] = frame.into();
    metadata["audio_samples"] = simulation.cursor.into();
    metadata["scene_exit_us"] = simulation.exit_us.into();
    metadata["ex_score"] = simulation.prepared.play.session.score.ex_score().into();
    metadata["judge_counts"] =
        serde_json::json!(format!("{:?}", simulation.prepared.play.session.score));
    encoder.finish(&options, &metadata)?;
    crate::stdio::stderr_line(format_args!(
        "Exported {} ({frame} frames)",
        options.output.display()
    ));
    Ok(())
}
