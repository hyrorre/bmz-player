use std::collections::{HashMap, HashSet};

use bmz_chart::model::{BgaAssetId, BgaAssetKind, BgaEventKind};
use bmz_core::judge::Judge;
use bmz_core::time::TimeUs;
use bmz_render::plan::TextureId;
use bmz_video::{DecodedFrame, VideoBgaDecoder};

use crate::audio::RunningPlaySession;
use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id, display_video_bga_frame};

pub struct ActiveVideoBgaDecoder {
    pub event_start_time: TimeUs,
    pub decoder: VideoBgaDecoder,
    pub last_pts: Option<i64>,
}

/// Resolve an immutable BGA timeline selection against renderer-owned assets.
/// This also handles textures that finished loading after gameplay started.
pub fn resolve_snapshot_textures(
    snapshot: &mut bmz_render::snapshot::RenderSnapshot,
    frames: &BgaFrameCatalog,
) {
    for selected in [
        &mut snapshot.bga_base,
        &mut snapshot.bga_layer,
        &mut snapshot.bga_layer2,
        &mut snapshot.bga_poor,
    ] {
        if let Some(frame) = selected {
            if let Some(loaded) =
                frames.values().find(|loaded| loaded.texture_id == frame.texture_id)
            {
                frame.width = loaded.width;
                frame.height = loaded.height;
                frame.is_video = loaded.is_video;
            } else {
                *selected = None;
            }
        }
    }
}

/// Sentinel so a reused decoder is treated as needing a fresh event binding (and
/// `restart`) on the first activation after install / quick retry.
pub const REUSED_VIDEO_EVENT_START: TimeUs = TimeUs(i64::MIN);

/// BGA asset on screen and the chart time its playback started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveBgaChannel {
    pub kind: BgaEventKind,
    pub asset_id: BgaAssetId,
    pub origin: TimeUs,
}

/// Assets shown at `chart_now`, in Base / Layer / Layer2 / Poor order.
/// Base と Layer は BGA イベント時刻、Poor は直近の Bad/Poor 判定時刻を開始時刻とする。
pub fn active_bga_channels(
    session: &crate::gameplay_runtime::PlaySessionObservation,
    chart_now: TimeUs,
) -> Vec<ActiveBgaChannel> {
    let chart = &session.chart;
    let mut channels = Vec::with_capacity(4);
    for kind in [BgaEventKind::Base, BgaEventKind::Layer, BgaEventKind::Layer2] {
        if let Some(event) =
            chart.bga_events.iter().rev().find(|e| e.time <= chart_now && e.kind == kind)
            && let Some(asset_id) = event.asset
        {
            channels.push(ActiveBgaChannel { kind, asset_id, origin: event.time });
        }
    }

    let poor_duration_us = session.poor_bga_duration_us;
    if poor_duration_us > 0
        && let Some(judgement) = session.recent_judgements.iter().rev().find(|j| {
            matches!(j.judge, Judge::Bad | Judge::Poor)
                && chart_now.0 >= j.time.0
                && chart_now.0 < j.time.0 + poor_duration_us
        })
        && let Some(event) = chart
            .bga_events
            .iter()
            .rev()
            .find(|e| e.time <= judgement.time && e.kind == BgaEventKind::Poor)
        && let Some(asset_id) = event.asset
    {
        channels.push(ActiveBgaChannel {
            kind: BgaEventKind::Poor,
            asset_id,
            origin: judgement.time,
        });
    }
    channels
}

/// 表示オフセットを含まない chart_now 時刻でアクティブな動画BGAのテクスチャを更新する。
/// bga_frames カタログを更新して幅・高さも最新に保つ。
pub fn update_video_bga_frames(
    renderer: &mut bmz_render::renderer::Renderer,
    running: &mut RunningPlaySession,
    chart_now: TimeUs,
) {
    if !running.session.bga_enabled || !running.session.chart.metadata.has_bga {
        // Keep warm decoders across BGA-disabled stretches so quick retry can reuse them.
        return;
    }

    let RunningPlaySession { gameplay, video_bga_decoders, failed_video_bga, bga_frames, .. } =
        running;
    let session = &gameplay.session;
    let chart = &session.chart;
    let mut active_video_assets = HashSet::new();

    for channel in active_bga_channels(session, chart_now) {
        let Some(asset) = chart.bga_assets.iter().find(|a| a.id == channel.asset_id) else {
            continue;
        };
        if asset.kind != BgaAssetKind::Video {
            continue;
        }
        active_video_assets.insert(channel.asset_id);
        update_single_video(
            renderer,
            video_bga_decoders,
            failed_video_bga,
            bga_frames,
            channel.asset_id,
            &asset.path,
            channel.origin,
            chart_now.0 - channel.origin.0,
        );
    }

    // Drop only decoders that are no longer part of this chart's video assets.
    // Inactive-but-still-chart videos stay warm for reuse / quick retry (beatoraja style).
    video_bga_decoders.retain(|asset_id, _| {
        active_video_assets.contains(asset_id)
            || chart
                .bga_assets
                .iter()
                .any(|asset| asset.id == *asset_id && asset.kind == BgaAssetKind::Video)
    });
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

    // デコーダが未作成またはイベント開始時刻が変わっていたら reuse/restart または新規作成
    let needs_new = match video_bga_decoders.get(&asset_id) {
        Some(active) => active.event_start_time != event_start_time,
        None => true,
    };

    if needs_new {
        if let Some(active) = video_bga_decoders.get_mut(&asset_id) {
            // Same path reuse: seek directly to the current event offset instead of reopening.
            if active.decoder.path() == path {
                active.decoder.restart_at(video_offset_us);
                active.event_start_time = event_start_time;
                active.last_pts = None;
            } else {
                match open_video_decoder_at(path, video_offset_us) {
                    Ok(decoder) => {
                        *active =
                            ActiveVideoBgaDecoder { event_start_time, decoder, last_pts: None };
                        tracing::info!(
                            asset_id = asset_id.0,
                            path = %path.display(),
                            "opened video BGA decoder"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            asset_id = asset_id.0,
                            %e,
                            "failed to open video BGA; skipping"
                        );
                        failed_video_bga.insert(asset_id);
                        video_bga_decoders.remove(&asset_id);
                        return;
                    }
                }
            }
        } else {
            match open_video_decoder_at(path, video_offset_us) {
                Ok(decoder) => {
                    video_bga_decoders.insert(
                        asset_id,
                        ActiveVideoBgaDecoder { event_start_time, decoder, last_pts: None },
                    );
                    tracing::info!(
                        asset_id = asset_id.0,
                        path = %path.display(),
                        "opened video BGA decoder"
                    );
                }
                Err(e) => {
                    tracing::warn!(asset_id = asset_id.0, %e, "failed to open video BGA; skipping");
                    failed_video_bga.insert(asset_id);
                    return;
                }
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

fn open_video_decoder_at(
    path: &std::path::Path,
    video_offset_us: i64,
) -> anyhow::Result<VideoBgaDecoder> {
    let mut decoder = VideoBgaDecoder::open(path)?;
    if video_offset_us > 0 {
        decoder.restart_at(video_offset_us);
    }
    Ok(decoder)
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

/// Prepare reused video decoders for a new play session (seek to start, clear PTS).
pub fn prepare_reused_video_decoders(decoders: &mut VideoBgaDecoderMap) {
    for active in decoders.values_mut() {
        active.decoder.restart();
        active.event_start_time = REUSED_VIDEO_EVENT_START;
        active.last_pts = None;
    }
}

/// Mark reused decoders for Viewer seek without first rewinding them to zero.
/// The first active BGA update supplies the exact event-relative target to `restart_at`.
pub fn prepare_reused_video_decoders_for_seek(decoders: &mut VideoBgaDecoderMap) {
    for active in decoders.values_mut() {
        active.event_start_time = REUSED_VIDEO_EVENT_START;
        active.last_pts = None;
    }
}

pub type VideoBgaDecoderMap = HashMap<BgaAssetId, ActiveVideoBgaDecoder>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_bga_selection_resolves_late_video_upload_and_preserves_tint() {
        let mut selected = display_video_bga_frame(BgaAssetId(7), 1, 1);
        selected.tint_a = 0.5;
        let mut snapshot =
            bmz_render::snapshot::RenderSnapshot { bga_base: Some(selected), ..Default::default() };
        resolve_snapshot_textures(&mut snapshot, &BgaFrameCatalog::new());
        assert!(snapshot.bga_base.is_none());
        snapshot.bga_base = Some(selected);
        let frames = BgaFrameCatalog::from([(
            BgaAssetId(7),
            display_video_bga_frame(BgaAssetId(7), 1920, 1080),
        )]);
        resolve_snapshot_textures(&mut snapshot, &frames);
        let frame = snapshot.bga_base.unwrap();
        assert_eq!((frame.width, frame.height, frame.tint_a), (1920.0, 1080.0, 0.5));
        assert!(frame.is_video);
    }
}
