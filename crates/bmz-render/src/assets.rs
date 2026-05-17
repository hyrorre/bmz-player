use std::fs::File;
use std::io::{BufReader, Read};
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

pub fn load_static_rgba_image(path: &Path) -> Result<RgbaImageAsset> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => load_png_rgba(path),
        "bmp" => load_bmp_rgba(path),
        "jpg" | "jpeg" => load_jpeg_rgba(path),
        _ => bail!("unsupported image format: {}", path.display()),
    }
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

pub fn load_bmp_rgba(path: &Path) -> Result<RgbaImageAsset> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("failed to open bmp: {}", path.display()))?
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read bmp: {}", path.display()))?;
    decode_bmp_rgba(&bytes).with_context(|| format!("failed to decode bmp: {}", path.display()))
}

pub fn load_jpeg_rgba(path: &Path) -> Result<RgbaImageAsset> {
    let file =
        File::open(path).with_context(|| format!("failed to open jpeg: {}", path.display()))?;
    let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(file));
    let bytes =
        decoder.decode().with_context(|| format!("failed to decode jpeg: {}", path.display()))?;
    let info = decoder.info().context("jpeg decoder did not return image info")?;
    let pixels = normalize_jpeg_frame(&bytes, info.pixel_format)?;
    let asset = RgbaImageAsset { width: info.width as u32, height: info.height as u32, pixels };
    asset.validate()?;
    Ok(asset)
}

fn normalize_jpeg_frame(bytes: &[u8], pixel_format: jpeg_decoder::PixelFormat) -> Result<Vec<u8>> {
    match pixel_format {
        jpeg_decoder::PixelFormat::L8 => {
            let mut out = Vec::with_capacity(bytes.len() * 4);
            for &value in bytes {
                out.extend_from_slice(&[value, value, value, 255]);
            }
            Ok(out)
        }
        jpeg_decoder::PixelFormat::RGB24 => {
            let mut out = Vec::with_capacity(bytes.len() / 3 * 4);
            for rgb in bytes.chunks_exact(3) {
                out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            Ok(out)
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            let mut out = Vec::with_capacity(bytes.len());
            for cmyk in bytes.chunks_exact(4) {
                let c = cmyk[0] as u16;
                let m = cmyk[1] as u16;
                let y = cmyk[2] as u16;
                let k = cmyk[3] as u16;
                out.extend_from_slice(&[
                    (255 - (c * (255 - k) / 255 + k)) as u8,
                    (255 - (m * (255 - k) / 255 + k)) as u8,
                    (255 - (y * (255 - k) / 255 + k)) as u8,
                    255,
                ]);
            }
            Ok(out)
        }
        _ => bail!("unsupported jpeg pixel format: {pixel_format:?}"),
    }
}

fn decode_bmp_rgba(bytes: &[u8]) -> Result<RgbaImageAsset> {
    if bytes.len() < 54 || &bytes[0..2] != b"BM" {
        bail!("invalid bmp header");
    }

    let pixel_offset = read_u32(bytes, 10)? as usize;
    let dib_size = read_u32(bytes, 14)? as usize;
    if dib_size < 40 || bytes.len() < 14 + dib_size {
        bail!("unsupported bmp dib header");
    }

    let width = read_i32(bytes, 18)?;
    let height = read_i32(bytes, 22)?;
    let planes = read_u16(bytes, 26)?;
    let bits_per_pixel = read_u16(bytes, 28)?;
    let compression = read_u32(bytes, 30)?;
    if planes != 1 {
        bail!("invalid bmp plane count: {planes}");
    }
    if compression != 0 {
        bail!("compressed bmp textures are not supported");
    }
    if width <= 0 || height == 0 {
        bail!("invalid bmp dimensions: {width}x{height}");
    }
    if !matches!(bits_per_pixel, 24 | 32) {
        bail!("only 24-bit and 32-bit bmp textures are supported");
    }

    let width = width as usize;
    let abs_height = height.unsigned_abs() as usize;
    let channels = bits_per_pixel as usize / 8;
    let row_stride = (width * channels).div_ceil(4) * 4;
    let required = pixel_offset
        .checked_add(row_stride.checked_mul(abs_height).context("bmp dimensions overflow")?)
        .context("bmp dimensions overflow")?;
    if bytes.len() < required {
        bail!("bmp pixel data is truncated");
    }

    let mut pixels = vec![0; width * abs_height * 4];
    for row in 0..abs_height {
        let source_row = if height > 0 { abs_height - 1 - row } else { row };
        let source_offset = pixel_offset + source_row * row_stride;
        for col in 0..width {
            let source = source_offset + col * channels;
            let dest = (row * width + col) * 4;
            pixels[dest] = bytes[source + 2];
            pixels[dest + 1] = bytes[source + 1];
            pixels[dest + 2] = bytes[source];
            pixels[dest + 3] = if channels == 4 { bytes[source + 3] } else { 255 };
        }
    }

    let asset = RgbaImageAsset { width: width as u32, height: abs_height as u32, pixels };
    asset.validate()?;
    Ok(asset)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = bytes.get(offset..offset + 2).context("unexpected end of bmp header")?;
    Ok(u16::from_le_bytes(value.try_into().unwrap()))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = bytes.get(offset..offset + 4).context("unexpected end of bmp header")?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32> {
    let value = bytes.get(offset..offset + 4).context("unexpected end of bmp header")?;
    Ok(i32::from_le_bytes(value.try_into().unwrap()))
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
    fn load_static_rgba_image_reads_24_bit_bmp_pixels() {
        let path = temp_image_path("bmp");
        write_test_bmp_24(&path, 2, 2, &[[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]]);

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
    fn decode_bmp_rgba_reads_top_down_32_bit_pixels() {
        let bytes = test_bmp_32_bytes(1, -2, &[[10, 20, 30, 40], [50, 60, 70, 80]]);

        let asset = decode_bmp_rgba(&bytes).unwrap();

        assert_eq!(asset.width, 1);
        assert_eq!(asset.height, 2);
        assert_eq!(asset.pixels, vec![10, 20, 30, 40, 50, 60, 70, 80]);
    }

    #[test]
    fn normalize_jpeg_frame_expands_rgb_to_rgba() {
        let rgba =
            normalize_jpeg_frame(&[10, 20, 30, 40, 50, 60], jpeg_decoder::PixelFormat::RGB24)
                .unwrap();

        assert_eq!(rgba, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn normalize_jpeg_frame_expands_grayscale_to_rgba() {
        let rgba = normalize_jpeg_frame(&[10, 20], jpeg_decoder::PixelFormat::L8).unwrap();

        assert_eq!(rgba, vec![10, 10, 10, 255, 20, 20, 20, 255]);
    }

    #[test]
    fn image_asset_validation_rejects_wrong_lengths() {
        let asset = RgbaImageAsset { width: 1, height: 1, pixels: vec![255] };

        assert!(asset.validate().is_err());
    }

    fn temp_png_path() -> std::path::PathBuf {
        temp_image_path("png")
    }

    fn temp_image_path(extension: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("bmz-render-test-{stamp}.{extension}"))
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

    fn write_test_bmp_24(path: &Path, width: i32, height: i32, pixels: &[[u8; 3]]) {
        std::fs::write(path, test_bmp_24_bytes(width, height, pixels)).unwrap();
    }

    fn test_bmp_24_bytes(width: i32, height: i32, pixels: &[[u8; 3]]) -> Vec<u8> {
        let width_usize = width.unsigned_abs() as usize;
        let height_usize = height.unsigned_abs() as usize;
        assert_eq!(pixels.len(), width_usize * height_usize);
        let row_stride = (width_usize * 3).div_ceil(4) * 4;
        let mut bytes = bmp_header(width, height, 24, row_stride * height_usize);
        for file_row in 0..height_usize {
            let source_row = if height > 0 { height_usize - 1 - file_row } else { file_row };
            for col in 0..width_usize {
                let [r, g, b] = pixels[source_row * width_usize + col];
                bytes.extend_from_slice(&[b, g, r]);
            }
            while (bytes.len() - 54) % row_stride != 0 {
                bytes.push(0);
            }
        }
        bytes
    }

    fn test_bmp_32_bytes(width: i32, height: i32, pixels: &[[u8; 4]]) -> Vec<u8> {
        let width_usize = width.unsigned_abs() as usize;
        let height_usize = height.unsigned_abs() as usize;
        assert_eq!(pixels.len(), width_usize * height_usize);
        let row_stride = width_usize * 4;
        let mut bytes = bmp_header(width, height, 32, row_stride * height_usize);
        for file_row in 0..height_usize {
            let source_row = if height > 0 { height_usize - 1 - file_row } else { file_row };
            for col in 0..width_usize {
                let [r, g, b, a] = pixels[source_row * width_usize + col];
                bytes.extend_from_slice(&[b, g, r, a]);
            }
        }
        bytes
    }

    fn bmp_header(width: i32, height: i32, bits_per_pixel: u16, pixel_bytes: usize) -> Vec<u8> {
        let file_size = 54 + pixel_bytes;
        let mut bytes = Vec::with_capacity(file_size);
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&(file_size as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&bits_per_pixel.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        bytes
    }

    use std::io::BufWriter;
}
