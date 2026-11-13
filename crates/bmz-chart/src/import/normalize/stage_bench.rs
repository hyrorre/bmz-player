//! Per-stage timing of `normalize_chart` on real charts, single-threaded.
//!
//! ```text
//! BMZ_IMPORT_BENCH_DIR=G:/BMS/EVENTS \
//!   cargo test -p bmz-chart --release --lib -- --ignored --nocapture normalize_stage_timing
//! ```
//!
//! The stages mirror `pipeline::normalize_chart`; keep them in step when it changes.

use std::time::{Duration, Instant};

use super::*;

fn chart_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            chart_files(&path, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "bms" | "bme" | "bml"))
        {
            out.push(path);
        }
    }
}

/// `BMZ_IMPORT_BENCH_ASSETS=declared` measures the library-scan lookup.
fn bench_lookup() -> AssetLookup {
    if std::env::var("BMZ_IMPORT_BENCH_ASSETS").is_ok_and(|value| value == "declared") {
        AssetLookup::Declared
    } else {
        AssetLookup::Resolve { warn_missing: false }
    }
}

#[test]
#[ignore = "needs real charts in BMZ_IMPORT_BENCH_DIR"]
fn normalize_stage_timing() {
    let Some(dir) = std::env::var_os("BMZ_IMPORT_BENCH_DIR") else {
        eprintln!("skipping: BMZ_IMPORT_BENCH_DIR is not set");
        return;
    };
    let limit = std::env::var("BMZ_IMPORT_BENCH_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(400_usize);
    let mut files = Vec::new();
    chart_files(Path::new(&dir), &mut files);
    files.sort();
    files.truncate(limit);

    let mut stages: Vec<(&'static str, Duration)> = Vec::new();
    let mut add = |name: &'static str, elapsed: Duration| match stages
        .iter_mut()
        .find(|(stage, _)| *stage == name)
    {
        Some((_, total)) => *total += elapsed,
        None => stages.push((name, elapsed)),
    };
    let mut measured = 0_usize;
    for path in &files {
        let bytes = std::fs::read(path).unwrap();
        let mut warnings = Vec::new();
        let Ok(intermediate) = super::super::bms_rs_adapter::import_bms_bytes_to_intermediate(
            path,
            &bytes,
            &super::super::BmsRandomSource::Seed(None),
            &mut Vec::new(),
            &mut Vec::new(),
            &mut warnings,
        ) else {
            continue;
        };
        measured += 1;
        macro_rules! timed {
            ($name:literal, $expr:expr) => {{
                let started = Instant::now();
                let value = $expr;
                add($name, started.elapsed());
                value
            }};
        }
        let metadata = timed!("metadata", pipeline::normalize_metadata(&intermediate.metadata));
        let sound_table = timed!(
            "sound table",
            build_sound_table(path, &intermediate, &mut warnings, bench_lookup())
        );
        let bga_table = timed!(
            "bga table",
            build_bga_table(path, &intermediate, &mut warnings, bench_lookup())
        );
        let Ok(tick_objects) = timed!("tick objects", materialize_tick_objects(&intermediate))
        else {
            continue;
        };
        let Ok(tick_timing_events) =
            timed!("timing events", collect_timing_events(&intermediate, &mut warnings))
        else {
            continue;
        };
        let timing_map = timed!(
            "timing map",
            build_timing_map_with_tick_scale(
                intermediate.metadata.initial_bpm.max(1.0),
                tick_timing_events.clone(),
                IMPORT_TICK_SCALE,
            )
        );
        let mut draft = PlayableChartDraft::new(
            intermediate.identity.clone(),
            metadata,
            sound_table.assets.clone(),
            bga_table.assets.clone(),
        );
        let lane_buckets = timed!("lane buckets", collect_lane_objects(&tick_objects, &timing_map));
        let mut next_note_id = 0_u32;
        for lane in Lane::ALL {
            let resolved = timed!(
                "long notes",
                normalize_lane_objects_with_markers(
                    lane,
                    &lane_buckets[lane.index()],
                    intermediate.lnobj_wav_key,
                    &intermediate.typed_lnobj,
                    &mut warnings,
                )
            );
            timed!(
                "lane events",
                emit_resolved_lane_events(
                    lane,
                    resolved,
                    &sound_table,
                    &mut draft,
                    &mut next_note_id,
                    &mut warnings,
                )
            );
        }
        let _ = timed!(
            "layered sounds",
            apply_layered_note_sounds(
                &intermediate.layered_note_sounds,
                &intermediate.measures,
                &sound_table,
                &mut draft,
                &mut warnings,
                false,
            )
        );
        let _ = timed!(
            "long end sounds",
            apply_layered_note_sounds(
                &intermediate.long_end_sounds,
                &intermediate.measures,
                &sound_table,
                &mut draft,
                &mut warnings,
                true,
            )
        );
        draft.bgm_events = timed!(
            "bgm events",
            build_bgm_events(&tick_objects, &timing_map, &sound_table, &mut warnings)
        );
        draft.bga_events = timed!(
            "bga events",
            build_bga_events(&tick_objects, &timing_map, &bga_table, &mut warnings)
        );
        draft.timing_events = timed!(
            "timing events out",
            build_timing_events(
                intermediate.metadata.initial_bpm.max(1.0),
                &tick_timing_events,
                &timing_map,
            )
        );
        timed!("other events", {
            draft.scroll_events = build_scroll_events(&intermediate, &timing_map).unwrap();
            draft.speed_events = build_speed_events(&intermediate, &timing_map).unwrap();
            draft.judge_rank_events = build_judge_rank_events(&intermediate, &timing_map).unwrap();
            draft.bgm_volume_events =
                build_chart_volume_events(&intermediate, &timing_map, true).unwrap();
            draft.key_volume_events =
                build_chart_volume_events(&intermediate, &timing_map, false).unwrap();
            draft.text_events = build_text_events(&intermediate, &timing_map).unwrap();
            draft.bga_opacity_events =
                build_bga_opacity_events(&intermediate, &timing_map).unwrap();
            draft.bga_argb_events = build_bga_argb_events(&intermediate, &timing_map).unwrap();
            draft.swbga_definitions = build_swbga_definitions(&intermediate);
            draft.bga_keybound_events =
                build_bga_keybound_events(&intermediate, &timing_map).unwrap();
        });
        draft.bar_lines = timed!("bar lines", build_bar_lines(&intermediate.measures, &timing_map));
        timed!("compress ticks", compress_import_ticks(&mut draft));
        let _ = timed!("finalize", finalize_playable_chart(draft));
    }

    let total: Duration = stages.iter().map(|(_, elapsed)| *elapsed).sum();
    stages.sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));
    eprintln!(
        "{measured} charts; normalize {:.2} ms/chart",
        total.as_secs_f64() * 1000.0 / measured as f64
    );
    for (name, elapsed) in &stages {
        eprintln!(
            "{name:>18}: {:8.3} ms/chart {:5.1}%",
            elapsed.as_secs_f64() * 1000.0 / measured as f64,
            elapsed.as_secs_f64() * 100.0 / total.as_secs_f64(),
        );
    }
}
