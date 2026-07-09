use std::path::Path;

use ffmpeg_next::format::Sample as FfmpegSample;
use ffmpeg_next::{codec, format, frame, media};

use crate::loader::{SampleLoadError, SampleLoader};
use crate::sample::DecodedSample;

/// ffmpeg-next を使って wav/ogg/flac/mp3 等の音声ファイルをデコードするローダー。
/// `bmz-video` の動画デコードと同じく ffmpeg-next を利用する。
///
/// 出力はインターリーブ済み f32。サンプルレート変換はミキサー側で行うため、
/// ここではフォーマット変換（整数 PCM / planar → f32 packed）のみ行う。
#[derive(Debug, Default)]
pub struct FfmpegSampleLoader;

impl SampleLoader for FfmpegSampleLoader {
    fn load(&mut self, path: &Path) -> Result<DecodedSample, SampleLoadError> {
        if let Err(message) = bmz_ffmpeg::ensure_init() {
            return Err(decode_error(path, message));
        }
        decode_audio(path).map_err(|message| decode_error(path, message))
    }

    fn duration_ms_hint(&mut self, path: &Path) -> Option<i64> {
        bmz_ffmpeg::ensure_init().ok()?;
        probe_audio_duration_ms(path).ok()
    }
}

fn probe_audio_duration_ms(path: &Path) -> Result<i64, String> {
    let ictx = format::input(path).map_err(|e| format!("failed to open input: {e}"))?;
    let stream = ictx
        .streams()
        .best(media::Type::Audio)
        .ok_or_else(|| "no audio stream found".to_string())?;
    let duration = stream.duration();
    let time_base = stream.time_base();
    if duration <= 0 || time_base.denominator() <= 0 {
        return Err("audio stream has no duration".to_string());
    }
    let duration_ms = (duration as i128)
        .saturating_mul(time_base.numerator() as i128)
        .saturating_mul(1_000)
        .saturating_div(time_base.denominator() as i128);
    (duration_ms > 0)
        .then_some(duration_ms.min(i64::MAX as i128) as i64)
        .ok_or_else(|| "audio stream has no duration".to_string())
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

    let planes = audio.planes();
    if planes == 0 {
        return Ok(());
    }

    frames.reserve(samples * channels);

    if format.is_planar() {
        // planar: チャンネルごとに別プレーン。
        // frame::Audio::data() はスライス長を linesize[index] から取るが、ffmpeg は
        // 音声では linesize[0] しか設定せず linesize[1..] は 0 のままになる。
        // そのため data(1) 以降は空スライス扱いになり、右チャンネルが無音化する
        // （= キー音が左耳からしか鳴らない）。長さを samples で決める plane::<T>()
        // アクセサ経由で各プレーンを読み出す。
        let mut channel_samples: Vec<Vec<f32>> = Vec::with_capacity(channels);
        for channel in 0..channels {
            let plane = channel.min(planes - 1);
            channel_samples.push(read_planar_plane(audio, format, plane)?);
        }
        for sample_index in 0..samples {
            for channel in &channel_samples {
                frames.push(channel.get(sample_index).copied().unwrap_or(0.0));
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

/// planar 音声の 1 プレーン（= 1 チャンネル）を f32 サンプル列として読み出す。
///
/// `plane::<T>()` はスライス長を `linesize` ではなく `samples()` から決めるため、
/// ffmpeg が linesize[1..] を 0 のままにしていても正しい長さで読み出せる。
fn read_planar_plane(
    audio: &frame::Audio,
    format: FfmpegSample,
    plane: usize,
) -> Result<Vec<f32>, String> {
    let values = match format {
        FfmpegSample::U8(_) => {
            audio.plane::<u8>(plane).iter().map(|&v| (v as f32 - 128.0) / 128.0).collect()
        }
        FfmpegSample::I16(_) => {
            audio.plane::<i16>(plane).iter().map(|&v| v as f32 / 32_768.0).collect()
        }
        FfmpegSample::I32(_) => {
            audio.plane::<i32>(plane).iter().map(|&v| v as f32 / 2_147_483_648.0).collect()
        }
        FfmpegSample::F32(_) => audio.plane::<f32>(plane).to_vec(),
        FfmpegSample::F64(_) => audio.plane::<f64>(plane).iter().map(|&v| v as f32).collect(),
        FfmpegSample::I64(_) => {
            return Err("planar 64-bit integer audio is not supported".to_string());
        }
        FfmpegSample::None => return Err("decoder produced no sample format".to_string()),
    };
    Ok(values)
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
    fn append_frame_samples_planar_keeps_both_channels() {
        use ffmpeg_next::ChannelLayout;
        use ffmpeg_next::format::sample::Type;

        // planar ステレオフレームを組み立てる。ffmpeg は linesize[1] を 0 のままにするため、
        // 旧実装では右チャンネル（plane 1）が空スライス扱いになり無音化していた。
        let mut audio =
            frame::Audio::new(FfmpegSample::F32(Type::Planar), 3, ChannelLayout::STEREO);
        audio.set_rate(48_000);
        audio.plane_mut::<f32>(0).copy_from_slice(&[0.1, 0.2, 0.3]);
        audio.plane_mut::<f32>(1).copy_from_slice(&[-0.1, -0.2, -0.3]);

        let mut frames = Vec::new();
        append_frame_samples(&audio, 2, &mut frames).unwrap();

        // L/R が交互に並んだインターリーブになっていること（右が 0.0 に潰れていない）。
        assert_eq!(frames, vec![0.1, -0.1, 0.2, -0.2, 0.3, -0.3]);
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

    #[test]
    fn ffmpeg_loader_reports_stream_duration_without_decoding_samples() {
        let path = write_pcm16_wav(&vec![0; 44_100]);
        let mut loader = FfmpegSampleLoader;

        let duration_ms = loader.duration_ms_hint(&path).unwrap();

        assert!((990..=1_010).contains(&duration_ms), "duration_ms={duration_ms}");
        std::fs::remove_file(path).unwrap();
    }
}
