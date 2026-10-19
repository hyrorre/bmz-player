use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result, bail};
use flate2::read::ZlibDecoder;
use image::ImageReader;

pub mod animated;
pub mod sprite_sheet;

pub use animated::{AnimatedImage, AnimatedImageLimits, DecodedImage, LimitPolicy, may_animate};

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
    load_static_rgba_image_with_color_key(path, None)
}

/// LR2 color key applies to images without their own alpha channel.
pub fn load_static_rgba_image_with_color_key(
    path: &Path,
    color_key: Option<[u8; 3]>,
) -> Result<RgbaImageAsset> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "bmp" | "jpg" | "jpeg" | "gif" | "tga" | "webp" => {
            load_image_rgba_with_color_key(path, color_key)
        }
        "cim" => load_cim_rgba(path),
        _ => bail!("unsupported image format: {}", path.display()),
    }
}

/// Skin source image. Animated GIF / WebP / APNG become a beatoraja sprite sheet that the
/// skin animates with `divx` / `divy` / `cycle`; other files load as still images.
pub fn load_skin_source_rgba_image_with_color_key(
    path: &Path,
    color_key: Option<[u8; 3]>,
) -> Result<RgbaImageAsset> {
    if !may_animate(path) {
        return load_static_rgba_image_with_color_key(path, color_key);
    }
    match animated::decode_image_frames(path, color_key, &AnimatedImageLimits::SKIN)? {
        DecodedImage::Static(image) => Ok(image),
        DecodedImage::Animated(animation) => {
            let (sheet, (columns, rows)) = sprite_sheet::build_sprite_sheet(
                &animation,
                sprite_sheet::SPRITE_SHEET_MAX_TEXTURE_DIMENSION,
            )
            .with_context(|| format!("animated skin image: {}", path.display()))?;
            tracing::info!(
                path = %path.display(),
                frames = animation.frames().len(),
                columns,
                rows,
                width = sheet.width,
                height = sheet.height,
                "loaded animated skin image as a sprite sheet"
            );
            Ok(sheet)
        }
    }
}

/// `#STAGEFILE` / `#BANNER` / `#BACKBMP` image with its animation, if any.
pub fn load_chart_image(path: &Path, limits: &AnimatedImageLimits) -> Result<DecodedImage> {
    if may_animate(path) {
        animated::decode_image_frames(path, None, limits)
    } else {
        load_static_rgba_image(path).map(DecodedImage::Static)
    }
}

pub fn load_chart_bga_image(path: &Path) -> Result<RgbaImageAsset> {
    load_static_rgba_image(path).map(pad_small_bga_like_beatoraja)
}

/// BGA image with its animation, if any. Small images are padded per frame like
/// [`load_chart_bga_image`].
pub fn load_chart_bga_frames(path: &Path, limits: &AnimatedImageLimits) -> Result<DecodedImage> {
    Ok(match load_chart_image(path, limits)? {
        DecodedImage::Static(image) => DecodedImage::Static(pad_small_bga_like_beatoraja(image)),
        DecodedImage::Animated(animation) => DecodedImage::Animated(std::sync::Arc::new(
            std::sync::Arc::unwrap_or_clone(animation).map_frames(pad_small_bga_like_beatoraja),
        )),
    })
}

fn load_image_rgba(path: &Path) -> Result<RgbaImageAsset> {
    load_image_rgba_with_color_key(path, None)
}

fn load_image_rgba_with_color_key(
    path: &Path,
    color_key: Option<[u8; 3]>,
) -> Result<RgbaImageAsset> {
    if !path.is_file() {
        let bytes = bmz_skin_assets::read(path)?;
        // TGA has no reliable magic. Keep the extension hint, as ImageReader::open does.
        let reader =
            ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::from_path(path)?);
        return decode_image_rgba(reader, path, color_key);
    }
    let reader = ImageReader::open(path)
        .with_context(|| format!("failed to open image: {}", path.display()))?;
    decode_image_rgba(reader, path, color_key)
}

fn read_image_bytes(path: &Path) -> Result<Vec<u8>> {
    if path.is_file() {
        std::fs::read(path).with_context(|| format!("failed to read image: {}", path.display()))
    } else {
        bmz_skin_assets::read(path)
    }
}

fn decode_image_rgba(
    reader: ImageReader<impl std::io::BufRead + std::io::Seek>,
    path: &Path,
    color_key: Option<[u8; 3]>,
) -> Result<RgbaImageAsset> {
    let reader = reader
        .with_guessed_format()
        .with_context(|| format!("failed to guess image format: {}", path.display()))?;
    let image =
        reader.decode().with_context(|| format!("failed to decode image: {}", path.display()))?;
    let has_alpha = image.color().has_alpha();
    let mut rgba = image.to_rgba8();
    if !has_alpha && let Some(color_key) = color_key {
        apply_lr2_color_key(&mut rgba, color_key);
    }
    let width = rgba.width();
    let height = rgba.height();
    let asset = RgbaImageAsset { width, height, pixels: rgba.into_raw() };
    asset.validate()?;
    Ok(asset)
}

fn apply_lr2_color_key(rgba: &mut image::RgbaImage, color_key: [u8; 3]) {
    for pixel in rgba.pixels_mut() {
        if pixel.0[..3] == color_key {
            pixel.0[3] = 0;
        }
    }
}

/// libGDX `PixmapIO.writeCIM` 形式を RGBA8 へ展開する。
///
/// CIM は zlib stream 内に big-endian の width / height / Gdx2DPixmap format と
/// Pixmap の生 pixel buffer を順に格納する。beatoraja は画像キャッシュだけでなく
/// スキン配布物の source としてもこの形式を読み込む。
fn load_cim_rgba(path: &Path) -> Result<RgbaImageAsset> {
    let file: Box<dyn Read> = if path.is_file() {
        Box::new(
            File::open(path)
                .with_context(|| format!("failed to open CIM image: {}", path.display()))?,
        )
    } else {
        Box::new(Cursor::new(bmz_skin_assets::read(path)?))
    };
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
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        3 => source
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        4 => source,
        5 => source
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|pixel| {
                let value = u16::from_le_bytes([pixel[0], pixel[1]]);
                let r = (((value >> 11) & 0x1f) * 255 / 31) as u8;
                let g = (((value >> 5) & 0x3f) * 255 / 63) as u8;
                let b = ((value & 0x1f) * 255 / 31) as u8;
                [r, g, b, 255]
            })
            .collect(),
        6 => source
            .as_chunks::<2>()
            .0
            .iter()
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
    fn lr2_color_key_masks_rgb_but_preserves_existing_alpha() {
        for extension in ["bmp", "png"] {
            let (_temp, path) = temp_image_path(extension);
            image::RgbImage::from_raw(2, 1, vec![0, 255, 0, 12, 34, 56])
                .unwrap()
                .save(&path)
                .unwrap();
            let asset = load_static_rgba_image_with_color_key(&path, Some([0, 255, 0])).unwrap();
            assert_eq!(asset.pixels, [0, 255, 0, 0, 12, 34, 56, 255]);
            let plain = load_static_rgba_image(&path).unwrap();
            assert_eq!(plain.pixels[3], 255);
        }
        let (_temp, path) = temp_image_path("png");
        image::RgbaImage::from_raw(1, 1, vec![0, 255, 0, 200]).unwrap().save(&path).unwrap();
        let asset = load_static_rgba_image_with_color_key(&path, Some([0, 255, 0])).unwrap();
        assert_eq!(asset.pixels, [0, 255, 0, 200]);
    }

    #[test]
    fn load_static_rgba_image_reads_24_bit_bmp_pixels() {
        let (_temp, path) = temp_image_path("bmp");
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
    }

    #[test]
    fn load_static_rgba_image_decodes_8bpp_indexed_bmp() {
        // 旧デコーダは bits_per_pixel=8 を弾いていたため、image crate 移行後は
        // パレット付き 8bpp BMP がデコードできることを担保する。
        let (_temp, path) = temp_image_path("bmp");
        let buffer = image::GrayImage::from_raw(2, 2, vec![10, 20, 30, 40]).unwrap();
        buffer.save_with_format(&path, image::ImageFormat::Bmp).unwrap();

        let asset = load_static_rgba_image(&path).expect("8bpp BMP must decode via image crate");

        assert_eq!(asset.width, 2);
        assert_eq!(asset.height, 2);
    }

    #[test]
    fn load_static_rgba_image_decodes_beatoraja_image_extensions() {
        for (extension, format) in
            [("gif", image::ImageFormat::Gif), ("tga", image::ImageFormat::Tga)]
        {
            let (_temp, path) = temp_image_path(extension);
            let buffer = image::RgbaImage::from_raw(1, 1, vec![12, 34, 56, 255]).unwrap();
            buffer.save_with_format(&path, format).unwrap();

            let asset = load_static_rgba_image(&path)
                .unwrap_or_else(|error| panic!("failed to decode {extension}: {error}"));

            assert_eq!(asset.width, 1);
            assert_eq!(asset.height, 1);
            assert_eq!(asset.pixels, vec![12, 34, 56, 255]);
        }
    }

    #[test]
    fn load_static_rgba_image_decodes_libgdx_cim_rgba8888() {
        let (_temp, path) = temp_image_path("cim");
        write_test_cim(&path, 2, 1, 4, &[12, 34, 56, 78, 90, 123, 200, 255]);

        let asset = load_static_rgba_image(&path).expect("libGDX CIM must decode");

        assert_eq!(asset.width, 2);
        assert_eq!(asset.height, 1);
        assert_eq!(asset.pixels, vec![12, 34, 56, 78, 90, 123, 200, 255]);
    }

    #[test]
    fn load_static_rgba_image_rejects_truncated_cim_pixels() {
        let (_temp, path) = temp_image_path("cim");
        write_test_cim(&path, 2, 1, 4, &[12, 34, 56, 78]);

        let error = load_static_rgba_image(&path).unwrap_err();

        assert!(error.to_string().contains("truncated CIM pixel data"));
    }

    #[test]
    fn load_static_rgba_image_rejects_unsupported_extension() {
        let (_temp, path) = temp_image_path("xyz");
        std::fs::write(&path, b"not an image").unwrap();
        let err = load_static_rgba_image(&path).unwrap_err();
        assert!(err.to_string().contains("unsupported image format"));
    }

    #[test]
    fn image_asset_validation_rejects_wrong_lengths() {
        let asset = RgbaImageAsset { width: 1, height: 1, pixels: vec![255] };

        assert!(asset.validate().is_err());
    }

    #[test]
    fn load_chart_bga_image_pads_small_images_like_beatoraja() {
        let (_temp, path) = temp_image_path("png");
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
        let pixel_range = |row: usize, column: usize| {
            let start = (row * 256 + column) * 4;
            start..start + 4
        };
        let left_black = pixel_range(0, 126);
        assert_eq!(&asset.pixels[left_black], &[0, 0, 0, 0]);
        let first_source_pixel = pixel_range(0, 127);
        assert_eq!(&asset.pixels[first_source_pixel], &[255, 0, 0, 255]);
        let second_row_pixel = pixel_range(1, 128);
        assert_eq!(&asset.pixels[second_row_pixel], &[255, 255, 255, 255]);
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

        for file_name in [
            "small.png",
            "still.gif",
            "tga_only.tga",
            "animated.gif",
            "webp_still.webp",
            "webp_anim.webp",
            "apng_anim.png",
        ] {
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

    #[test]
    fn load_static_rgba_image_decodes_webp_and_first_animation_frames() {
        let root = repo_root().join("data/songs/bga-compat");

        let still = load_static_rgba_image(&root.join("webp_still.webp")).unwrap();
        assert_eq!((still.width, still.height), (2, 2));
        assert_eq!(&still.pixels[..4], &[12, 34, 56, 255]);
        for (file_name, first) in [
            ("webp_anim.webp", [255, 0, 0, 255]),
            ("apng_anim.png", [255, 0, 0, 255]),
            ("apng_single.png", [255, 255, 255, 255]),
        ] {
            let asset = load_static_rgba_image(&root.join(file_name)).unwrap();
            assert_eq!(&asset.pixels[..4], &first, "{file_name}");
        }
    }

    #[test]
    fn load_chart_bga_frames_pads_every_animated_frame() {
        let root = repo_root().join("data/songs/bga-compat");

        let decoded =
            load_chart_bga_frames(&root.join("animated.gif"), &AnimatedImageLimits::CHART).unwrap();

        let animation = decoded.animation().expect("animated.gif has two frames");
        assert_eq!((animation.width(), animation.height()), (256, 256));
        assert_eq!(animation.frames().len(), 2);
        assert_eq!(animation.plays(), None);
        // GIF delays are 0 ms and 100 ms; 0 ms is raised to 10 ms.
        assert_eq!(animation.first_cycle_us(), 110_000);
        let first_source_pixel = (127usize * 4)..(128usize * 4);
        assert_eq!(&animation.frame(0).pixels[first_source_pixel.clone()], &[255, 0, 0, 255]);
        assert_eq!(&animation.frame(1).pixels[first_source_pixel], &[0, 0, 255, 255]);
        assert_eq!(&animation.frame(1).pixels[..4], &[0, 0, 0, 0]);

        let still =
            load_chart_bga_frames(&root.join("small.png"), &AnimatedImageLimits::CHART).unwrap();
        assert!(still.animation().is_none());
        assert_eq!((still.first_frame().width, still.first_frame().height), (256, 256));
    }

    #[test]
    fn load_skin_source_builds_beatoraja_sprite_sheets_for_animations() {
        let root = repo_root().join("data/songs/bga-compat");

        // Two 2x2 frames: 1x2 and 2x1 tie, so beatoraja keeps one column.
        let gif =
            load_skin_source_rgba_image_with_color_key(&root.join("animated.gif"), None).unwrap();
        assert_eq!((gif.width, gif.height), (2, 4));
        assert_eq!(&gif.pixels[..4], &[255, 0, 0, 255]);
        assert_eq!(&gif.pixels[2 * 2 * 4..2 * 2 * 4 + 4], &[0, 0, 255, 255]);

        for file_name in ["webp_anim.webp", "apng_anim.png"] {
            let sheet =
                load_skin_source_rgba_image_with_color_key(&root.join(file_name), None).unwrap();
            assert_eq!((sheet.width, sheet.height), (2, 6), "{file_name}");
        }
        for file_name in ["webp_still.webp", "apng_single.png", "small.png"] {
            let still =
                load_skin_source_rgba_image_with_color_key(&root.join(file_name), None).unwrap();
            let expected = load_static_rgba_image(&root.join(file_name)).unwrap();
            assert_eq!(still, expected, "{file_name}");
        }
    }

    /// The name only carried a timestamp, so equal stamps (coarse clocks, or another test
    /// process) shared a file. The TempDir owns the image and removes it on drop.
    fn temp_image_path(extension: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::Builder::new().prefix("bmz-render-test-").tempdir().unwrap();
        let path = temp.path().join(format!("image.{extension}"));
        (temp, path)
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
