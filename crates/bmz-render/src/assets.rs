use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use flate2::read::ZlibDecoder;
use image::ImageReader;

const CIM_HEADER_LEN: usize = 12;
const MAX_CIM_RGBA_BYTES: u64 = 256 * 1024 * 1024;

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
        "cim" => load_cim_rgba(path),
        _ => bail!("unsupported image format: {}", path.display()),
    }
}

pub fn load_chart_bga_image(path: &Path) -> Result<RgbaImageAsset> {
    load_static_rgba_image(path).map(pad_small_bga_like_beatoraja)
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

/// libGDX `PixmapIO.writeCIM` 形式を RGBA8 へ展開する。
///
/// CIM は zlib stream 内に big-endian の width / height / Gdx2DPixmap format と
/// Pixmap の生 pixel buffer を順に格納する。beatoraja は画像キャッシュだけでなく
/// スキン配布物の source としてもこの形式を読み込む。
fn load_cim_rgba(path: &Path) -> Result<RgbaImageAsset> {
    let file = File::open(path)
        .with_context(|| format!("failed to open CIM image: {}", path.display()))?;
    let mut decoder = ZlibDecoder::new(file);
    let mut header = [0_u8; CIM_HEADER_LEN];
    decoder
        .read_exact(&mut header)
        .with_context(|| format!("failed to read CIM header: {}", path.display()))?;

    let width = i32::from_be_bytes(header[0..4].try_into().unwrap());
    let height = i32::from_be_bytes(header[4..8].try_into().unwrap());
    let format = i32::from_be_bytes(header[8..12].try_into().unwrap());
    if width <= 0 || height <= 0 {
        bail!("invalid CIM dimensions {width}x{height}: {}", path.display());
    }

    let width = width as u32;
    let height = height as u32;
    let pixel_count =
        u64::from(width).checked_mul(u64::from(height)).context("CIM pixel count overflow")?;
    let rgba_len = pixel_count.checked_mul(4).context("CIM RGBA length overflow")?;
    if rgba_len > MAX_CIM_RGBA_BYTES {
        bail!("CIM image is too large after decode ({rgba_len} bytes): {}", path.display());
    }

    let bytes_per_pixel = match format {
        1 => 1_u64, // GDX2D_FORMAT_ALPHA
        2 => 2,     // GDX2D_FORMAT_LUMINANCE_ALPHA
        3 => 3,     // GDX2D_FORMAT_RGB888
        4 => 4,     // GDX2D_FORMAT_RGBA8888
        5 | 6 => 2, // GDX2D_FORMAT_RGB565 / RGBA4444
        _ => bail!("unsupported CIM pixel format {format}: {}", path.display()),
    };
    let source_len = pixel_count
        .checked_mul(bytes_per_pixel)
        .and_then(|len| usize::try_from(len).ok())
        .context("CIM source length overflow")?;
    let mut source = vec![0_u8; source_len];
    decoder
        .read_exact(&mut source)
        .with_context(|| format!("truncated CIM pixel data: {}", path.display()))?;
    let mut trailing = [0_u8; 1];
    if decoder
        .read(&mut trailing)
        .with_context(|| format!("failed to finish CIM decode: {}", path.display()))?
        != 0
    {
        bail!("unexpected trailing CIM pixel data: {}", path.display());
    }

    let pixels = match format {
        1 => source.into_iter().flat_map(|a| [255, 255, 255, a]).collect(),
        2 => source
            .chunks_exact(2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        3 => source.chunks_exact(3).flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255]).collect(),
        4 => source,
        5 => source
            .chunks_exact(2)
            .flat_map(|pixel| {
                let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                let r = (((value >> 11) & 0x1f) * 255 / 31) as u8;
                let g = (((value >> 5) & 0x3f) * 255 / 63) as u8;
                let b = ((value & 0x1f) * 255 / 31) as u8;
                [r, g, b, 255]
            })
            .collect(),
        6 => source
            .chunks_exact(2)
            .flat_map(|pixel| {
                let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                let r = (((value >> 12) & 0x0f) * 17) as u8;
                let g = (((value >> 8) & 0x0f) * 17) as u8;
                let b = (((value >> 4) & 0x0f) * 17) as u8;
                let a = ((value & 0x0f) * 17) as u8;
                [r, g, b, a]
            })
            .collect(),
        _ => unreachable!(),
    };
    let asset = RgbaImageAsset { width, height, pixels };
    asset.validate()?;
    Ok(asset)
}

fn pad_small_bga_like_beatoraja(asset: RgbaImageAsset) -> RgbaImageAsset {
    let source_width = asset.width as usize;
    let source_height = asset.height as usize;
    if source_width.max(source_height) > 256 {
        return asset;
    }

    let mut pixels = vec![0; 256 * 256 * 4];
    let offset_x = (256usize.saturating_sub(source_width)) / 2;
    let row_bytes = source_width * 4;
    for row in 0..source_height {
        let src_start = row * row_bytes;
        let dst_start = (row * 256 + offset_x) * 4;
        pixels[dst_start..dst_start + row_bytes]
            .copy_from_slice(&asset.pixels[src_start..src_start + row_bytes]);
    }
    RgbaImageAsset { width: 256, height: 256, pixels }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

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
    fn load_static_rgba_image_decodes_libgdx_cim_rgba8888() {
        let path = temp_image_path("cim");
        write_test_cim(&path, 2, 1, 4, &[12, 34, 56, 78, 90, 123, 200, 255]);

        let asset = load_static_rgba_image(&path).expect("libGDX CIM must decode");

        assert_eq!(asset.width, 2);
        assert_eq!(asset.height, 1);
        assert_eq!(asset.pixels, vec![12, 34, 56, 78, 90, 123, 200, 255]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_static_rgba_image_rejects_truncated_cim_pixels() {
        let path = temp_image_path("cim");
        write_test_cim(&path, 2, 1, 4, &[12, 34, 56, 78]);

        let error = load_static_rgba_image(&path).unwrap_err();

        assert!(error.to_string().contains("truncated CIM pixel data"));
        std::fs::remove_file(path).unwrap();
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

    #[test]
    fn load_chart_bga_image_pads_small_images_like_beatoraja() {
        let path = temp_image_path("png");
        let image = image::RgbaImage::from_raw(
            2,
            2,
            vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255],
        )
        .unwrap();
        image.save_with_format(&path, image::ImageFormat::Png).unwrap();

        let asset = load_chart_bga_image(&path).unwrap();

        assert_eq!(asset.width, 256);
        assert_eq!(asset.height, 256);
        let left_black = ((0 * 256 + 126) * 4)..((0 * 256 + 127) * 4);
        assert_eq!(&asset.pixels[left_black], &[0, 0, 0, 0]);
        let first_source_pixel = ((0 * 256 + 127) * 4)..((0 * 256 + 128) * 4);
        assert_eq!(&asset.pixels[first_source_pixel], &[255, 0, 0, 255]);
        let second_row_pixel = ((1 * 256 + 128) * 4)..((1 * 256 + 129) * 4);
        assert_eq!(&asset.pixels[second_row_pixel], &[255, 255, 255, 255]);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_chart_bga_image_keeps_large_images_unpadded() {
        let asset = pad_small_bga_like_beatoraja(RgbaImageAsset {
            width: 257,
            height: 1,
            pixels: vec![255; 257 * 4],
        });

        assert_eq!(asset.width, 257);
        assert_eq!(asset.height, 1);
    }

    #[test]
    fn load_chart_bga_image_decodes_data_song_fixture_images() {
        let root = repo_root().join("data/songs/bga-compat");

        for file_name in ["small.png", "still.gif", "tga_only.tga", "animated.gif"] {
            let asset = load_chart_bga_image(&root.join(file_name))
                .unwrap_or_else(|error| panic!("failed to decode {file_name}: {error}"));

            assert_eq!(asset.width, 256, "{file_name}");
            assert_eq!(asset.height, 256, "{file_name}");
        }
    }

    #[test]
    fn load_chart_bga_image_uses_first_frame_for_animated_gif_like_beatoraja() {
        let root = repo_root().join("data/songs/bga-compat");

        let asset = load_chart_bga_image(&root.join("animated.gif")).unwrap();

        let first_source_pixel = ((127usize) * 4)..((128usize) * 4);
        assert_eq!(&asset.pixels[first_source_pixel], &[255, 0, 0, 255]);
    }

    fn temp_image_path(extension: &str) -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("bmz-render-test-{stamp}.{extension}"))
    }

    fn write_test_cim(path: &Path, width: i32, height: i32, format: i32, pixels: &[u8]) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&width.to_be_bytes()).unwrap();
        encoder.write_all(&height.to_be_bytes()).unwrap();
        encoder.write_all(&format.to_be_bytes()).unwrap();
        encoder.write_all(pixels).unwrap();
        std::fs::write(path, encoder.finish().unwrap()).unwrap();
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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
