use crate::cli::VideoExportOptions;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
};

pub fn command(executable: &Path) -> Command {
    let mut cmd = Command::new(executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
    cmd
}

pub fn preflight(options: &VideoExportOptions) -> Result<String> {
    let output = command(&options.ffmpeg).arg("-encoders").output().context(
        "cannot run FFmpeg; install FFmpeg with libx264 and AAC or specify --ffmpeg PATH",
    )?;
    ensure!(output.status.success(), "FFmpeg encoder discovery failed");
    let encoders = String::from_utf8_lossy(&output.stdout);
    for encoder in ["libx264", "aac"] {
        ensure!(
            encoders.lines().any(|line| line.split_whitespace().nth(1) == Some(encoder)),
            "FFmpeg encoder not available: {encoder}"
        );
    }
    let version = command(&options.ffmpeg).arg("-version").output()?;
    Ok(String::from_utf8_lossy(&version.stdout).lines().next().unwrap_or_default().to_string())
}

pub struct Encoder {
    pub directory: PathBuf,
    child: Option<Child>,
    stdin: Option<BufWriter<ChildStdin>>,
    audio: Option<BufWriter<File>>,
}

impl Encoder {
    pub fn new(options: &VideoExportOptions) -> Result<Self> {
        let parent =
            options.output.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let directory = parent.join(format!(".bmz-export-{}", super::random_id()?));
        fs::create_dir(&directory).context("cannot create export temporary directory")?;
        let mut encoder = Self { directory, child: None, stdin: None, audio: None };
        encoder.audio = Some(BufWriter::new(File::create(encoder.directory.join("audio.f32"))?));
        let stderr = File::create(encoder.directory.join("ffmpeg.log"))?;
        let mut child = command(&options.ffmpeg)
            .args(["-f", "rawvideo", "-pixel_format", "rgba", "-video_size"])
            .arg(format!("{}x{}", options.width, options.height))
            .args([
                "-framerate",
                &options.fps.ffmpeg(),
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-fps_mode",
                "passthrough",
            ])
            .arg(encoder.directory.join("video.mp4"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()?;
        encoder.stdin = child.stdin.take().map(BufWriter::new);
        encoder.child = Some(child);
        Ok(encoder)
    }
    pub fn frame(&mut self, rgba: &[u8]) -> Result<()> {
        self.stdin.as_mut().context("encoder closed")?.write_all(rgba).with_context(|| {
            format!(
                "video encoding failed: {}",
                fs::read_to_string(self.directory.join("ffmpeg.log")).unwrap_or_default()
            )
        })
    }
    pub fn audio(&mut self, pcm: &[f32]) -> Result<()> {
        let writer = self.audio.as_mut().context("audio closed")?;
        for sample in pcm {
            ensure!(sample.is_finite(), "non-finite mixed audio sample");
            writer.write_all(&sample.clamp(-1.0, 1.0).to_le_bytes())?;
        }
        Ok(())
    }
    pub fn finish(
        &mut self,
        options: &VideoExportOptions,
        metadata: &serde_json::Value,
    ) -> Result<()> {
        if let Some(mut input) = self.stdin.take() {
            input.flush()?;
        }
        if let Some(mut audio) = self.audio.take() {
            audio.flush()?;
        }
        let status = self.child.as_mut().context("encoder missing")?.wait()?;
        self.child = None;
        ensure!(
            status.success(),
            "FFmpeg video encoding failed: {}",
            fs::read_to_string(self.directory.join("ffmpeg.log")).unwrap_or_default()
        );
        let output = command(&options.ffmpeg)
            .arg("-i")
            .arg(self.directory.join("video.mp4"))
            .args(["-f", "f32le", "-ar", "48000", "-ac", "2", "-i"])
            .arg(self.directory.join("audio.f32"))
            .args([
                "-map",
                "0:v:0",
                "-map",
                "1:a:0",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
                "-b:a",
                "320k",
                "-movflags",
                "+faststart",
            ])
            .arg(self.directory.join("complete.mp4"))
            .output()?;
        ensure!(
            output.status.success(),
            "FFmpeg mux failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(self.directory.join("metadata.json"), serde_json::to_vec_pretty(metadata)?)?;
        // Both completed artifacts are staged on the destination filesystem.
        let sidecar = options.output.with_extension("export.json");
        publish(&self.directory.join("metadata.json"), &sidecar, options.overwrite)?;
        publish(&self.directory.join("complete.mp4"), &options.output, options.overwrite)?;
        Ok(())
    }
}

fn publish(source: &Path, destination: &Path, overwrite: bool) -> Result<()> {
    if overwrite {
        fs::rename(source, destination)?;
    } else {
        fs::hard_link(source, destination)
            .context("destination exists or cannot publish completed export")?;
    }
    Ok(())
}

impl Drop for Encoder {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin.take();
        self.audio.take();
        // This directory was uniquely created by this invocation; no user files are stored here.
        let _ = fs::remove_dir_all(&self.directory);
    }
}
