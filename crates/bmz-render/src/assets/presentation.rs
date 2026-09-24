//! Bounded, CPU-only decoding for chart loading/ready images.
use super::RgbaImageAsset;
use anyhow::{Context, Result, ensure};
use image::{AnimationDecoder, ImageDecoder};
use std::{
    io::Cursor,
    path::Path,
    time::{Duration, Instant},
};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RGBA_BYTES: usize = 256 * 1024 * 1024;
const MAX_FRAMES: usize = 4096;
const MAX_DECODE_TIME: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct PresentationImage {
    pub frames: Vec<RgbaImageAsset>,
    frame_ends_us: Vec<u64>,
    /// Total plays. None means infinite; absent GIF loop extension means one play.
    plays: Option<u32>,
}

impl PresentationImage {
    pub fn first_cycle_us(&self) -> u64 {
        *self.frame_ends_us.last().unwrap_or(&0)
    }

    pub fn frame_index(&self, elapsed_us: u64) -> usize {
        let duration = self.first_cycle_us();
        if duration == 0 {
            return 0;
        }
        if self.plays.is_some_and(|plays| elapsed_us / duration >= u64::from(plays)) {
            return self.frames.len() - 1;
        }
        self.frame_ends_us.partition_point(|end| *end <= elapsed_us % duration)
    }
}

pub fn load_presentation_image(path: &Path) -> Result<PresentationImage> {
    ensure!(
        std::fs::metadata(path)?.len() <= MAX_FILE_BYTES,
        "presentation image file exceeds 64 MiB"
    );
    // Bound the actual read too, including a file growing between metadata and read.
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_FILE_BYTES, "presentation image file exceeds 64 MiB");
    decode_presentation_image(&bytes)
        .with_context(|| format!("presentation image: {}", path.display()))
}

fn decode_presentation_image(bytes: &[u8]) -> Result<PresentationImage> {
    let started = Instant::now();
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(MAX_RGBA_BYTES as u64);
    if image::guess_format(bytes)? != image::ImageFormat::Gif {
        let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        reader.limits(limits);
        let rgba = reader.decode()?.into_rgba8();
        return Ok(PresentationImage {
            frames: vec![RgbaImageAsset {
                width: rgba.width(),
                height: rgba.height(),
                pixels: rgba.into_raw(),
            }],
            frame_ends_us: vec![0],
            plays: Some(1),
        });
    }
    // image's loop_count maps the missing extension (Finite(0)) to infinite.
    // Read the GIF container metadata directly to preserve once/finite/infinite.
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut metadata = options.read_info(Cursor::new(bytes))?;
    let mut count = 0;
    while metadata.next_frame_info()?.is_some() {
        count += 1;
        ensure!(
            count <= MAX_FRAMES && started.elapsed() <= MAX_DECODE_TIME,
            "presentation GIF decode limit exceeded"
        );
    }
    let plays = match metadata.repeat() {
        gif::Repeat::Infinite => None,
        gif::Repeat::Finite(repeats) => Some(u32::from(repeats) + 1),
    };
    let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(limits)?;
    let mut frames = Vec::new();
    let mut frame_ends_us = Vec::new();
    let mut total_bytes = 0usize;
    let mut duration = 0u64;
    for frame in decoder.into_frames() {
        ensure!(started.elapsed() <= MAX_DECODE_TIME, "presentation GIF decode timed out");
        let frame = frame?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        // GIF centiseconds: keep positive delays, normalize zero to 10 ms.
        let delay = (u64::from(numerator) * 1000 / u64::from(denominator).max(1)).max(10_000);
        duration = duration.checked_add(delay).context("GIF duration overflow")?;
        let rgba = frame.into_buffer();
        total_bytes = total_bytes.checked_add(rgba.len()).context("GIF size overflow")?;
        ensure!(
            total_bytes <= MAX_RGBA_BYTES && frames.len() < MAX_FRAMES,
            "presentation GIF exceeds decoded image budget"
        );
        frame_ends_us.push(duration);
        frames.push(RgbaImageAsset {
            width: rgba.width(),
            height: rgba.height(),
            pixels: rgba.into_raw(),
        });
    }
    ensure!(!frames.is_empty(), "empty presentation GIF");
    Ok(PresentationImage { frames, frame_ends_us, plays })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(repeat: Option<image::codecs::gif::Repeat>) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
            if let Some(repeat) = repeat {
                encoder.set_repeat(repeat).unwrap();
            }
            for (color, delay) in [([255, 0, 0, 255], 20), ([0, 0, 255, 255], 70)] {
                encoder
                    .encode_frame(image::Frame::from_parts(
                        image::RgbaImage::from_pixel(2, 2, image::Rgba(color)),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(delay, 1),
                    ))
                    .unwrap();
            }
        }
        bytes
    }

    #[test]
    fn gif_without_loop_extension_plays_once_including_last_delay() {
        let image = decode_presentation_image(&gif(None)).unwrap();
        assert_eq!(image.first_cycle_us(), 90_000);
        assert_eq!(image.frame_index(19_999), 0);
        assert_eq!(image.frame_index(20_000), 1);
        assert_eq!(image.frame_index(89_999), 1);
        assert_eq!(image.frame_index(90_000), 1);
        assert_eq!(image.frame_index(u64::MAX), 1);
    }

    #[test]
    fn finite_and_infinite_loops_do_not_change_first_cycle_gate() {
        let finite =
            decode_presentation_image(&gif(Some(image::codecs::gif::Repeat::Finite(1)))).unwrap();
        assert_eq!(finite.frame_index(90_000), 0);
        assert_eq!(finite.frame_index(180_000), 1);
        let infinite =
            decode_presentation_image(&gif(Some(image::codecs::gif::Repeat::Infinite))).unwrap();
        assert_eq!(infinite.first_cycle_us(), 90_000);
        assert_eq!(infinite.frame_index(180_000), 0);
    }

    #[test]
    fn corrupt_gif_is_rejected() {
        assert!(decode_presentation_image(b"GIF89a\x01\x00").is_err());
    }

    #[test]
    fn single_frame_gif_waits_its_delay_and_zero_delay_is_bounded() {
        for (delay, expected) in [(0, 10_000), (7, 70_000)] {
            let mut bytes = Vec::new();
            {
                let mut encoder = gif::Encoder::new(&mut bytes, 1, 1, &[255, 0, 0]).unwrap();
                encoder
                    .write_frame(&gif::Frame {
                        width: 1,
                        height: 1,
                        delay,
                        buffer: std::borrow::Cow::Borrowed(&[0]),
                        ..Default::default()
                    })
                    .unwrap();
            }
            let image = decode_presentation_image(&bytes).unwrap();
            assert_eq!(image.first_cycle_us(), expected);
            assert_eq!(image.frame_index(u64::MAX), 0);
        }
    }

    #[test]
    fn partial_frames_respect_transparency_and_disposal() {
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
                        buffer: std::borrow::Cow::Owned(pixels),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        let image = decode_presentation_image(&bytes).unwrap();
        assert_eq!(image.frames[0].pixels, [255, 0, 0, 255, 255, 0, 0, 255]);
        assert_eq!(image.frames[1].pixels, [255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(image.frames[2].pixels, [0, 0, 255, 255, 255, 0, 0, 255]);
        assert_eq!(image.frames[3].pixels, [255, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn oversized_gif_canvas_is_rejected() {
        let mut bytes = gif(None);
        bytes[6..8].copy_from_slice(&8193_u16.to_le_bytes());
        assert!(decode_presentation_image(&bytes).is_err());
    }
}
