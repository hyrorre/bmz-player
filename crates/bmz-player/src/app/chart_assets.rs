use super::*;

pub(super) fn chart_asset_folder(chart: &PlayableChart) -> Option<PathBuf> {
    chart
        .sounds
        .iter()
        .find_map(|asset| asset.path.parent())
        .or_else(|| chart.bga_assets.iter().find_map(|asset| asset.path.parent()))
        .map(Path::to_path_buf)
}

/// Uploads the still image (an animation's first frame) and returns its size and path.
pub(super) fn load_chart_meta_texture(
    renderer: &mut Renderer,
    texture_id: TextureId,
    folder_path: &str,
    relative: &str,
) -> Option<(SkinImageSize, PathBuf)> {
    let path = crate::chart_asset::resolve_chart_asset_path(folder_path, relative)?;
    match load_static_rgba_image(&path) {
        Ok(image) => {
            if let Err(error) = renderer.upsert_image_asset(texture_id, &image) {
                tracing::warn!(path = %path.display(), %error, "failed to upload chart meta image");
                None
            } else {
                let size = SkinImageSize { width: image.width as f32, height: image.height as f32 };
                Some((size, path))
            }
        }
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "skipping chart meta image");
            None
        }
    }
}

pub(super) fn load_chart_bga_textures(
    renderer: &mut Renderer,
    chart: &PlayableChart,
    animations: &mut crate::animated_bga::BgaAnimationCatalog,
) -> BgaFrameCatalog {
    use bmz_chart::model::BgaAssetKind;

    let total_start = Instant::now();
    let mut considered_assets = 0u32;
    let mut static_assets = 0u32;
    let mut skipped_non_static = 0u32;
    let mut loaded_assets = 0u32;
    let mut failed_assets = 0u32;
    let mut total_file_bytes = 0u64;
    let mut loaded_file_bytes = 0u64;
    let mut rgba_bytes = 0u64;
    let mut decode_us = 0u128;
    let mut upload_us = 0u128;
    let mut frames = BgaFrameCatalog::new();
    let mut animation_budget = crate::animated_bga::AnimatedBgaBudget::default();
    for asset in &chart.bga_assets {
        considered_assets += 1;
        let path = &asset.path;
        let file_bytes = bmz_song_resource::file_len(path).unwrap_or(0);
        total_file_bytes = total_file_bytes.saturating_add(file_bytes);
        if asset.kind != BgaAssetKind::Static {
            skipped_non_static += 1;
            tracing::debug!(
                asset_id = asset.id.0,
                path = %path.display(),
                "skipping non-static BGA asset (will be decoded at play time)"
            );
            continue;
        }
        static_assets += 1;

        let decode_start = Instant::now();
        match animation_budget.load(path) {
            Ok(decoded) => {
                let image_decode_us = decode_start.elapsed().as_micros();
                decode_us += image_decode_us;
                let texture_id = TextureId(bga_texture_id(asset.id));
                let image = decoded.first_frame();
                let frame = display_bga_frame(asset.id, image.width, image.height);
                let animation = decoded.animation();
                let image_rgba_bytes =
                    animation.map_or(image.pixels.len() as u64, |animation| animation.rgba_bytes());
                let upload_start = Instant::now();
                if let Err(error) = renderer.upsert_image_asset(texture_id, image) {
                    let image_upload_us = upload_start.elapsed().as_micros();
                    upload_us += image_upload_us;
                    failed_assets += 1;
                    tracing::warn!(
                        asset_id = asset.id.0,
                        texture_id = texture_id.0,
                        file_bytes,
                        rgba_bytes = image_rgba_bytes,
                        decode_us = image_decode_us,
                        upload_us = image_upload_us,
                        path = %path.display(),
                        %error,
                        "failed to upload BGA image"
                    );
                } else {
                    let image_upload_us = upload_start.elapsed().as_micros();
                    upload_us += image_upload_us;
                    loaded_assets += 1;
                    loaded_file_bytes = loaded_file_bytes.saturating_add(file_bytes);
                    rgba_bytes = rgba_bytes.saturating_add(image_rgba_bytes);
                    tracing::info!(
                        asset_id = asset.id.0,
                        texture_id = texture_id.0,
                        width = image.width,
                        height = image.height,
                        file_bytes,
                        rgba_bytes = image_rgba_bytes,
                        decode_us = image_decode_us,
                        upload_us = image_upload_us,
                        path = %path.display(),
                        "loaded BGA image"
                    );
                    frames.insert(asset.id, frame);
                    if let Some(animation) = animation {
                        animations.insert(asset.id, animation.clone());
                    }
                }
            }
            Err(error) => {
                let image_decode_us = decode_start.elapsed().as_micros();
                decode_us += image_decode_us;
                failed_assets += 1;
                tracing::warn!(
                    asset_id = asset.id.0,
                    file_bytes,
                    decode_us = image_decode_us,
                    path = %path.display(),
                    %error,
                    "skipping unreadable BGA image"
                );
            }
        }
    }
    tracing::info!(
        chart_bga_assets = considered_assets,
        static_assets,
        skipped_non_static,
        loaded_assets,
        failed_assets,
        total_file_bytes,
        loaded_file_bytes,
        rgba_bytes,
        decode_us,
        upload_us,
        total_us = total_start.elapsed().as_micros(),
        "chart BGA image load timing"
    );
    frames
}
