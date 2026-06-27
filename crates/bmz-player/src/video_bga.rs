use std::collections::{HashMap, HashSet};

use bmz_chart::model::{BgaAssetId, BgaAssetKind, BgaEventKind};
use bmz_core::judge::Judge;
use bmz_core::time::TimeUs;
use bmz_render::plan::TextureId;
use bmz_video::{DecodedFrame, VideoBgaDecoder, decode_first_frame};

use crate::audio::RunningPlaySession;
use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id, display_video_bga_frame};

pub struct ActiveVideoBgaDecoder {
    pub event_start_time: TimeUs,
    pub decoder: VideoBgaDecoder,
    pub last_pts: Option<i64>,
}

/// render_now 時刻でアクティブな動画BGAのテクスチャを更新する。
/// bga_frames カタログを更新して幅・高さも最新に保つ。
pub fn update_video_bga_frames(
    renderer: &mut bmz_render::renderer::Renderer,
    running: &mut RunningPlaySession,
    render_now: TimeUs,
) {
    if !running.session.bga_enabled || !running.session.chart.metadata.has_bga {
        return;
    }

    let RunningPlaySession { session, video_bga_decoders, failed_video_bga, bga_frames, .. } =
        running;
    let chart = &session.chart;

    // Base と Layer は BGA イベント時刻をビデオ開始時刻とする
    for kind in [BgaEventKind::Base, BgaEventKind::Layer, BgaEventKind::Layer2] {
        let Some(event) =
            chart.bga_events.iter().rev().find(|e| e.time <= render_now && e.kind == kind)
        else {
            continue;
        };

        let Some(asset) = chart.bga_assets.iter().find(|a| a.id == event.asset) else {
            continue;
        };
        if asset.kind != BgaAssetKind::Video {
            continue;
        }

        let video_offset_us = render_now.0 - event.time.0;
        update_single_video(
            renderer,
            video_bga_decoders,
            failed_video_bga,
            bga_frames,
            asset.id,
            &asset.path,
            event.time,
            video_offset_us,
        );
    }

    // Poor は直近の Bad/Poor 判定時刻をビデオ開始時刻とする
    let poor_duration_us = session.poor_bga_duration_us;
    if poor_duration_us > 0 {
        let judgement = session.recent_judgements.iter().rev().find(|j| {
            matches!(j.judge, Judge::Bad | Judge::Poor)
                && render_now.0 >= j.time.0
                && render_now.0 < j.time.0 + poor_duration_us
        });

        if let Some(judgement) = judgement {
            let judge_time = judgement.time;
            let poor_event = chart
                .bga_events
                .iter()
                .rev()
                .find(|e| e.time <= judge_time && e.kind == BgaEventKind::Poor);

            if let Some(event) = poor_event {
                let Some(asset) = chart.bga_assets.iter().find(|a| a.id == event.asset) else {
                    return;
                };
                if asset.kind == BgaAssetKind::Video {
                    let video_offset_us = render_now.0 - judge_time.0;
                    update_single_video(
                        renderer,
                        video_bga_decoders,
                        failed_video_bga,
                        bga_frames,
                        asset.id,
                        &asset.path,
                        judge_time,
                        video_offset_us,
                    );
                }
            }
        }
    }
}

fn update_single_video(
    renderer: &mut bmz_render::renderer::Renderer,
    video_bga_decoders: &mut VideoBgaDecoderMap,
    failed_video_bga: &mut HashSet<BgaAssetId>,
    bga_frames: &mut BgaFrameCatalog,
    asset_id: BgaAssetId,
    path: &std::path::Path,
    event_start_time: TimeUs,
    video_offset_us: i64,
) {
    if failed_video_bga.contains(&asset_id) {
        return;
    }

    // デコーダが未作成またはイベント開始時刻が変わっていたら新規作成
    let needs_new = match video_bga_decoders.get(&asset_id) {
        Some(active) => active.event_start_time != event_start_time,
        None => true,
    };

    if needs_new {
        let initial_frame = match decode_first_frame(path) {
            Ok(frame) => Some(frame),
            Err(error) => {
                tracing::debug!(
                    asset_id = asset_id.0,
                    path = %path.display(),
                    %error,
                    "failed to decode first video BGA frame before async playback"
                );
                None
            }
        };
        match VideoBgaDecoder::open(path) {
            Ok(decoder) => {
                video_bga_decoders.insert(
                    asset_id,
                    ActiveVideoBgaDecoder { event_start_time, decoder, last_pts: None },
                );
                tracing::info!(asset_id = asset_id.0, path = %path.display(), "opened video BGA decoder");
                if let Some(frame) = initial_frame
                    && upload_video_bga_frame(renderer, bga_frames, asset_id, &frame)
                    && let Some(active) = video_bga_decoders.get_mut(&asset_id)
                {
                    active.last_pts = Some(frame.pts_us);
                }
            }
            Err(e) => {
                tracing::warn!(asset_id = asset_id.0, %e, "failed to open video BGA; skipping");
                failed_video_bga.insert(asset_id);
                return;
            }
        }
    }

    let active = video_bga_decoders.get_mut(&asset_id).unwrap();
    if let Some(frame) = active.decoder.poll_frame(video_offset_us)
        && active.last_pts != Some(frame.pts_us)
    {
        let pts = frame.pts_us;
        if upload_video_bga_frame(renderer, bga_frames, asset_id, frame) {
            active.last_pts = Some(pts);
        }
    }
}

fn upload_video_bga_frame(
    renderer: &mut bmz_render::renderer::Renderer,
    bga_frames: &mut BgaFrameCatalog,
    asset_id: BgaAssetId,
    frame: &DecodedFrame,
) -> bool {
    let texture_id = TextureId(bga_texture_id(asset_id));
    match renderer.upsert_rgba_texture_ref(texture_id, frame.width, frame.height, &frame.rgba) {
        Ok(()) => {
            bga_frames
                .insert(asset_id, display_video_bga_frame(asset_id, frame.width, frame.height));
            true
        }
        Err(error) => {
            tracing::warn!(
                asset_id = asset_id.0,
                %error,
                "failed to upload video BGA frame"
            );
            false
        }
    }
}

pub type VideoBgaDecoderMap = HashMap<BgaAssetId, ActiveVideoBgaDecoder>;
