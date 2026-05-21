use std::path::Path;
use std::sync::OnceLock;

use ffmpeg_next::format::Sample as FfmpegSample;
use ffmpeg_next::{codec, format, frame, media};

use crate::loader::{SampleLoadError, SampleLoader};
use crate::sample::DecodedSample;

static FFMPEG_INIT: OnceLock<Result<(), String>> = OnceLock::new();

fn ensure_ffmpeg_init() -> Result<(), String> {
    FFMPEG_INIT
        .get_or_init(|| ffmpeg_next::init().map_err(|e| format!("ffmpeg init failed: {e}")))
        .clone()
}

/// ffmpeg-next を使って wav/ogg/flac/mp3 等の音声ファイルをデコードするローダー。
/// `bmz-video` の動画デコードと同じく ffmpeg-next を利用する。
///
/// 出力はインターリーブ済み f32。サンプルレート変換はミキサー側で行うため、
/// ここではフォーマット変換（整数 PCM / planar → f32 packed）のみ行う。
#[derive(Debug, Default)]
pub struct FfmpegSampleLoader;

impl SampleLoader for FfmpegSampleLoader {
    fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
        if let Err(message) = ensure_ffmpeg_init() {
            return Err(decode_error(path, message));
        }
        decode_audio(path).map_err(|message| decode_error(path, message))
    }
}

fn decode_audio(path: &Path) -> Result<DecodedSample, String> {
    let mut ictx = format::input(path).map_err(|e| format!("failed to open input: {e}"))?;

    let stream = ictx
        .streams()
        .best(media::Type::Audio)
        .ok_or_else(|| "no audio stream found".to_string())?;
    let stream_index = stream.index();

    let context = codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| format!("failed to build codec context: {e}"))?;
    let mut decoder =
        context.decoder().audio().map_err(|e| format!("failed to open audio decoder: {e}"))?;

    let mut frames: Vec<f32> = Vec::new();
    let mut out_channels: u16 = 0;
    let mut out_rate: u32 = 0;

    let receive_frames = |decoder: &mut ffmpeg_next::decoder::Audio,
                          out_channels: &mut u16,
                          out_rate: &mut u32,
                          frames: &mut Vec<f32>|
     -> Result<(), String> {
        let mut decoded = frame::Audio::empty();
        while decoder.receive_frame(&mut decoded).is_ok() {
            let channels = decoded.channels().max(1);
            let rate = decoded.rate();
            if rate == 0 {
                return Err("decoded frame reported zero sample rate".to_string());
            }
            if *out_rate == 0 {
                *out_channels = channels;
                *out_rate = rate;
            }
            append_frame_samples(&decoded, *out_channels, frames)?;
        }
        Ok(())
    };

    for (stream, packet) in ictx.packets() {
        if stream.index() != stream_index {
            continue;
        }
        decoder.send_packet(&packet).map_err(|e| format!("send_packet failed: {e}"))?;
        receive_frames(&mut decoder, &mut out_channels, &mut out_rate, &mut frames)?;
    }

    decoder.send_eof().map_err(|e| format!("send_eof failed: {e}"))?;
    receive_frames(&mut decoder, &mut out_channels, &mut out_rate, &mut frames)?;

    if out_rate == 0 {
        return Err("no audio frames were decoded".to_string());
    }

    Ok(DecodedSample { channels: out_channels, sample_rate: out_rate, frames })
}

/// デコード済みオーディオフレームを f32 インターリーブに変換して `frames` に追記する。
fn append_frame_samples(
    audio: &frame::Audio,
    channels: u16,
    frames: &mut Vec<f32>,
) -> Result<(), String> {
    let samples = audio.samples();
    if samples == 0 || channels == 0 {
        return Ok(());
    }
    let channels = channels as usize;
    let format = audio.format();
    let bytes_per = format.bytes();
    if bytes_per == 0 {
        return Err(format!("unsupported sample format: {}", format.name()));
    }

    frames.reserve(samples * channels);

    if format.is_planar() {
        // planar: チャンネルごとに別プレーン。frame.planes() は channels と一致する想定。
        let planes = audio.planes();
        for sample_index in 0..samples {
            for channel in 0..channels {
                let plane = channel.min(planes.saturating_sub(1));
                let buf = audio.data(plane);
                let offset = sample_index * bytes_per;
                frames.push(sample_to_f32(format, buf, offset)?);
            }
        }
    } else {
        // packed: プレーン 0 にインターリーブで格納されている。
        let buf = audio.data(0);
        let total = samples * channels;
        for index in 0..total {
            let offset = index * bytes_per;
            if offset + bytes_per > buf.len() {
                break;
            }
            frames.push(sample_to_f32(format, buf, offset)?);
        }
    }

    Ok(())
}

/// `buf[offset..]` の 1 サンプルを `format` に従って f32 (-1.0..=1.0 目安) に変換する。
/// ffmpeg はネイティブエンディアンでデコードするため `from_ne_bytes` を使う。
fn sample_to_f32(format: FfmpegSample, buf: &[u8], offset: usize) -> Result<f32, String> {
    let need = format.bytes();
    if offset + need > buf.len() {
        return Ok(0.0);
    }
    let value = match format {
        FfmpegSample::U8(_) => (buf[offset] as f32 - 128.0) / 128.0,
        FfmpegSample::I16(_) => {
            i16::from_ne_bytes([buf[offset], buf[offset + 1]]) as f32 / 32_768.0
        }
        FfmpegSample::I32(_) => {
            i32::from_ne_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]])
                as f32
                / 2_147_483_648.0
        }
        FfmpegSample::I64(_) => {
            let bytes: [u8; 8] = buf[offset..offset + 8].try_into().unwrap();
            i64::from_ne_bytes(bytes) as f32 / 9_223_372_036_854_775_808.0
        }
        FfmpegSample::F32(_) => {
            f32::from_ne_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]])
        }
        FfmpegSample::F64(_) => {
            let bytes: [u8; 8] = buf[offset..offset + 8].try_into().unwrap();
            f64::from_ne_bytes(bytes) as f32
        }
        FfmpegSample::None => return Err("decoder produced no sample format".to_string()),
    };
    Ok(value)
}

fn decode_error(path: &Path, message: impl Into<String>) -> SampleLoadError {
    SampleLoadError::Decode { path: path.to_path_buf(), message: message.into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PCM16 mono WAV をメモリ上に組み立てる。
    fn write_pcm16_wav(samples: &[i16]) -> std::path::PathBuf {
        let sample_rate = 44_100_u32;
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&data);

        let path = std::env::temp_dir().join(format!(
            "bmz-ffmpeg-loader-{}-{}.wav",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn ffmpeg_loader_decodes_pcm16_wav() {
        let path = write_pcm16_wav(&[0, 16_384, -16_384, i16::MAX]);
        let mut loader = FfmpegSampleLoader;

        let sample = loader.load(&path).unwrap();

        assert_eq!(sample.channels, 1);
        assert_eq!(sample.sample_rate, 44_100);
        assert_eq!(sample.frames.len(), 4);
        assert!((sample.frames[0]).abs() < 1e-4);
        assert!((sample.frames[1] - 0.5).abs() < 1e-3);
        assert!((sample.frames[2] + 0.5).abs() < 1e-3);

        std::fs::remove_file(path).unwrap();
    }
}
