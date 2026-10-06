//! Silent stream verification without a window, app config, databases or IR.
use crate::{
    audio::{AudioRuntime, latency_json},
    config::app_config::{AudioBackend, AudioBufferSizeMode, AudioConfig, AudioSampleRateMode},
};
use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    backend: AudioBackend,
    frames: Option<u32>,
    rate: u32,
    seconds: u64,
}
pub(super) fn parse(args: &[String]) -> Result<Options> {
    if args.len() != 4 {
        bail!("Use: audio-probe auto|pipewire|pulse|alsa auto|FRAMES RATE SECONDS (3..60)");
    }
    let backend = match args[0].as_str() {
        "auto" => AudioBackend::Auto,
        "pipewire" => AudioBackend::PipeWire,
        "pulse" => AudioBackend::Pulse,
        "alsa" => AudioBackend::Alsa,
        _ => bail!("unknown audio-probe backend"),
    };
    let frames = if args[1] == "auto" { None } else { Some(args[1].parse::<u32>()?) };
    let rate = args[2].parse()?;
    let seconds = args[3].parse()?;
    if frames.is_some_and(|n| !(16..=4096).contains(&n))
        || !(8000..=384000).contains(&rate)
        || !(3..=60).contains(&seconds)
    {
        bail!("audio-probe argument outside supported range");
    }
    Ok(Options { backend, frames, rate, seconds })
}

pub fn run(options: &Options) -> Result<()> {
    let config = AudioConfig {
        backend: options.backend.clone(),
        buffer_size_mode: if options.frames.is_some() {
            AudioBufferSizeMode::Fixed
        } else {
            AudioBufferSizeMode::Auto
        },
        buffer_size: options.frames.unwrap_or(256),
        sample_rate_mode: AudioSampleRateMode::Fixed,
        sample_rate: options.rate,
        ..crate::config::app_config::AppConfig::default().audio
    };
    let environment = serde_json::json!({"schema":1,"kind":"environment","purpose":"silent_audio_probe",
        "pid":std::process::id(),"commit":env!("BMZ_BUILD_COMMIT"),"features":env!("BMZ_BUILD_FEATURES"),
        "build":if cfg!(debug_assertions){"debug"}else{"release"},
        "execution":if std::path::Path::new("/.flatpak-info").exists(){"flatpak"}else{"native"},
        "window_backend":null,"diagnostics":bmz_core::latency::diagnostics_enabled(),
        "physical_press_to_output_ns":null});
    crate::stdio::stdout_line(format_args!("BMZ_LATENCY_JSON {environment}"));
    let runtime = match AudioRuntime::open(&config) {
        Ok(runtime) => runtime,
        Err(error) => {
            let failure = serde_json::json!({"schema":1,"kind":"audio_open_failure",
                "phase":failure_phase(&error, &options.backend),
                "requested_host":format!("{:?}", options.backend),"error":format!("{error:#}")});
            crate::stdio::stdout_line(format_args!("BMZ_LATENCY_JSON {failure}"));
            return Err(error);
        }
    };
    if let Err(error) = runtime.play() {
        let failure = serde_json::json!({"schema":1,"kind":"audio_open_failure",
            "phase":"stream_start","error":format!("{error:#}")});
        crate::stdio::stdout_line(format_args!("BMZ_LATENCY_JSON {failure}"));
        return Err(error);
    }
    let mut rendered_frames = 0;
    for _ in 0..options.seconds {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let snapshot = runtime.take_diagnostics();
        rendered_frames = snapshot.rendered_frames;
        let summary = latency_json(snapshot, Some(runtime.stream_info()), false);
        crate::stdio::stdout_line(format_args!("BMZ_LATENCY_JSON {summary}"));
    }
    if rendered_frames == 0 {
        crate::stdio::stdout_line(format_args!(
            "BMZ_LATENCY_JSON {}",
            serde_json::json!({"schema":1,"kind":"audio_stream_inactive",
                "reason":"stream opened but received no frames; inspect device and server links"})
        ));
        bail!("audio-probe received no output frames");
    }
    Ok(())
}

fn failure_phase(error: &anyhow::Error, backend: &AudioBackend) -> &'static str {
    use bmz_audio::backend::cpal::CpalBackendError as E;
    if !crate::audio::available_audio_backends().contains(backend) {
        return "not_built_or_platform";
    }
    match error.downcast_ref::<E>() {
        Some(E::UnsupportedHost(_)) => "not_built_or_platform",
        Some(E::HostUnavailable(_)) => "server_or_host_initialization",
        Some(E::MissingDefaultOutputDevice | E::MissingRequestedOutputDevice(_)) => "device_absent",
        Some(E::OutputDevices(_) | E::DefaultOutputConfig(_)) => "device_query",
        Some(E::BuildStream(_) | E::PlayStream(_)) => "stream_initialization",
        _ => "configuration_or_initialization",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_device_and_unbuilt_host_are_distinct_without_opening_hardware() {
        use super::*;
        use bmz_audio::backend::cpal::{CpalBackendError, CpalHostId};
        assert_eq!(
            failure_phase(
                &CpalBackendError::MissingDefaultOutputDevice.into(),
                &AudioBackend::Auto
            ),
            "device_absent"
        );
        assert_eq!(
            failure_phase(
                &CpalBackendError::UnsupportedHost(CpalHostId::PipeWire).into(),
                &AudioBackend::Auto
            ),
            "not_built_or_platform"
        );
    }
    #[test]
    fn probe_arguments_are_bounded_and_separate_from_launch_options() {
        use crate::cli::{Command, parse_command};
        assert!(matches!(
            parse_command(["audio-probe", "pipewire", "128", "48000", "8"]).unwrap(),
            Command::AudioProbe(_)
        ));
        for args in [
            ["audio-probe", "pulse", "0", "48000", "8"],
            ["audio-probe", "alsa", "auto", "0", "8"],
            ["audio-probe", "auto", "256", "48000", "0"],
        ] {
            assert!(parse_command(args).is_err());
        }
    }
}
