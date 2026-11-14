impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer")
            .field("last_scene", &self.last_scene)
            .field("last_plan", &self.last_plan)
            .field("font_count", &self.fonts.len())
            .field("bitmap_font_count", &self.bitmap_fonts.len())
            .field("gpu_attached", &self.gpu.is_some())
            .finish()
    }
}

pub(super) fn resolve_wgpu_present_mode(
    requested: WgpuPresentMode,
    available: &[wgpu::PresentMode],
) -> wgpu::PresentMode {
    let preferred: &[wgpu::PresentMode] = match requested {
        WgpuPresentMode::Fifo => &[wgpu::PresentMode::Fifo],
        WgpuPresentMode::FifoRelaxed => &[wgpu::PresentMode::FifoRelaxed, wgpu::PresentMode::Fifo],
        WgpuPresentMode::Immediate => &[
            wgpu::PresentMode::Immediate,
            wgpu::PresentMode::Mailbox,
            wgpu::PresentMode::FifoRelaxed,
            wgpu::PresentMode::Fifo,
        ],
        WgpuPresentMode::Mailbox => {
            &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::FifoRelaxed, wgpu::PresentMode::Fifo]
        }
    };
    if let Some(mode) = preferred.iter().copied().find(|mode| available.contains(mode)) {
        return mode;
    }
    let fallback = available.first().copied().unwrap_or(wgpu::PresentMode::Fifo);
    tracing::warn!(
        requested = ?requested,
        available = ?available,
        fallback = ?fallback,
        "requested present mode is unavailable; using fallback"
    );
    fallback
}

pub(super) fn configure_surface_settings(
    config: &mut wgpu::SurfaceConfiguration,
    requested_present_mode: WgpuPresentMode,
    frame_latency_mode: WgpuFrameLatencyMode,
    available_present_modes: &[wgpu::PresentMode],
) {
    config.present_mode =
        resolve_wgpu_present_mode(requested_present_mode, available_present_modes);
    config.desired_maximum_frame_latency = resolve_maximum_frame_latency(
        frame_latency_mode,
        config.present_mode,
        cfg!(target_os = "macos"),
    );
}

pub(super) fn screenshot_surface_usage(supported: wgpu::TextureUsages) -> wgpu::TextureUsages {
    wgpu::TextureUsages::RENDER_ATTACHMENT | (supported & wgpu::TextureUsages::COPY_SRC)
}

pub(super) struct RendererErrorScopes([wgpu::ErrorScopeGuard; 3]);

impl RendererErrorScopes {
    pub(super) fn push(device: &wgpu::Device) -> Self {
        Self([
            device.push_error_scope(wgpu::ErrorFilter::Validation),
            device.push_error_scope(wgpu::ErrorFilter::Internal),
            device.push_error_scope(wgpu::ErrorFilter::OutOfMemory),
        ])
    }

    pub(super) fn pop(self) -> Option<String> {
        // Each filter catches a distinct kind of error. Pop every scope in reverse order.
        let errors: Vec<_> = self
            .0
            .into_iter()
            .rev()
            .filter_map(|scope| block_on(scope.pop()))
            .map(|error| error.to_string())
            .collect();
        (!errors.is_empty()).then(|| errors.join("; "))
    }
}

/// Capture synchronous errors reported by `Surface::configure` to the device.
pub(super) fn configure_surface_checked(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    adapter_info: &wgpu::AdapterInfo,
) -> Result<()> {
    let scopes = RendererErrorScopes::push(device);
    surface.configure(device, config);
    let Some(captured_errors) = scopes.pop() else {
        return Ok(());
    };
    tracing::error!(
        adapter_name = %adapter_info.name,
        adapter_backend = ?adapter_info.backend,
        adapter_device_type = ?adapter_info.device_type,
        driver = %adapter_info.driver,
        driver_info = %adapter_info.driver_info,
        surface_width = config.width,
        surface_height = config.height,
        surface_format = ?config.format,
        present_mode = ?config.present_mode,
        alpha_mode = ?config.alpha_mode,
        desired_maximum_frame_latency = config.desired_maximum_frame_latency,
        captured_error = %captured_errors,
        "failed to configure renderer surface"
    );
    Err(anyhow!(
        "surface configure failed: adapter_name={:?}, adapter_backend={:?}, adapter_device_type={:?}, driver={:?}, driver_info={:?}, width={}, height={}, format={:?}, present_mode={:?}, alpha_mode={:?}, desired_maximum_frame_latency={}, captured_error={}",
        adapter_info.name,
        adapter_info.backend,
        adapter_info.device_type,
        adapter_info.driver,
        adapter_info.driver_info,
        config.width,
        config.height,
        config.format,
        config.present_mode,
        config.alpha_mode,
        config.desired_maximum_frame_latency,
        captured_errors
    ))
}

pub(super) fn resolve_maximum_frame_latency(
    mode: WgpuFrameLatencyMode,
    effective_present_mode: wgpu::PresentMode,
    is_macos: bool,
) -> u32 {
    match mode {
        WgpuFrameLatencyMode::LowLatency => LOW_LATENCY_MAXIMUM_FRAME_LATENCY,
        WgpuFrameLatencyMode::Stable => STABLE_MAXIMUM_FRAME_LATENCY,
        WgpuFrameLatencyMode::Auto
            if is_macos && effective_present_mode == wgpu::PresentMode::Immediate =>
        {
            STABLE_MAXIMUM_FRAME_LATENCY
        }
        WgpuFrameLatencyMode::Auto => LOW_LATENCY_MAXIMUM_FRAME_LATENCY,
    }
}

pub(super) fn wgpu_present_mode_label(mode: wgpu::PresentMode) -> &'static str {
    match mode {
        wgpu::PresentMode::AutoVsync => "AutoVsync",
        wgpu::PresentMode::AutoNoVsync => "AutoNoVsync",
        wgpu::PresentMode::Fifo => "Fifo",
        wgpu::PresentMode::FifoRelaxed => "FifoRelaxed",
        wgpu::PresentMode::Immediate => "Immediate",
        wgpu::PresentMode::Mailbox => "Mailbox",
    }
}
use super::*;

#[cfg(test)]
#[derive(Default)]
pub(super) struct SurfaceTest {
    pub(super) failures_remaining: u32,
    pub(super) configure_calls: u32,
    pub(super) acquire_calls: u32,
    pub(super) acquire_result: Option<wgpu::CurrentSurfaceTexture>,
    pub(super) lose_device_during_configure: bool,
}

#[cfg(test)]
#[path = "surface_recovery_tests.rs"]
mod recovery_tests;
