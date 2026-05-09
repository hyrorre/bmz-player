use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};

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
    let file =
        File::open(path).with_context(|| format!("failed to open png: {}", path.display()))?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("failed to read png info: {}", path.display()))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .with_context(|| format!("failed to decode png: {}", path.display()))?;
    let bytes = &buffer[..info.buffer_size()];
    let pixels = normalize_png_frame(bytes, info.color_type, info.bit_depth)?;
    let asset = RgbaImageAsset { width: info.width, height: info.height, pixels };
    asset.validate()?;
    Ok(asset)
}

fn normalize_png_frame(
    bytes: &[u8],
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
) -> Result<Vec<u8>> {
    if bit_depth != png::BitDepth::Eight {
        return Err(anyhow!("only 8-bit png textures are supported for now"));
    }

    match color_type {
        png::ColorType::Rgba => Ok(bytes.to_vec()),
        png::ColorType::Rgb => {
            let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
            for rgb in bytes.chunks_exact(3) {
                out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            Ok(out)
        }
        png::ColorType::Grayscale => {
            let mut out = Vec::with_capacity(bytes.len() * 4);
            for &value in bytes {
                out.extend_from_slice(&[value, value, value, 255]);
            }
            Ok(out)
        }
        png::ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(bytes.len() / 2 * 4);
            for ga in bytes.chunks_exact(2) {
                out.extend_from_slice(&[ga[0], ga[0], ga[0], ga[1]]);
            }
            Ok(out)
        }
        png::ColorType::Indexed => {
            Err(anyhow!("indexed png textures are not supported without expansion"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_png_frame_expands_rgb_to_rgba() {
        let rgba = normalize_png_frame(
            &[10, 20, 30, 40, 50, 60],
            png::ColorType::Rgb,
            png::BitDepth::Eight,
        )
        .unwrap();

        assert_eq!(rgba, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn load_png_rgba_reads_rgba_pixels() {
        let path = temp_png_path();
        write_test_png(&path, png::ColorType::Rgba, &[255, 0, 0, 255, 0, 255, 0, 128]);

        let asset = load_png_rgba(&path).unwrap();

        assert_eq!(asset.width, 2);
        assert_eq!(asset.height, 1);
        assert_eq!(asset.pixels, vec![255, 0, 0, 255, 0, 255, 0, 128]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn image_asset_validation_rejects_wrong_lengths() {
        let asset = RgbaImageAsset { width: 1, height: 1, pixels: vec![255] };

        assert!(asset.validate().is_err());
    }

    fn temp_png_path() -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("bmz-render-test-{stamp}.png"))
    }

    fn write_test_png(path: &Path, color_type: png::ColorType, pixels: &[u8]) {
        let file = File::create(path).unwrap();
        let writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, 2, 1);
        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }

    use std::io::BufWriter;
}
