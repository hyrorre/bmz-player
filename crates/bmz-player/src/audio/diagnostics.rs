use super::AudioOutputDiagnostics;
use bmz_audio::backend::cpal::CpalStreamInfo;

pub fn latency_json(
    snapshot: AudioOutputDiagnostics,
    info: Option<&CpalStreamInfo>,
    exclusive: bool,
) -> serde_json::Value {
    let timing = snapshot.timing;
    serde_json::json!({
        "kind": "audio", "schema": 1,
        "epoch": timing.epoch, "warmup_seconds": 2,
        "callback_count": snapshot.callback_count,
        "rendered_frames": snapshot.rendered_frames,
        "frames": timing.frames, "interval_ns": timing.interval_ns,
        "duration_ns": timing.duration_ns, "prediction_ns": timing.prediction_ns,
        "invalid_predictions": timing.invalid_predictions,
        "unavailable_predictions": timing.unavailable_predictions,
        "confirmed_xruns": null,
        "stream_errors": snapshot.stream_error_count,
        "processor_overloads": snapshot.processor_overload_count,
        "lock_misses": snapshot.engine_lock_miss_count,
        "queue_drops": snapshot.command_dropped_count,
        "timeline_catch_ups": snapshot.timeline_catch_up_count,
        "stream": info.map(|i| serde_json::json!({
            "id":i.stream_id,
            "requested_host":format!("{:?}",i.requested_host),"requested_device":i.requested_device,
            "actual_host":i.actual_host,"actual_device":i.actual_device,
            "actual_device_id":i.actual_device_id,"channels":i.channels,
            "requested_rate":i.requested_rate,"actual_rate":i.actual_rate,
            "requested_frames":i.requested_frames,"supported_frames":i.supported_frames,
            "cpal_buffer":i.cpal_buffer,
            "pipewire_node_latency_request":if i.actual_host == "PipeWire" && i.cpal_buffer.starts_with("Fixed(") {
                i.cpal_buffer.strip_prefix("Fixed(").and_then(|s| s.strip_suffix(')'))
                    .map(|frames| format!("{frames}/{}", i.actual_rate))
            } else { None },
            "prediction_source": if i.requested_host == Some(bmz_audio::backend::cpal::CpalHostId::CoreAudioIoProc) {
                "hal_output_time_minus_now"
            } else if exclusive {
                "unmeasured"
            } else if i.actual_host == "PipeWire" {
                "unavailable_cpal_0.18.1_cannot_distinguish_synthetic_fallback"
            } else if i.actual_host == "PulseAudio" {
                "cpal_pulse_server_latency_interpolation_may_be_stale"
            } else { "cpal_playback_minus_callback" },
            "client_route": match i.actual_host.as_str() {
                "PipeWire" => "native_pipewire",
                "PulseAudio" => "pulse_client_server_type_unknown",
                "Alsa" => "alsa_pcm_plugin_unknown",
                _ => "platform_audio",
            },
            "server_sample_rate": null,
            "driver_quantum_frames": null,
            "server_buffer_frames": null,
        })),
    })
}
