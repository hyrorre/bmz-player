//! beatoraja `PixmapResourcePool.toSpriteSheetPixmap` compatible sprite sheets.
//!
//! beatoraja packs every frame of an animated skin image into one texture and leaves the
//! animation to the image's `divx` / `divy` / `cycle`. The grid depends on the maximum
//! texture size, so BMZ fixes it to a common GPU limit instead of the adapter's value:
//! skins decode before the GPU exists, and the same skin must always get the same grid.

use anyhow::{Result, ensure};

use super::RgbaImageAsset;
use super::animated::AnimatedImage;

/// Maximum texture size used to choose the grid (GL_MAX_TEXTURE_SIZE on common GPUs).
pub const SPRITE_SHEET_MAX_TEXTURE_DIMENSION: u32 = 16_384;
const MAX_SPRITE_SHEET_RGBA_BYTES: u64 = 256 * 1024 * 1024;

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
    let frame_count = u32::try_from(animation.frames().len())?;
    let max_columns = (max_texture_dimension / frame_width.max(1)).max(1);
    let max_rows = (max_texture_dimension / frame_height.max(1)).max(1);
    let (columns, rows) =
        beatoraja_sprite_sheet_grid(frame_count, frame_width, frame_height, max_columns, max_rows);
    let width = u64::from(frame_width) * u64::from(columns);
    let height = u64::from(frame_height) * u64::from(rows);
    ensure!(
        width <= u64::from(max_texture_dimension) && height <= u64::from(max_texture_dimension),
        "sprite sheet {width}x{height} ({frame_count} frames) exceeds texture size \
         {max_texture_dimension}"
    );
    ensure!(
        width * height * 4 <= MAX_SPRITE_SHEET_RGBA_BYTES,
        "sprite sheet {width}x{height} exceeds the RGBA size budget"
    );
    let (width, height) = (width as u32, height as u32);
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
