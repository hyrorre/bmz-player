use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FrameRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl FrameRate {
    pub fn parse(value: &str) -> Result<Self> {
        let (n, d) = value.split_once('/').unwrap_or((value, "1"));
        let numerator: u32 = n.parse().context("invalid FPS numerator")?;
        let denominator: u32 = d.parse().context("invalid FPS denominator")?;
        ensure!(
            denominator > 0
                && numerator > 0
                && numerator <= 1_000_000
                && denominator <= 1_000_000
                && u64::from(numerator) <= 240 * u64::from(denominator)
                && numerator >= denominator,
            "FPS must be between 1 and 240"
        );
        Ok(Self { numerator, denominator })
    }
    pub fn time_us(self, frame: u64) -> i64 {
        (u128::from(frame) * u128::from(self.denominator) * 1_000_000 / u128::from(self.numerator))
            as i64
    }
    pub fn samples(self, frame: u64) -> u64 {
        (u128::from(frame) * u128::from(self.denominator) * 48_000 / u128::from(self.numerator))
            as u64
    }
    pub fn ffmpeg(self) -> String {
        format!("{}/{}", self.numerator, self.denominator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VideoExportOptions {
    pub chart: PathBuf,
    pub output: PathBuf,
    pub width: u32,
    pub height: u32,
    pub fps: FrameRate,
    pub replay_slot: Option<u8>,
    pub overwrite: bool,
    pub ffmpeg: PathBuf,
    pub seed: Option<u64>,
}

pub(super) fn parse(args: &[String]) -> Result<VideoExportOptions> {
    ensure!(args.first().is_some_and(|s| s == "video"), "Use: export video PATH -o OUTPUT.mp4");
    let mut options = VideoExportOptions {
        chart: PathBuf::new(),
        output: PathBuf::new(),
        width: 1920,
        height: 1080,
        fps: FrameRate { numerator: 60, denominator: 1 },
        replay_slot: None,
        overwrite: false,
        ffmpeg: "ffmpeg".into(),
        seed: None,
    };
    let mut args = args[1..].iter();
    while let Some(arg) = args.next() {
        if arg == "--overwrite" {
            options.overwrite = true;
            continue;
        }
        if !arg.starts_with('-') {
            ensure!(options.chart.as_os_str().is_empty(), "unexpected positional argument: {arg}");
            options.chart = arg.into();
            continue;
        }
        let (flag, inline) =
            arg.split_once('=').map_or((arg.as_str(), None), |(a, b)| (a, Some(b)));
        let value = inline
            .or_else(|| args.next().map(String::as_str))
            .context("option requires a value")?;
        match flag {
            "-o" | "--output" => options.output = value.into(),
            "--fps" => options.fps = FrameRate::parse(value)?,
            "--resolution" => {
                let (w, h) = value.split_once('x').context("resolution must be WIDTHxHEIGHT")?;
                options.width = w.parse()?;
                options.height = h.parse()?;
            }
            "--replay-slot" => {
                let slot = value.parse()?;
                ensure!((1..=4).contains(&slot), "replay slot must be 1..4");
                options.replay_slot = Some(slot);
            }
            "--ffmpeg" => options.ffmpeg = value.into(),
            "--seed" => options.seed = Some(value.parse()?),
            _ => bail!("unknown export option: {flag}"),
        }
    }
    ensure!(!options.chart.as_os_str().is_empty(), "chart PATH is required");
    ensure!(
        options.output.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")),
        "output must have .mp4 extension"
    );
    ensure!(
        options.width > 0
            && options.height > 0
            && options.width <= 8192
            && options.height <= 8192
            && options.width.is_multiple_of(2)
            && options.height.is_multiple_of(2),
        "resolution must have even dimensions between 2 and 8192"
    );
    ensure!(
        options.replay_slot.is_none() || options.seed.is_none(),
        "replay seed cannot be overridden"
    );
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rational_clock_has_no_accumulated_drift() {
        let fps = FrameRate::parse("60000/1001").unwrap();
        assert_eq!(fps.time_us(60000), 1_001_000_000);
        assert_eq!(fps.samples(60000), 48_048_000);
        assert_eq!(fps.samples(1), 800);
        assert_eq!(fps.samples(2), 1601);
    }
    #[test]
    fn rejects_invalid_arguments() {
        for input in [
            "video c.bms -o a.mp4 --fps 0",
            "video c.bms -o a.mp4 --fps 60/0",
            "video c.bms -o a.mp4 --resolution 1919x1080",
            "video c.bms -o a.mp4 --replay-slot 5",
            "video c.bms -o a.mp4 --tail-seconds 3",
        ] {
            assert!(
                parse(&input.split_whitespace().map(str::to_string).collect::<Vec<_>>()).is_err()
            );
        }
    }
}
