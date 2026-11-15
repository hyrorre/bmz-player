use super::*;
use std::io::Read;

const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RGBA_BYTES: u64 = 16 * 1024 * 1024;

/// Bounded, first-frame chart image decode for songlist rows. Called by workers
/// after chart/archive path resolution, never by a render-item evaluator.
pub fn load_chart_thumbnail(path: &Path) -> Result<RgbaImageAsset> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    decode_chart_thumbnail(&bytes)
}

/// Same bounded first-frame decoder for an archive entry read into memory.
pub fn decode_chart_thumbnail(bytes: &[u8]) -> Result<RgbaImageAsset> {
    anyhow::ensure!(bytes.len() as u64 <= MAX_FILE_BYTES, "banner file exceeds 16 MiB");
    let header = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let format = header.format().context("unknown banner image format")?;
    let (width, height) = header.into_dimensions()?;
    anyhow::ensure!(
        width > 0 && height > 0 && width <= 8192 && height <= 8192,
        "invalid banner dimensions"
    );
    anyhow::ensure!(
        u64::from(width) * u64::from(height) * 4 <= MAX_RGBA_BYTES,
        "decoded banner exceeds 16 MiB"
    );
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_RGBA_BYTES);
    reader.limits(limits);
    let pixels = reader.decode()?.into_rgba8();
    let scale = (1024.0 / f64::from(width)).min(256.0 / f64::from(height)).min(1.0);
    let size = (
        (f64::from(width) * scale).floor().max(1.0) as u32,
        (f64::from(height) * scale).floor().max(1.0) as u32,
    );
    let pixels = if size == (width, height) {
        pixels
    } else {
        image::imageops::resize(&pixels, size.0, size.1, image::imageops::FilterType::Triangle)
    };
    Ok(RgbaImageAsset { width: size.0, height: size.1, pixels: pixels.into_raw() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
        let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 128]));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(pixels).write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn banner_thumbnail_preserves_alpha_aspect_and_never_upscales() {
        for (width, height, expected) in
            [(200, 50, (200, 50)), (2048, 512, (1024, 256)), (100, 1000, (25, 256))]
        {
            let thumbnail =
                decode_chart_thumbnail(&encoded(width, height, image::ImageFormat::Png)).unwrap();
            assert_eq!((thumbnail.width, thumbnail.height), expected);
            assert_eq!(&thumbnail.pixels[..4], &[10, 20, 30, 128]);
            assert!(thumbnail.pixels.len() <= 1024 * 256 * 4);
        }
    }

    #[test]
    fn banner_thumbnail_uses_first_animated_frame_and_standard_static_formats() {
        for format in [image::ImageFormat::Png, image::ImageFormat::Bmp, image::ImageFormat::WebP] {
            let thumbnail = decode_chart_thumbnail(&encoded(4, 2, format)).unwrap();
            assert_eq!((thumbnail.width, thumbnail.height), (4, 2));
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            for red in [10, 200] {
                encoder
                    .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                        2,
                        1,
                        image::Rgba([red, 20, 30, 255]),
                    )))
                    .unwrap();
            }
        }
        let thumbnail = decode_chart_thumbnail(&bytes).unwrap();
        assert_eq!(&thumbnail.pixels[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn banner_thumbnail_rejects_encoded_and_decoded_limits_and_corrupt_inputs() {
        assert!(decode_chart_thumbnail(&vec![0; MAX_FILE_BYTES as usize + 1]).is_err());
        assert!(decode_chart_thumbnail(b"not an image").is_err());
        let large = encoded(4096, 1025, image::ImageFormat::Png);
        let error = decode_chart_thumbnail(&large).unwrap_err();
        assert!(error.to_string().contains("decoded banner exceeds"));
    }
}
