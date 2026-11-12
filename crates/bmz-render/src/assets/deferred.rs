//! Lossless CPU storage for large, infrequently drawn skin animation frames.
use super::RgbaImageAsset;
use anyhow::{Result, ensure};
use std::io::{Read, Write};

#[derive(Debug)]
pub struct DeferredImage {
    pub width: u32,
    pub height: u32,
    bytes: Vec<u8>,
}

impl DeferredImage {
    pub fn encode(image: &RgbaImageAsset) -> Result<Self> {
        image.validate()?;
        ensure!(image.pixels.len() <= 16 * 1024 * 1024, "deferred skin frame exceeds 16 MiB");
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&image.pixels)?;
        Ok(Self { width: image.width, height: image.height, bytes: encoder.finish()? })
    }

    pub fn stored_bytes(&self) -> usize {
        self.bytes.len()
    }

    pub fn decode(&self) -> Result<RgbaImageAsset> {
        let len = (u64::from(self.width) * u64::from(self.height)).saturating_mul(4);
        ensure!(len <= 16 * 1024 * 1024, "deferred skin frame exceeds 16 MiB");
        let len = len as usize;
        let mut pixels = Vec::with_capacity(len);
        flate2::read::ZlibDecoder::new(self.bytes.as_slice())
            .take(len as u64 + 1)
            .read_to_end(&mut pixels)?;
        ensure!(pixels.len() == len, "invalid deferred skin frame");
        Ok(RgbaImageAsset { width: self.width, height: self.height, pixels })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_transparent_rgb_and_partial_alpha_losslessly() {
        let image =
            RgbaImageAsset { width: 2, height: 1, pixels: vec![3, 7, 11, 0, 13, 17, 19, 127] };
        assert_eq!(DeferredImage::encode(&image).unwrap().decode().unwrap(), image);
    }

    #[test]
    fn rejects_oversized_and_inconsistent_deferred_frames_before_upload() {
        let image = RgbaImageAsset { width: 1, height: 1, pixels: vec![0; 4] };
        let mut encoded = DeferredImage::encode(&image).unwrap();
        encoded.width = u32::MAX;
        encoded.height = u32::MAX;
        assert!(encoded.decode().is_err());
        encoded.width = 2;
        encoded.height = 1;
        assert!(encoded.decode().is_err());
    }
}
