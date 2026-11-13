use std::collections::{HashMap, HashSet, hash_map::Entry};

use bmz_chart::model::{BgaAssetId, BgaAssetKind, BgaEventKind};
use bmz_core::judge::Judge;
use bmz_core::time::TimeUs;
use bmz_render::plan::TextureId;
use bmz_video::{DecodedFrame, DecodedPixelFormat, VideoBgaDecoder};

use crate::audio::RunningPlaySession;
use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id, display_video_bga_frame};

pub struct ActiveVideoBgaDecoder {
    pub event_start_time: TimeUs,
    pub decoder: VideoBgaDecoder,
    pub last_pts: Option<i64>,
    last_used: std::time::Instant,
    canvas_size: [u32; 2],
    sample_size: [u32; 2],
}

pub(crate) fn update_video_bga_sample_sizes(
    renderer: &bmz_render::renderer::Renderer,
    decoders: &mut VideoBgaDecoderMap,
) {
    if decoders.is_empty() {
        return;
    }
    let canvas_size = renderer.play_canvas_size();
    for (texture, size) in renderer.play_video_sample_sizes().into_iter().flatten() {
        let id = BgaAssetId(texture.0 - bmz_render::plan::CHART_BGA_TEXTURE_BASE);
        if let Some(active) = decoders.get_mut(&id)
            && active.canvas_size == canvas_size
            && active.sample_size != size
        {
            active.sample_size = size;
            active.decoder.set_playback_canvas_size(size);
        }
    }
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

/// A reused decoder must bind to the first event of the new playback position.
pub const REUSED_VIDEO_EVENT_START: TimeUs = TimeUs(i64::MIN);

// A normal event can first be observed a few frames late. Decode from the start
// in that case: some demuxers can seek to a keyframe after a near-zero target.
const MIN_DIRECT_SEEK_OFFSET_US: i64 = 250_000;

fn video_seek_offset_us(video_offset_us: i64) -> i64 {
    if video_offset_us < MIN_DIRECT_SEEK_OFFSET_US { 0 } else { video_offset_us }
}

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
    cache: &crate::screens::play_snapshot::PlayRenderSnapshotCache,
) -> Vec<ActiveBgaChannel> {
    let mut channels = Vec::with_capacity(4);
    for kind in [BgaEventKind::Base, BgaEventKind::Layer, BgaEventKind::Layer2] {
        if let Some(event) = cache.bga_event_at(kind, chart_now)
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
        && let Some(event) = cache.bga_event_at(BgaEventKind::Poor, judgement.time)
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
        trim_video_decoders(
            &mut running.video_bga_decoders,
            &HashSet::new(),
            &running.gameplay.session.chart,
        );
        return;
    }

    let RunningPlaySession {
        gameplay,
        video_bga_decoders,
        failed_video_bga,
        bga_frames,
        render_snapshot_cache,
        ..
    } = running;
    let session = &gameplay.session;
    let chart = &session.chart;
    let canvas_size = renderer.play_canvas_size();
    let mut active_video_assets = HashSet::new();

    for channel in active_bga_channels(session, chart_now, render_snapshot_cache) {
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
            canvas_size,
        );
    }

    trim_video_decoders(video_bga_decoders, &active_video_assets, chart);
}

/// Inactive video decoders kept for re-display and quick retry. Covers charts
/// that switch among about ten videos per play without reopening; the cap bounds
/// idle decode threads and queued RGBA frames for charts with dozens of videos.
const WARM_VIDEO_DECODERS: usize = 10;

fn most_recent_warm_assets(mut inactive: Vec<(BgaAssetId, std::time::Instant)>) -> Vec<BgaAssetId> {
    inactive.sort_unstable_by_key(|(id, used)| (std::cmp::Reverse(*used), id.0));
    inactive.truncate(WARM_VIDEO_DECODERS);
    inactive.into_iter().map(|(id, _)| id).collect()
}

fn trim_video_decoders(
    decoders: &mut VideoBgaDecoderMap,
    active: &HashSet<BgaAssetId>,
    chart: &bmz_chart::model::PlayableChart,
) {
    let warm = most_recent_warm_assets(
        decoders
            .iter()
            .filter_map(|(&id, decoder)| {
                (!active.contains(&id)
                    && chart
                        .bga_assets
                        .iter()
                        .any(|asset| asset.id == id && asset.kind == BgaAssetKind::Video))
                .then_some((id, decoder.last_used))
            })
            .collect(),
    );
    decoders.retain(|id, _| active.contains(id) || warm.contains(id));
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
    canvas_size: [u32; 2],
) {
    if failed_video_bga.contains(&asset_id) {
        return;
    }

    let now = std::time::Instant::now();
    let active = match video_bga_decoders.entry(asset_id) {
        Entry::Occupied(entry) if entry.get().decoder.path() == path => {
            let active = entry.into_mut();
            if canvas_size != active.canvas_size && !canvas_size.contains(&0) {
                active.decoder.set_playback_canvas_size(canvas_size);
                active.canvas_size = canvas_size;
                active.sample_size = canvas_size;
            }
            if active.event_start_time != event_start_time {
                active.decoder.restart_at(video_seek_offset_us(video_offset_us));
                active.event_start_time = event_start_time;
                active.last_pts = None;
            }
            active
        }
        entry => match open_video_decoder_at(path, video_offset_us, canvas_size) {
            Ok(decoder) => {
                tracing::info!(asset_id = asset_id.0, path = %path.display(), ?canvas_size,
                    "opened video BGA decoder");
                entry
                    .insert_entry(ActiveVideoBgaDecoder {
                        event_start_time,
                        decoder,
                        last_pts: None,
                        last_used: now,
                        canvas_size,
                        sample_size: canvas_size,
                    })
                    .into_mut()
            }
            Err(error) => {
                tracing::warn!(asset_id = asset_id.0, %error, "failed to open video BGA; skipping");
                failed_video_bga.insert(asset_id);
                if let Entry::Occupied(entry) = entry {
                    entry.remove();
                }
                return;
            }
        },
    };
    active.last_used = now;
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
    canvas_size: [u32; 2],
) -> anyhow::Result<VideoBgaDecoder> {
    let mut decoder = if canvas_size.contains(&0) {
        VideoBgaDecoder::open_for_playback(path, canvas_size)?
    } else {
        VideoBgaDecoder::open_for_gpu_playback(path, canvas_size)?
    };
    let seek_offset_us = video_seek_offset_us(video_offset_us);
    if seek_offset_us > 0 {
        decoder.restart_at(seek_offset_us);
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
    let upload = match frame.format {
        DecodedPixelFormat::Yuv420 => {
            renderer.upsert_yuv420_texture(texture_id, frame.width, frame.height, &frame.pixels)
        }
        DecodedPixelFormat::Rgb24 => {
            renderer.upsert_rgb24_texture(texture_id, frame.width, frame.height, &frame.pixels)
        }
        DecodedPixelFormat::Rgba => {
            renderer.upsert_rgba_texture_ref(texture_id, frame.width, frame.height, &frame.pixels)
        }
    };
    match upload {
        Ok(()) => {
            let [width, height] = frame.source_size;
            bga_frames.insert(asset_id, display_video_bga_frame(asset_id, width, height));
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

/// Rebind reused decoders on first activation after a retry or Viewer seek.
pub fn prepare_reused_video_decoders(decoders: &mut VideoBgaDecoderMap) {
    for active in decoders.values_mut() {
        active.event_start_time = REUSED_VIDEO_EVENT_START;
        active.last_pts = None;
    }
}

pub use prepare_reused_video_decoders as prepare_reused_video_decoders_for_seek;

pub type VideoBgaDecoderMap = HashMap<BgaAssetId, ActiveVideoBgaDecoder>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_bga_activation_decodes_from_the_start() {
        assert_eq!(video_seek_offset_us(0), 0);
        assert_eq!(video_seek_offset_us(30_000), 0);
        assert_eq!(video_seek_offset_us(MIN_DIRECT_SEEK_OFFSET_US - 1), 0);
        assert_eq!(video_seek_offset_us(MIN_DIRECT_SEEK_OFFSET_US), MIN_DIRECT_SEEK_OFFSET_US);
        assert_eq!(video_seek_offset_us(10_000_000), 10_000_000);
    }

    #[test]
    fn only_most_recent_inactive_videos_stay_warm() {
        let now = std::time::Instant::now();
        let warm = most_recent_warm_assets(
            (0..32)
                .map(|i| (BgaAssetId(i), now + std::time::Duration::from_millis(i as u64)))
                .collect(),
        );
        assert_eq!(warm, (22..32).rev().map(BgaAssetId).collect::<Vec<_>>());
    }
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

    #[test]
    fn scaled_video_upload_keeps_original_geometry_in_the_catalog() {
        let mut renderer = bmz_render::renderer::Renderer::default();
        let mut frames = BgaFrameCatalog::new();
        let id = BgaAssetId(7);
        let frame = DecodedFrame {
            pts_us: 0,
            pixels: vec![0; 8],
            width: 2,
            height: 1,
            source_size: [3840, 1920],
            format: DecodedPixelFormat::Rgba,
        };
        assert!(upload_video_bga_frame(&mut renderer, &mut frames, id, &frame));
        assert_eq!(frames[&id], display_video_bga_frame(id, 3840, 1920));
    }
}
