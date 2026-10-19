//! beatoraja `PixmapResourcePool.toSpriteSheetPixmap` compatible sprite sheets.
//!
//! beatoraja packs every frame of an animated skin image into one texture and leaves the
//! animation to the image's `divx` / `divy` / `cycle`. The grid depends on the maximum
//! texture size (`GL_MAX_TEXTURE_SIZE` in beatoraja). Skins decode their frames before a
//! GPU device may exist, so the sheet is built right before upload, with the
//! `max_texture_dimension_2d` of the device that receives it.

use anyhow::{Result, ensure};

use super::RgbaImageAsset;
use super::animated::AnimatedImage;

/// Only sizes a source before any GPU device exists. The sheet is rebuilt for the device's
/// own limit when it attaches.
pub const PROVISIONAL_SPRITE_SHEET_TEXTURE_DIMENSION: u32 = 16_384;
const MAX_SPRITE_SHEET_RGBA_BYTES: u64 = 256 * 1024 * 1024;

/// Grid and pixel size of a sprite sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteSheetLayout {
    pub columns: u32,
    pub rows: u32,
    pub width: u32,
    pub height: u32,
}

/// beatoraja's grid for `frames` frames of `frame_width` x `frame_height` under a maximum
/// texture size, without checking that the sheet fits.
pub fn sprite_sheet_grid_for_limit(
    frames: u32,
    frame_width: u32,
    frame_height: u32,
    max_texture_dimension: u32,
) -> (u32, u32) {
    let max_columns = (max_texture_dimension / frame_width.max(1)).max(1);
    let max_rows = (max_texture_dimension / frame_height.max(1)).max(1);
    beatoraja_sprite_sheet_grid(frames, frame_width, frame_height, max_columns, max_rows)
}

/// The sheet `animation` gets under `max_texture_dimension`. Fails when no layout fits the
/// texture size or the RGBA size budget; nothing is allocated either way.
pub fn sprite_sheet_layout(
    animation: &AnimatedImage,
    max_texture_dimension: u32,
) -> Result<SpriteSheetLayout> {
    sprite_sheet_layout_for(
        u32::try_from(animation.frames().len())?,
        animation.width(),
        animation.height(),
        max_texture_dimension,
    )
}

/// [`sprite_sheet_layout`] for `frame_count` frames of `frame_width` x `frame_height`.
pub fn sprite_sheet_layout_for(
    frame_count: u32,
    frame_width: u32,
    frame_height: u32,
    max_texture_dimension: u32,
) -> Result<SpriteSheetLayout> {
    let (columns, rows) =
        sprite_sheet_grid_for_limit(frame_count, frame_width, frame_height, max_texture_dimension);
    let width = u64::from(frame_width) * u64::from(columns);
    let height = u64::from(frame_height) * u64::from(rows);
    ensure!(
        width <= u64::from(max_texture_dimension) && height <= u64::from(max_texture_dimension),
        "sprite sheet {width}x{height} ({frame_count} frames) exceeds texture size \
         {max_texture_dimension}"
    );
    ensure!(
        width.saturating_mul(height).saturating_mul(4) <= MAX_SPRITE_SHEET_RGBA_BYTES,
        "sprite sheet {width}x{height} exceeds the RGBA size budget"
    );
    Ok(SpriteSheetLayout { columns, rows, width: width as u32, height: height as u32 })
}

/// Pixel size of the provisional sheet used before a GPU device exists.
pub fn provisional_sprite_sheet_size(animation: &AnimatedImage) -> (u32, u32) {
    let frame_count = u32::try_from(animation.frames().len()).unwrap_or(u32::MAX);
    let (columns, rows) = sprite_sheet_grid_for_limit(
        frame_count,
        animation.width(),
        animation.height(),
        PROVISIONAL_SPRITE_SHEET_TEXTURE_DIMENSION,
    );
    (animation.width().saturating_mul(columns), animation.height().saturating_mul(rows))
}

/// Port of beatoraja `PixmapResourcePool.getSpriteSheetGrid`. Returns `(columns, rows)`.
pub fn beatoraja_sprite_sheet_grid(
    frames: u32,
    frame_width: u32,
    frame_height: u32,
    max_columns: u32,
    max_rows: u32,
) -> (u32, u32) {
    let (frame_width, frame_height) = (i64::from(frame_width), i64::from(frame_height));
    let column_limit = frames.min(max_columns);
    let mut best = None;
    let mut best_score = i64::MAX;
    for columns in 1..=column_limit {
        if !frames.is_multiple_of(columns) {
            continue;
        }
        let rows = frames / columns;
        if rows > max_rows {
            continue;
        }
        let score = (frame_width * i64::from(columns) - frame_height * i64::from(rows)).abs();
        // Strict comparison: the first grid wins ties, as in beatoraja.
        if score < best_score {
            best = Some((columns, rows));
            best_score = score;
        }
    }
    if let Some(grid) = best {
        return grid;
    }

    let mut columns = column_limit;
    let mut rows = frames.div_ceil(columns);
    while rows > max_rows && columns < column_limit {
        columns += 1;
        rows = frames.div_ceil(columns);
    }
    while columns > 1
        && i64::from(columns - 1) * frame_width >= i64::from(rows) * frame_height
        && frames.div_ceil(columns - 1) <= max_rows
    {
        columns -= 1;
        rows = frames.div_ceil(columns);
    }
    (columns, rows)
}

/// Packs the frames row-major from the top-left into one transparent RGBA sheet.
/// Returns the sheet and its `(columns, rows)` grid. Frame delays are discarded.
pub fn build_sprite_sheet(
    animation: &AnimatedImage,
    max_texture_dimension: u32,
) -> Result<(RgbaImageAsset, (u32, u32))> {
    let (frame_width, frame_height) = (animation.width(), animation.height());
    let SpriteSheetLayout { columns, rows, width, height } =
        sprite_sheet_layout(animation, max_texture_dimension)?;
    let mut pixels = vec![0_u8; width as usize * height as usize * 4];
    let sheet_row_bytes = width as usize * 4;
    let frame_row_bytes = frame_width as usize * 4;
    for (index, frame) in animation.frames().iter().enumerate() {
        let index = index as u32;
        let x = ((index % columns) * frame_width) as usize;
        let y = ((index / columns) * frame_height) as usize;
        for (row, source) in frame.pixels.chunks_exact(frame_row_bytes).enumerate() {
            let start = (y + row) * sheet_row_bytes + x * 4;
            pixels[start..start + frame_row_bytes].copy_from_slice(source);
        }
    }
    let sheet = RgbaImageAsset { width, height, pixels };
    sheet.validate()?;
    Ok((sheet, (columns, rows)))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn grid(frames: u32, fw: u32, fh: u32, limit: u32) -> (u32, u32) {
        beatoraja_sprite_sheet_grid(
            frames,
            fw,
            fh,
            (limit / fw.max(1)).max(1),
            (limit / fh.max(1)).max(1),
        )
    }

    #[test]
    fn grid_prefers_the_squarest_exact_layout() {
        assert_eq!(grid(1, 10, 10, 16_384), (1, 1));
        assert_eq!(grid(4, 10, 10, 16_384), (2, 2));
        assert_eq!(grid(6, 100, 50, 16_384), (2, 3));
        assert_eq!(grid(6, 30, 10, 16_384), (1, 6));
        assert_eq!(grid(60, 512, 512, 16_384), (6, 10));
        assert_eq!(grid(60, 512, 512, 4_096), (8, 8));
    }

    #[test]
    fn grid_keeps_the_first_layout_on_ties() {
        // 1x3 and 3x1 both score 20; beatoraja keeps the first.
        assert_eq!(grid(3, 10, 10, 16_384), (1, 3));
    }

    #[test]
    fn grid_falls_back_when_no_exact_layout_fits() {
        // 7 frames: only 1x7 / 7x1 divide, and both exceed a 5-cell side.
        assert_eq!(grid(7, 10, 10, 50), (3, 3));
        assert_eq!(grid(100, 10, 10, 50), (5, 20));
        assert_eq!(grid(3, 100, 10, 50), (1, 3));
    }

    #[test]
    fn layout_uses_the_device_texture_limit() {
        let layout = |limit| sprite_sheet_layout_for(60, 512, 512, limit);
        // 6x10 needs 5120 px, so a 4096 px GPU gets beatoraja's 8x8 fallback instead.
        let small = layout(4096).unwrap();
        assert_eq!((small.columns, small.rows, small.width, small.height), (8, 8, 4096, 4096));
        for limit in [8192, 16_384] {
            let large = layout(limit).unwrap();
            assert_eq!((large.columns, large.rows, large.width, large.height), (6, 10, 3072, 5120));
        }
        // No 512 px layout of 60 frames fits 2048 px.
        assert!(layout(2048).is_err());
    }

    #[test]
    fn layout_keeps_the_rgba_budget() {
        // 6 frames of 4096 x 4096 fit 16384 px as 2x3 but need 384 MiB.
        assert!(sprite_sheet_layout_for(6, 4096, 4096, 16_384).is_err());
        assert!(sprite_sheet_layout_for(4, 4096, 4096, 16_384).is_ok());
    }

    #[test]
    fn sheet_places_frames_row_major_and_leaves_spare_cells_transparent() {
        let frames = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]
            .into_iter()
            .map(|rgba| {
                let asset = RgbaImageAsset { width: 1, height: 1, pixels: rgba.to_vec() };
                (asset, Duration::from_millis(100))
            })
            .collect();
        let animation = AnimatedImage::new(frames, None).unwrap();

        let (sheet, grid) = build_sprite_sheet(&animation, 2).unwrap();

        assert_eq!(grid, (2, 2));
        assert_eq!((sheet.width, sheet.height), (2, 2));
        assert_eq!(
            sheet.pixels,
            [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [0, 0, 0, 0]].concat()
        );
    }

    #[test]
    fn sheet_that_does_not_fit_the_texture_limit_is_rejected() {
        let frames = (0..3)
            .map(|_| {
                let asset = RgbaImageAsset { width: 2, height: 2, pixels: vec![255; 16] };
                (asset, Duration::from_millis(100))
            })
            .collect();
        let animation = AnimatedImage::new(frames, None).unwrap();

        assert!(build_sprite_sheet(&animation, 3).is_err());
    }
}
