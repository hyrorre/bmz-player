use std::path::Path;

use anyhow::{Context, Result, bail};
use image::ImageReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImageAsset {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImageAsset {
    pub fn validate(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            bail!("image dimensions must be non-zero");
        }
        let expected = self.width as usize * self.height as usize * 4;
        if self.pixels.len() != expected {
            bail!(
                "rgba image length mismatch: expected {expected} bytes, got {}",
                self.pixels.len()
            );
        }
        Ok(())
    }
}

pub fn load_png_rgba(path: &Path) -> Result<RgbaImageAsset> {
    load_image_rgba(path)
}

pub fn load_static_rgba_image(path: &Path) -> Result<RgbaImageAsset> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "bmp" | "jpg" | "jpeg" | "gif" | "tga" => load_image_rgba(path),
        _ => bail!("unsupported image format: {}", path.display()),
    }
}

fn load_image_rgba(path: &Path) -> Result<RgbaImageAsset> {
    let reader = ImageReader::open(path)
        .with_context(|| format!("failed to open image: {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("failed to guess image format: {}", path.display()))?;
    let image =
        reader.decode().with_context(|| format!("failed to decode image: {}", path.display()))?;
    let rgba = image.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let asset = RgbaImageAsset { width, height, pixels: rgba.into_raw() };
    asset.validate()?;
    Ok(asset)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn load_static_rgba_image_reads_24_bit_bmp_pixels() {
        let path = temp_image_path("bmp");
        std::fs::write(
            &path,
            test_bmp_24_bytes(2, 2, &[[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]]),
        )
        .unwrap();

        let asset = load_static_rgba_image(&path).unwrap();

        assert_eq!(asset.width, 2);
        assert_eq!(asset.height, 2);
        assert_eq!(
            asset.pixels,
            vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,]
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_static_rgba_image_decodes_8bpp_indexed_bmp() {
        // 旧デコーダは bits_per_pixel=8 を弾いていたため、image crate 移行後は
        // パレット付き 8bpp BMP がデコードできることを担保する。
        let path = temp_image_path("bmp");
        let buffer = image::GrayImage::from_raw(2, 2, vec![10, 20, 30, 40]).unwrap();
        buffer.save_with_format(&path, image::ImageFormat::Bmp).unwrap();

        let asset = load_static_rgba_image(&path).expect("8bpp BMP must decode via image crate");

        assert_eq!(asset.width, 2);
        assert_eq!(asset.height, 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_static_rgba_image_decodes_beatoraja_image_extensions() {
        for (extension, format) in
            [("gif", image::ImageFormat::Gif), ("tga", image::ImageFormat::Tga)]
        {
            let path = temp_image_path(extension);
            let buffer = image::RgbaImage::from_raw(1, 1, vec![12, 34, 56, 255]).unwrap();
            buffer.save_with_format(&path, format).unwrap();

            let asset = load_static_rgba_image(&path)
                .unwrap_or_else(|error| panic!("failed to decode {extension}: {error}"));

            assert_eq!(asset.width, 1);
            assert_eq!(asset.height, 1);
            assert_eq!(asset.pixels, vec![12, 34, 56, 255]);
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn load_static_rgba_image_rejects_unsupported_extension() {
        let path = temp_image_path("xyz");
        std::fs::write(&path, b"not an image").unwrap();
        let err = load_static_rgba_image(&path).unwrap_err();
        assert!(err.to_string().contains("unsupported image format"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn image_asset_validation_rejects_wrong_lengths() {
        let asset = RgbaImageAsset { width: 1, height: 1, pixels: vec![255] };

        assert!(asset.validate().is_err());
    }

    fn temp_image_path(extension: &str) -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("bmz-render-test-{stamp}.{extension}"))
    }

    fn test_bmp_24_bytes(width: i32, height: i32, pixels: &[[u8; 3]]) -> Vec<u8> {
        let width_usize = width.unsigned_abs() as usize;
        let height_usize = height.unsigned_abs() as usize;
        assert_eq!(pixels.len(), width_usize * height_usize);
        let row_stride = (width_usize * 3).div_ceil(4) * 4;
        let mut bytes = bmp_header(width, height, 24, row_stride * height_usize, 0, 0);
        for file_row in 0..height_usize {
            let source_row = if height > 0 { height_usize - 1 - file_row } else { file_row };
            for col in 0..width_usize {
                let [r, g, b] = pixels[source_row * width_usize + col];
                bytes.extend_from_slice(&[b, g, r]);
            }
            while !(bytes.len() - 54).is_multiple_of(row_stride) {
                bytes.push(0);
            }
        }
        bytes
    }

    fn bmp_header(
        width: i32,
        height: i32,
        bits_per_pixel: u16,
        pixel_bytes: usize,
        compression: u32,
        palette_entries: u32,
    ) -> Vec<u8> {
        let pixel_offset = 14u32 + 40 + palette_entries * 4;
        let file_size = pixel_offset as usize + pixel_bytes;
        let mut bytes = Vec::with_capacity(file_size);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&(file_size as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&pixel_offset.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&bits_per_pixel.to_le_bytes());
        bytes.extend_from_slice(&compression.to_le_bytes());
        bytes.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes
    }
}
