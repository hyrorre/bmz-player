//! Metadata on the window thread only. No subprocesses or server probes here.
use winit::raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

pub(crate) fn log(window: &winit::window::Window) {
    if !bmz_core::latency::diagnostics_enabled() {
        return;
    }
    let display = window.display_handle().ok().map(|h| h.as_raw());
    let window_backend = match display {
        Some(RawDisplayHandle::Wayland(_)) => "wayland",
        Some(RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)) => "x11",
        Some(RawDisplayHandle::AppKit(_)) => "appkit",
        Some(RawDisplayHandle::Windows(_)) => "win32",
        _ => "unknown",
    };
    let summary = serde_json::json!({
        "schema": 1, "kind": "environment", "pid": std::process::id(),
        "commit": env!("BMZ_BUILD_COMMIT"), "features": env!("BMZ_BUILD_FEATURES"),
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "execution": if std::path::Path::new("/.flatpak-info").exists() { "flatpak" } else { "native" },
        "window_backend": window_backend, "xwayland": null,
        "session_type_hint": std::env::var("XDG_SESSION_TYPE").ok(),
        "diagnostics": true, "stall_test": crate::cli::latency_stall_test_enabled(),
        "quantiles": "logarithmic bucket upper bound capped by observed max",
        "physical_press_to_output_ns": null,
        "winit_os_to_receive_ns": null,
        "manual_sound_enqueue_to_render_ns": null,
        "matched_event_total_ns": null,
    });
    tracing::info!("BMZ_LATENCY_JSON {summary}");
}
