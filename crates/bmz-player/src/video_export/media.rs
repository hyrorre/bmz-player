use super::*;
use crate::screens::play_snapshot::{
    BgaFrameCatalog, bga_texture_id, display_bga_frame, display_video_bga_frame,
};
use bmz_chart::model::{BgaAssetId, BgaAssetKind, BgaEventKind, PlayableChart};
use bmz_render::{plan::TextureId, snapshot::RenderSnapshot};
use bmz_video::VideoBgaDecoder;
use std::collections::HashMap;

pub struct Media {
    pub catalog: BgaFrameCatalog,
    videos: HashMap<BgaAssetId, PathBuf>,
    active_bga: [Option<(BgaAssetId, i64, VideoBgaDecoder)>; 4],
    output_texture_base: u32,
    stagefile_size: Option<bmz_render::skin::SkinImageSize>,
    backbmp: bool,
    skin_videos: Vec<SkinVideo>,
    document: bmz_render::skin::SkinDocument,
}

struct SkinVideo {
    texture: TextureId,
    path: PathBuf,
    period: i64,
    active: bool,
    ops: Vec<Vec<i32>>,
    origin: Option<i64>,
    cycle: i64,
    decoder: Option<VideoBgaDecoder>,
}

impl Media {
    pub fn load(
        chart: &PlayableChart,
        chart_path: &Path,
        decoded: &crate::skin_loader::DecodedSkin,
        renderer: &mut Renderer,
    ) -> Result<Self> {
        let max_texture = chart
            .bga_assets
            .iter()
            .map(|asset| bga_texture_id(asset.id))
            .chain(decoded.sources.iter().map(|source| source.texture.0))
            .max()
            .unwrap_or(50_000);
        let mut media = Self {
            catalog: HashMap::new(),
            videos: HashMap::new(),
            skin_videos: Vec::new(),
            document: decoded.document.clone(),
            active_bga: std::array::from_fn(|_| None),
            output_texture_base: max_texture.checked_add(16).context("texture ID overflow")?,
            stagefile_size: None,
            backbmp: false,
        };
        let folder = chart_path.parent().context("chart has no parent folder")?.to_string_lossy();
        for (id, relative) in [
            (bmz_render::plan::SELECT_STAGE_TEXTURE, &chart.metadata.stage_file),
            (bmz_render::plan::PLAY_BACKBMP_TEXTURE, &chart.metadata.backbmp_file),
            (bmz_render::plan::SELECT_BANNER_TEXTURE, &chart.metadata.banner_file),
        ] {
            if let Some(path) = crate::chart_asset::resolve_chart_asset_path(&folder, relative)
                && let Ok(image) = bmz_render::assets::load_static_rgba_image(&path)
            {
                renderer.upsert_image_asset(id, &image)?;
                if id == bmz_render::plan::SELECT_STAGE_TEXTURE {
                    media.stagefile_size = Some(bmz_render::skin::SkinImageSize {
                        width: image.width as f32,
                        height: image.height as f32,
                    });
                }
                if id == bmz_render::plan::PLAY_BACKBMP_TEXTURE {
                    media.backbmp = true;
                }
            }
        }
        for asset in &chart.bga_assets {
            match asset.kind {
                BgaAssetKind::Video => {
                    media.videos.insert(asset.id, asset.path.clone());
                    media.catalog.insert(asset.id, display_video_bga_frame(asset.id, 1, 1));
                }
                _ => match bmz_render::assets::load_static_rgba_image(&asset.path) {
                    Ok(image) => {
                        renderer.upsert_image_asset(TextureId(bga_texture_id(asset.id)), &image)?;
                        media.catalog.insert(
                            asset.id,
                            display_bga_frame(asset.id, image.width, image.height),
                        );
                    }
                    Err(error) => {
                        tracing::warn!(path = %asset.path.display(), %error, "BGA image unavailable")
                    }
                },
            }
        }
        for source in decoded.sources.iter().filter(|s| s.is_video) {
            let (active, ops) =
                crate::app::offline_skin_video_gating(&decoded.document, &source.source_id);
            if !active {
                continue;
            }
            media.skin_videos.push(SkinVideo {
                texture: TextureId(source.texture.0),
                path: source.path.clone(),
                period: bmz_video::video_duration_us(&source.path)?,
                active,
                ops,
                origin: None,
                cycle: 0,
                decoder: None,
            });
        }
        Ok(media)
    }

    pub fn update(
        &mut self,
        chart: &PlayableChart,
        snapshot: &mut RenderSnapshot,
        renderer: &mut Renderer,
        chart_us: i64,
        scene_us: i64,
    ) -> Result<()> {
        snapshot.stagefile_background = self.stagefile_size.is_some();
        snapshot.stagefile_image_size = self.stagefile_size;
        snapshot.backbmp_background = self.backbmp;
        let state = crate::app::offline_skin_video_state(snapshot, &self.document);
        let enabled = self.document.enabled_options();
        for source in &mut self.skin_videos {
            if !source.active
                || (!source.ops.is_empty()
                    && !source
                        .ops
                        .iter()
                        .any(|ops| bmz_render::skin::test_skin_ops(ops, &enabled, &state)))
            {
                source.decoder = None;
                source.origin = None;
                continue;
            }
            let origin = *source.origin.get_or_insert(scene_us);
            let elapsed = scene_us - origin;
            let cycle = elapsed / source.period;
            if source.decoder.is_none() || source.cycle != cycle {
                source.decoder = Some(VideoBgaDecoder::open(&source.path)?);
                source.cycle = cycle;
            }
            if let Some(frame) = source
                .decoder
                .as_mut()
                .expect("opened")
                .frame_at_blocking(elapsed % source.period)?
            {
                renderer.upsert_rgba_texture_ref(
                    source.texture,
                    frame.width,
                    frame.height,
                    &frame.rgba,
                )?;
            }
        }
        for (channel, (kind, selected)) in [
            (BgaEventKind::Base, &mut snapshot.bga_base),
            (BgaEventKind::Layer, &mut snapshot.bga_layer),
            (BgaEventKind::Layer2, &mut snapshot.bga_layer2),
            (BgaEventKind::Poor, &mut snapshot.bga_poor),
        ]
        .into_iter()
        .enumerate()
        {
            let Some(display) = selected.as_mut() else {
                self.active_bga[channel] = None;
                continue;
            };
            let Some((&id, path)) =
                self.videos.iter().find(|(id, _)| bga_texture_id(**id) == display.texture_id)
            else {
                self.active_bga[channel] = None;
                continue;
            };
            let event_time = if kind == BgaEventKind::Poor {
                snapshot
                    .recent_judgements
                    .iter()
                    .rev()
                    .find(|j| {
                        matches!(
                            j.judge,
                            bmz_core::judge::Judge::Bad | bmz_core::judge::Judge::Poor
                        )
                    })
                    .map(|j| j.time.0)
            } else {
                chart
                    .bga_events
                    .iter()
                    .rev()
                    .find(|e| e.kind == kind && e.asset == Some(id) && e.time.0 <= chart_us)
                    .map(|e| e.time.0)
            };
            let Some(start) = event_time else {
                continue;
            };
            if !self.active_bga[channel]
                .as_ref()
                .is_some_and(|(old_id, old_start, _)| *old_id == id && *old_start == start)
            {
                self.active_bga[channel] = Some((id, start, VideoBgaDecoder::open(path)?));
            }
            let decoder = &mut self.active_bga[channel].as_mut().expect("opened").2;
            display.texture_id = self.output_texture_base + channel as u32;
            if let Some(frame) = decoder.frame_at_blocking(chart_us - start)? {
                renderer.upsert_rgba_texture_ref(
                    TextureId(display.texture_id),
                    frame.width,
                    frame.height,
                    &frame.rgba,
                )?;
                display.width = frame.width as f32;
                display.height = frame.height as f32;
            }
        }
        Ok(())
    }
}
