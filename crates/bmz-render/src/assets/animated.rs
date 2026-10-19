//! Animated GIF / WebP / APNG decoding.
//!
//! Skin sources turn the frames into a beatoraja sprite sheet, while chart images keep
//! the frames and their delays so BGA, STAGEFILE, BANNER and BACKBMP can play them.
//! Images with fewer than two frames stay on the still-image path.

use std::fmt;
use std::io::Cursor;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::metadata::LoopCount;
use image::{AnimationDecoder, Delay, Frames, ImageDecoder, ImageFormat, ImageReader};

use super::RgbaImageAsset;

/// Shorter delays (including the common GIF delay 0) are raised to this.
const MIN_FRAME_DELAY_US: u64 = 10_000;
const MIB: u64 = 1024 * 1024;

/// What to do when an animation exceeds [`AnimatedImageLimits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitPolicy {
    /// Warn and use the first frame (the default image for APNG) as a still image.
    FirstFrame,
    /// Fail the decode.
    Error,
}

/// Bounds for decoding every frame. They apply only after a file is known to be animated,
/// so large still images keep loading as before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimatedImageLimits {
    pub max_file_bytes: u64,
    pub max_canvas_dimension: u32,
    pub max_frames: usize,
    pub max_rgba_bytes: u64,
    pub max_decode_time: Duration,
    pub on_limit: LimitPolicy,
}

impl AnimatedImageLimits {
    /// Chart images fall back to their first frame instead of disappearing.
    pub const CHART: Self = Self {
        max_file_bytes: 64 * MIB,
        max_canvas_dimension: 8192,
        max_frames: 4096,
        max_rgba_bytes: 256 * MIB,
        max_decode_time: Duration::from_secs(10),
        on_limit: LimitPolicy::FirstFrame,
    };
    /// Skin sources follow the oversized-texture path and are skipped with a warning.
    pub const SKIN: Self = Self { on_limit: LimitPolicy::Error, ..Self::CHART };

    pub fn with_max_rgba_bytes(self, max_rgba_bytes: u64) -> Self {
        Self { max_rgba_bytes, ..self }
    }
}

/// Canvas-sized RGBA frames with their display end times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimatedImage {
    width: u32,
    height: u32,
    frames: Vec<RgbaImageAsset>,
    /// Cumulative end time of each frame within one cycle, in microseconds.
    frame_ends_us: Vec<u64>,
    /// Total plays including the first. `None` loops forever.
    plays: Option<NonZeroU32>,
}

impl AnimatedImage {
    /// Builds an animation from frames and delays. Delays below 10 ms are raised to 10 ms.
    pub fn new(frames: Vec<(RgbaImageAsset, Duration)>, plays: Option<NonZeroU32>) -> Result<Self> {
        ensure!(frames.len() >= 2, "an animation needs at least two frames");
        let (width, height) = (frames[0].0.width, frames[0].0.height);
        let mut elapsed_us = 0_u64;
        let mut frame_ends_us = Vec::with_capacity(frames.len());
        let mut assets = Vec::with_capacity(frames.len());
        for (frame, delay) in frames {
            frame.validate()?;
            ensure!(
                frame.width == width && frame.height == height,
                "animation frames must share the canvas size {width}x{height}"
            );
            let delay_us = u64::try_from(delay.as_micros()).unwrap_or(u64::MAX);
            elapsed_us = elapsed_us
                .checked_add(delay_us.max(MIN_FRAME_DELAY_US))
                .context("animation duration overflow")?;
            frame_ends_us.push(elapsed_us);
            assets.push(frame);
        }
        Ok(Self { width, height, frames: assets, frame_ends_us, plays })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn frames(&self) -> &[RgbaImageAsset] {
        &self.frames
    }

    pub fn frame(&self, index: usize) -> &RgbaImageAsset {
        &self.frames[index]
    }

    pub fn plays(&self) -> Option<NonZeroU32> {
        self.plays
    }

    /// Length of one cycle, including the last frame's delay.
    pub fn first_cycle_us(&self) -> u64 {
        *self.frame_ends_us.last().expect("animation has frames")
    }

    pub fn rgba_bytes(&self) -> u64 {
        self.frames.iter().map(|frame| frame.pixels.len() as u64).sum()
    }

    /// Frame shown `elapsed_us` after the animation started. After the final play of a
    /// finite animation the last frame is held.
    pub fn frame_index(&self, elapsed_us: u64) -> usize {
        let cycle_us = self.first_cycle_us();
        if self.plays.is_some_and(|plays| elapsed_us / cycle_us >= u64::from(plays.get())) {
            return self.frames.len() - 1;
        }
        self.frame_ends_us.partition_point(|end| *end <= elapsed_us % cycle_us)
    }

    /// Applies the same size-deterministic transform (such as BGA padding) to every frame.
    pub(crate) fn map_frames(self, mut map: impl FnMut(RgbaImageAsset) -> RgbaImageAsset) -> Self {
        let frames: Vec<_> = self.frames.into_iter().map(&mut map).collect();
        Self { width: frames[0].width, height: frames[0].height, frames, ..self }
    }
}

/// A decoded image file: a still image, or an animation with at least two frames.
#[derive(Debug, Clone)]
pub enum DecodedImage {
    Static(RgbaImageAsset),
    Animated(Arc<AnimatedImage>),
}

impl DecodedImage {
    pub fn first_frame(&self) -> &RgbaImageAsset {
        match self {
            Self::Static(image) => image,
            Self::Animated(animation) => animation.frame(0),
        }
    }

    pub fn animation(&self) -> Option<&Arc<AnimatedImage>> {
        match self {
            Self::Static(_) => None,
            Self::Animated(animation) => Some(animation),
        }
    }
}

/// Whether the extension may hold an animation this module plays (GIF, WebP, APNG).
pub fn may_animate(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        ["gif", "png", "webp"].iter().any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

/// Decodes every frame of an animated GIF, WebP or APNG. Other formats, single-frame
/// files and APNGs the decoder cannot animate (such as 16-bit ones) return the still image
/// that [`super::load_static_rgba_image_with_color_key`] would.
pub fn decode_image_frames(
    path: &Path,
    color_key: Option<[u8; 3]>,
    limits: &AnimatedImageLimits,
) -> Result<DecodedImage> {
    let bytes = super::read_image_bytes(path)?;
    decode_image_bytes(&bytes, path, color_key, limits)
}

fn decode_image_bytes(
    bytes: &[u8],
    path: &Path,
    color_key: Option<[u8; 3]>,
    limits: &AnimatedImageLimits,
) -> Result<DecodedImage> {
    let started = Instant::now();
    let animation = match image::guess_format(bytes).ok() {
        Some(ImageFormat::Png) => decode_apng(bytes, color_key, limits, started),
        Some(ImageFormat::WebP) => decode_animated_webp(bytes, color_key, limits, started),
        Some(ImageFormat::Gif) => decode_animated_gif(bytes, limits, started),
        _ => Ok(None),
    };
    match animation {
        Ok(Some(animation)) => return Ok(DecodedImage::Animated(Arc::new(animation))),
        Ok(None) => {}
        Err(error) if error.is::<LimitExceeded>() && limits.on_limit == LimitPolicy::Error => {
            return Err(error.context(format!("animated image: {}", path.display())));
        }
        Err(error) => tracing::warn!(
            path = %path.display(),
            error = %format!("{error:#}"),
            "using the first frame of an animated image"
        ),
    }
    decode_still_bytes(bytes, path, color_key).map(DecodedImage::Static)
}

fn decode_still_bytes(
    bytes: &[u8],
    path: &Path,
    color_key: Option<[u8; 3]>,
) -> Result<RgbaImageAsset> {
    let reader = match image::guess_format(bytes) {
        Ok(format) => ImageReader::with_format(Cursor::new(bytes), format),
        Err(_) => ImageReader::with_format(Cursor::new(bytes), ImageFormat::from_path(path)?),
    };
    super::decode_image_rgba(reader, path, color_key)
}

fn decode_apng(
    bytes: &[u8],
    color_key: Option<[u8; 3]>,
    limits: &AnimatedImageLimits,
    started: Instant,
) -> Result<Option<AnimatedImage>> {
    let decoder = PngDecoder::new(Cursor::new(bytes))?;
    if !decoder.is_apng()? {
        return Ok(None);
    }
    let (width, height) = decoder.dimensions();
    check_container(bytes, width, height, limits)?;
    let has_alpha = decoder.color_type().has_alpha();
    let decoder = decoder.apng()?;
    let plays = total_plays(decoder.loop_count());
    let frames = decoder.into_frames();
    collect_frames(frames, (width, height), has_alpha, color_key, plays, limits, started)
}

fn decode_animated_webp(
    bytes: &[u8],
    color_key: Option<[u8; 3]>,
    limits: &AnimatedImageLimits,
    started: Instant,
) -> Result<Option<AnimatedImage>> {
    let decoder = WebPDecoder::new(Cursor::new(bytes))?;
    if !decoder.has_animation() {
        return Ok(None);
    }
    let (width, height) = decoder.dimensions();
    check_container(bytes, width, height, limits)?;
    let has_alpha = decoder.color_type().has_alpha();
    let plays = total_plays(decoder.loop_count());
    let frames = decoder.into_frames();
    collect_frames(frames, (width, height), has_alpha, color_key, plays, limits, started)
}

fn decode_animated_gif(
    bytes: &[u8],
    limits: &AnimatedImageLimits,
    started: Instant,
) -> Result<Option<AnimatedImage>> {
    // image's loop_count maps the missing NETSCAPE extension (Finite(0)) to infinite.
    // Read the container directly to keep once / finite / infinite apart.
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut metadata = options.read_info(Cursor::new(bytes))?;
    let (width, height) = (u32::from(metadata.width()), u32::from(metadata.height()));
    let mut frame_count = 0_usize;
    while metadata.next_frame_info()?.is_some() {
        frame_count += 1;
        check_limit(frame_count <= limits.max_frames, "too many animation frames")?;
        check_limit(started.elapsed() <= limits.max_decode_time, "animation decode timed out")?;
    }
    if frame_count < 2 {
        return Ok(None);
    }
    check_container(bytes, width, height, limits)?;
    let frame_bytes = u64::from(width) * u64::from(height) * 4;
    check_limit(
        frame_bytes.saturating_mul(frame_count as u64) <= limits.max_rgba_bytes,
        "animation exceeds the decoded size budget",
    )?;
    let plays = match metadata.repeat() {
        gif::Repeat::Infinite => None,
        gif::Repeat::Finite(repeats) => NonZeroU32::new(u32::from(repeats) + 1),
    };
    // GIF frames always decode to RGBA, so the LR2 color key never applies (as for stills).
    let frames = GifDecoder::new(Cursor::new(bytes))?.into_frames();
    collect_frames(frames, (width, height), true, None, plays, limits, started)
}

fn total_plays(loop_count: LoopCount) -> Option<NonZeroU32> {
    match loop_count {
        LoopCount::Infinite => None,
        LoopCount::Finite(plays) => Some(plays),
    }
}

fn check_container(
    bytes: &[u8],
    width: u32,
    height: u32,
    limits: &AnimatedImageLimits,
) -> Result<()> {
    check_limit(bytes.len() as u64 <= limits.max_file_bytes, "animated image file is too large")?;
    check_limit(
        width <= limits.max_canvas_dimension && height <= limits.max_canvas_dimension,
        "animation canvas is too large",
    )
}

fn collect_frames(
    frames: Frames<'_>,
    (width, height): (u32, u32),
    has_alpha: bool,
    color_key: Option<[u8; 3]>,
    plays: Option<NonZeroU32>,
    limits: &AnimatedImageLimits,
    started: Instant,
) -> Result<Option<AnimatedImage>> {
    let frame_bytes = u64::from(width) * u64::from(height) * 4;
    let mut collected = Vec::new();
    for frame in frames {
        check_limit(started.elapsed() <= limits.max_decode_time, "animation decode timed out")?;
        check_limit(collected.len() < limits.max_frames, "too many animation frames")?;
        check_limit(
            frame_bytes.saturating_mul(collected.len() as u64 + 1) <= limits.max_rgba_bytes,
            "animation exceeds the decoded size budget",
        )?;
        let frame = frame?;
        ensure!(frame.left() == 0 && frame.top() == 0, "animation frame is not canvas-sized");
        let delay = frame_delay(frame.delay());
        let mut rgba = frame.into_buffer();
        ensure!(
            rgba.width() == width && rgba.height() == height,
            "animation frame is not canvas-sized"
        );
        if !has_alpha && let Some(color_key) = color_key {
            super::apply_lr2_color_key(&mut rgba, color_key);
        }
        collected.push((RgbaImageAsset { width, height, pixels: rgba.into_raw() }, delay));
    }
    if collected.len() < 2 {
        return Ok(None);
    }
    AnimatedImage::new(collected, plays).map(Some)
}

fn frame_delay(delay: Delay) -> Duration {
    let (numerator, denominator) = delay.numer_denom_ms();
    Duration::from_micros(u64::from(numerator) * 1000 / u64::from(denominator.max(1)))
}

#[derive(Debug)]
struct LimitExceeded(&'static str);

impl fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for LimitExceeded {}

fn check_limit(ok: bool, message: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(LimitExceeded(message).into()) }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::path::PathBuf;

    use image::codecs::gif::{GifEncoder, Repeat};

    use super::*;

    fn gif_bytes(repeat: Option<Repeat>, delays_ms: &[u32]) -> Vec<u8> {
        let colors = [[255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]];
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            if let Some(repeat) = repeat {
                encoder.set_repeat(repeat).unwrap();
            }
            for (index, delay) in delays_ms.iter().enumerate() {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(2, 2, image::Rgba(colors[index % 3])),
                        0,
                        0,
                        Delay::from_numer_denom_ms(*delay, 1),
                    ))
                    .unwrap();
            }
        }
        bytes
    }

    fn decode(bytes: &[u8], name: &str, limits: &AnimatedImageLimits) -> Result<DecodedImage> {
        decode_image_bytes(bytes, Path::new(name), None, limits)
    }

    fn animated(bytes: &[u8], name: &str) -> Arc<AnimatedImage> {
        match decode(bytes, name, &AnimatedImageLimits::CHART).unwrap() {
            DecodedImage::Animated(animation) => animation,
            DecodedImage::Static(_) => panic!("{name} decoded as a still image"),
        }
    }

    fn fixture(name: &str) -> Vec<u8> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/songs/bga-compat");
        std::fs::read(root.join(name)).unwrap()
    }

    fn assert_near(actual: &[u8], expected: [u8; 4]) {
        assert!(
            actual.iter().zip(expected).all(|(actual, expected)| actual.abs_diff(expected) <= 1),
            "{actual:?} != {expected:?}"
        );
    }

    fn solid(rgba: [u8; 4]) -> RgbaImageAsset {
        RgbaImageAsset { width: 1, height: 1, pixels: rgba.to_vec() }
    }

    #[test]
    fn gif_loop_extension_maps_to_total_plays() {
        let once = animated(&gif_bytes(None, &[20, 70]), "once.gif");
        assert_eq!(once.plays(), NonZeroU32::new(1));
        assert_eq!(once.first_cycle_us(), 90_000);
        let twice = animated(&gif_bytes(Some(Repeat::Finite(1)), &[20, 70]), "twice.gif");
        assert_eq!(twice.plays(), NonZeroU32::new(2));
        let forever = animated(&gif_bytes(Some(Repeat::Infinite), &[20, 70]), "forever.gif");
        assert_eq!(forever.plays(), None);
    }

    #[test]
    fn frame_index_includes_last_delay_and_holds_after_finite_plays() {
        let once = animated(&gif_bytes(None, &[20, 70]), "once.gif");
        assert_eq!(once.frame_index(0), 0);
        assert_eq!(once.frame_index(19_999), 0);
        assert_eq!(once.frame_index(20_000), 1);
        assert_eq!(once.frame_index(89_999), 1);
        assert_eq!(once.frame_index(90_000), 1);
        assert_eq!(once.frame_index(u64::MAX), 1);

        let twice = animated(&gif_bytes(Some(Repeat::Finite(1)), &[20, 70]), "twice.gif");
        assert_eq!(twice.frame_index(90_000), 0);
        assert_eq!(twice.frame_index(110_000), 1);
        assert_eq!(twice.frame_index(180_000), 1);

        let forever = animated(&gif_bytes(Some(Repeat::Infinite), &[20, 70]), "forever.gif");
        assert_eq!(forever.frame_index(180_000), 0);
        assert_eq!(forever.frame_index(u64::MAX - 5), forever.frame_index((u64::MAX - 5) % 90_000));
    }

    #[test]
    fn zero_delays_are_raised_to_ten_milliseconds() {
        let animation = animated(&gif_bytes(None, &[0, 0, 30]), "zero.gif");
        assert_eq!(animation.first_cycle_us(), 50_000);
        assert_eq!(animation.frame_index(9_999), 0);
        assert_eq!(animation.frame_index(10_000), 1);
    }

    #[test]
    fn single_frame_gif_stays_still() {
        let decoded =
            decode(&gif_bytes(None, &[500]), "still.gif", &AnimatedImageLimits::CHART).unwrap();
        let DecodedImage::Static(image) = decoded else { panic!("expected a still image") };
        assert_eq!(image.pixels, [255, 0, 0, 255].repeat(4));
    }

    #[test]
    fn gif_partial_frames_respect_transparency_and_disposal() {
        let mut bytes = Vec::new();
        {
            let mut encoder =
                gif::Encoder::new(&mut bytes, 2, 1, &[0, 0, 0, 255, 0, 0, 0, 0, 255]).unwrap();
            for (left, pixels, dispose) in [
                (0, vec![1, 1], gif::DisposalMethod::Keep),
                (1, vec![2], gif::DisposalMethod::Previous),
                (0, vec![2, 0], gif::DisposalMethod::Background),
                (0, vec![1], gif::DisposalMethod::Keep),
            ] {
                encoder
                    .write_frame(&gif::Frame {
                        width: pixels.len() as u16,
                        height: 1,
                        left,
                        delay: 1,
                        dispose,
                        transparent: Some(0),
                        buffer: Cow::Owned(pixels),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        let animation = animated(&bytes, "partial.gif");
        assert_eq!(animation.frame(0).pixels, [255, 0, 0, 255, 255, 0, 0, 255]);
        assert_eq!(animation.frame(1).pixels, [255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(animation.frame(2).pixels, [0, 0, 255, 255, 255, 0, 0, 255]);
        assert_eq!(animation.frame(3).pixels, [255, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn animated_webp_reads_frames_delays_and_total_plays() {
        let animation = animated(&fixture("webp_anim.webp"), "webp_anim.webp");
        assert_eq!((animation.width(), animation.height()), (2, 2));
        assert_eq!(animation.frames().len(), 3);
        assert_eq!(animation.first_cycle_us(), 600_000);
        assert_eq!(animation.plays(), NonZeroU32::new(2));
        // image-webp alpha-blends opaque frames with a /256 approximation (255 -> 254).
        assert_near(&animation.frame(0).pixels[..4], [255, 0, 0, 255]);
        assert_near(&animation.frame(1).pixels[..4], [0, 255, 0, 255]);
        assert_near(&animation.frame(2).pixels[..4], [0, 0, 255, 255]);
        assert_eq!(animation.frame_index(100_000), 1);
        assert_eq!(animation.frame_index(300_000), 2);
        assert_eq!(animation.frame_index(1_200_000), 2);
    }

    #[test]
    fn apng_reads_frames_and_total_plays() {
        let animation = animated(&fixture("apng_anim.png"), "apng_anim.png");
        assert_eq!(animation.frames().len(), 3);
        assert_eq!(animation.first_cycle_us(), 600_000);
        assert_eq!(animation.plays(), NonZeroU32::new(3));
        assert_eq!(&animation.frame(2).pixels[..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn apng_with_one_animation_frame_keeps_the_default_image() {
        let decoded =
            decode(&fixture("apng_single.png"), "apng_single.png", &AnimatedImageLimits::CHART)
                .unwrap();
        let DecodedImage::Static(image) = decoded else { panic!("expected the default image") };
        assert_eq!(&image.pixels[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn still_webp_decodes_as_a_still_image() {
        let decoded =
            decode(&fixture("webp_still.webp"), "webp_still.webp", &AnimatedImageLimits::CHART)
                .unwrap();
        let DecodedImage::Static(image) = decoded else { panic!("expected a still image") };
        assert_eq!(&image.pixels[..4], &[12, 34, 56, 255]);
    }

    #[test]
    fn limits_fall_back_to_the_first_frame_or_fail_by_policy() {
        let bytes = gif_bytes(Some(Repeat::Infinite), &[20, 70, 20]);
        let chart = AnimatedImageLimits { max_frames: 2, ..AnimatedImageLimits::CHART };
        let DecodedImage::Static(image) = decode(&bytes, "big.gif", &chart).unwrap() else {
            panic!("expected the first frame")
        };
        assert_eq!(&image.pixels[..4], &[255, 0, 0, 255]);

        let skin = AnimatedImageLimits { max_frames: 2, ..AnimatedImageLimits::SKIN };
        assert!(decode(&bytes, "big.gif", &skin).is_err());
        let budget = AnimatedImageLimits::SKIN.with_max_rgba_bytes(2 * 2 * 4 * 2);
        assert!(decode(&bytes, "big.gif", &budget).is_err());
        let canvas = AnimatedImageLimits { max_canvas_dimension: 1, ..AnimatedImageLimits::SKIN };
        assert!(decode(&bytes, "big.gif", &canvas).is_err());
    }

    #[test]
    fn corrupt_gif_is_rejected() {
        assert!(decode(b"GIF89a\x01\x00", "broken.gif", &AnimatedImageLimits::CHART).is_err());
    }

    #[test]
    fn new_rejects_mismatched_frames() {
        let one = Duration::from_millis(10);
        assert!(AnimatedImage::new(vec![(solid([0; 4]), one)], None).is_err());
        let wide = RgbaImageAsset { width: 2, height: 1, pixels: vec![0; 8] };
        assert!(AnimatedImage::new(vec![(solid([0; 4]), one), (wide, one)], None).is_err());
    }

    #[test]
    fn may_animate_matches_animation_capable_extensions() {
        for name in ["a.gif", "a.PNG", "a.webp"] {
            assert!(may_animate(Path::new(name)), "{name}");
        }
        for name in ["a.bmp", "a.jpg", "a.tga", "a.cim", "a"] {
            assert!(!may_animate(Path::new(name)), "{name}");
        }
    }
}
