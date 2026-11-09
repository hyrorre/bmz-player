use super::*;
use std::time::Duration;

// Allow a transient window-system transition to settle, without retrying at the
// render rate. Five attempts bound a drawable failure episode to about 1 second.
const RETRY_INTERVAL: Duration = Duration::from_millis(250);
const MAX_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SurfaceConfigureReason {
    Resize,
    Lost,
    DeviceLost,
    Outdated,
    PresentMode,
    FrameLatency,
}

/// Window presentation only. `WgpuRenderer::config` is the last successful
/// configuration; `desired` may differ even when the surface cannot be acquired.
pub(super) struct SurfaceRecovery {
    pub(super) desired: wgpu::SurfaceConfiguration,
    available: bool,
    attempts: u32,
    retry_at: Option<Instant>,
    reason: SurfaceConfigureReason,
    failure: Option<String>,
    failure_reported: bool,
}

impl SurfaceRecovery {
    pub(super) fn new(config: &wgpu::SurfaceConfiguration) -> Self {
        Self {
            desired: config.clone(),
            available: config.width > 0 && config.height > 0,
            attempts: 0,
            retry_at: None,
            reason: SurfaceConfigureReason::Resize,
            failure: None,
            failure_reported: false,
        }
    }

    pub(super) fn request(
        &mut self,
        desired: wgpu::SurfaceConfiguration,
        reason: SurfaceConfigureReason,
    ) {
        let invalidated = reason == SurfaceConfigureReason::Outdated;
        if self.desired == desired && !invalidated {
            return;
        }
        self.desired = desired;
        self.reason = reason;
        self.available = false;
        // Do not reset the deadline/budget, even for a changed request. Repeated
        // setters, resizing and minimizing cannot prolong a failing episode.
    }

    pub(super) fn refresh_physical_size(&mut self, size: SurfaceSize) {
        if !self.available && self.failure.is_none() {
            self.desired.width = size.width;
            self.desired.height = size.height;
        }
    }

    pub(super) fn fail(
        &mut self,
        reason: SurfaceConfigureReason,
        applied: &wgpu::SurfaceConfiguration,
        adapter: &wgpu::AdapterInfo,
        error: &str,
    ) {
        if self.failure.is_some() {
            return;
        }
        self.available = false;
        self.retry_at = None;
        self.reason = reason;
        let message = format!(
            "{reason:?}: {error}; backend={:?}, GPU={:?}, driver={:?} {:?}, requested={}x{} {:?} latency={}, last_applied={}x{} {:?} latency={}, attempts={}",
            adapter.backend,
            adapter.name,
            adapter.driver,
            adapter.driver_info,
            self.desired.width,
            self.desired.height,
            self.desired.present_mode,
            self.desired.desired_maximum_frame_latency,
            applied.width,
            applied.height,
            applied.present_mode,
            applied.desired_maximum_frame_latency,
            self.attempts,
        );
        tracing::error!(error = %message, "presentation stopped; renderer recreation required");
        self.failure = Some(message);
    }

    pub(super) fn blocked_status(&self) -> Option<RenderSurfaceStatus> {
        if self.desired.width == 0 || self.desired.height == 0 {
            Some(RenderSurfaceStatus::SkippedZeroSize)
        } else if !self.available {
            Some(RenderSurfaceStatus::Recovering)
        } else {
            None
        }
    }

    pub(super) fn retry_deadline(&self) -> Option<Instant> {
        (self.failure.is_none() && self.desired.width > 0 && self.desired.height > 0)
            .then_some(self.retry_at)
            .flatten()
    }

    pub(super) fn take_failure(&mut self) -> Option<String> {
        if self.failure_reported {
            return None;
        }
        let error = self.failure.clone()?;
        self.failure_reported = true;
        Some(error)
    }

    /// The configure callback is the production error-scope boundary and the
    /// failure-injection seam. Mark unavailable BEFORE it can remove presentation.
    pub(super) fn retry(
        &mut self,
        applied: &mut wgpu::SurfaceConfiguration,
        adapter: &wgpu::AdapterInfo,
        now: Instant,
        configure: impl FnOnce(&wgpu::SurfaceConfiguration) -> Result<()>,
    ) -> bool {
        if self.available
            || self.failure.is_some()
            || self.desired.width == 0
            || self.desired.height == 0
            || self.retry_at.is_some_and(|deadline| now < deadline)
        {
            return false;
        }
        self.available = false;
        self.attempts += 1;
        let backend = adapter.backend;
        let span = tracing::info_span!(
            "surface reconfiguration",
            reason = ?self.reason,
            ?backend,
            gpu = %adapter.name,
            driver = %adapter.driver,
            driver_info = %adapter.driver_info,
            physical_width = self.desired.width,
            physical_height = self.desired.height,
            desired_width = self.desired.width,
            desired_height = self.desired.height,
            applied_width = applied.width,
            applied_height = applied.height,
            present_mode = ?self.desired.present_mode,
            frame_latency = self.desired.desired_maximum_frame_latency,
            applied_present_mode = ?applied.present_mode,
            applied_frame_latency = applied.desired_maximum_frame_latency,
            attempt = self.attempts,
            max_attempts = MAX_ATTEMPTS,
        );
        let _entered = span.enter();
        match configure(&self.desired) {
            Ok(()) => {
                *applied = self.desired.clone();
                self.available = true;
                self.retry_at = None;
                tracing::info!("surface configuration succeeded; presentation available");
                self.attempts = 0;
                true
            }
            Err(error) => {
                if self.attempts >= MAX_ATTEMPTS {
                    let message = format!(
                        "Surface recovery exhausted after {} attempts: reason={:?}, backend={backend:?}, desired={}x{} {:?} latency={}, last_applied={}x{} {:?} latency={}: {error:#}",
                        self.attempts,
                        self.reason,
                        self.desired.width,
                        self.desired.height,
                        self.desired.present_mode,
                        self.desired.desired_maximum_frame_latency,
                        applied.width,
                        applied.height,
                        applied.present_mode,
                        applied.desired_maximum_frame_latency,
                    );
                    tracing::error!(error = %message, "surface recovery exhausted");
                    self.failure = Some(message);
                    self.retry_at = None;
                } else {
                    tracing::warn!(error = %format!("{error:#}"), retry_ms = RETRY_INTERVAL.as_millis(), "surface unavailable; retry scheduled");
                    self.retry_at = Some(now + RETRY_INTERVAL);
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(backend: wgpu::Backend) -> wgpu::AdapterInfo {
        wgpu::AdapterInfo {
            name: "test GPU".into(),
            vendor: 0,
            device: 0,
            device_type: wgpu::DeviceType::Other,
            device_pci_bus_id: String::new(),
            driver: "test".into(),
            driver_info: String::new(),
            backend,
            subgroup_min_size: 4,
            subgroup_max_size: 128,
            transient_saves_memory: false,
        }
    }

    fn config() -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8Unorm,
            width: 640,
            height: 480,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        }
    }

    #[test]
    fn configuration_is_committed_only_on_success_and_retries_use_latest_request() {
        let mut applied = config();
        let mut recovery = SurfaceRecovery::new(&applied);
        let mut desired = applied.clone();
        desired.width = 1920;
        recovery.request(desired.clone(), SurfaceConfigureReason::Resize);
        let now = Instant::now();
        assert_eq!(recovery.blocked_status(), Some(RenderSurfaceStatus::Recovering));
        assert!(!recovery.retry(
            &mut applied,
            &adapter(wgpu::Backend::Dx12),
            now,
            |_| anyhow::bail!("configure failed")
        ));
        assert_eq!(applied.width, 640);
        desired.width = 1280;
        desired.present_mode = wgpu::PresentMode::Immediate;
        desired.desired_maximum_frame_latency = 2;
        recovery.request(desired.clone(), SurfaceConfigureReason::PresentMode);
        assert!(!recovery.retry(
            &mut applied,
            &adapter(wgpu::Backend::Dx12),
            now + RETRY_INTERVAL / 2,
            |_| panic!("too early")
        ));
        assert!(recovery.retry(
            &mut applied,
            &adapter(wgpu::Backend::Dx12),
            now + RETRY_INTERVAL,
            |actual| {
                assert_eq!(actual, &desired);
                Ok(())
            }
        ));
        assert_eq!(applied, desired);
        assert_eq!(recovery.blocked_status(), None);
        assert!(recovery.retry_deadline().is_none());
    }

    #[test]
    fn repeated_and_changed_requests_cannot_reset_the_failure_budget() {
        let mut applied = config();
        let mut recovery = SurfaceRecovery::new(&applied);
        let now = Instant::now();
        for attempt in 0..MAX_ATTEMPTS {
            recovery.request(recovery.desired.clone(), SurfaceConfigureReason::Outdated);
            assert!(!recovery.retry(
                &mut applied,
                &adapter(wgpu::Backend::Dx12),
                now + RETRY_INTERVAL * attempt,
                |_| anyhow::bail!("persistent configure failure")
            ));
            assert_eq!(recovery.attempts, attempt + 1);
        }
        let mut desired = recovery.desired.clone();
        desired.width = 800;
        recovery.request(desired, SurfaceConfigureReason::Resize);
        assert!(!recovery.retry(
            &mut applied,
            &adapter(wgpu::Backend::Dx12),
            now + Duration::from_secs(10),
            |_| panic!("exhausted")
        ));
        assert!(recovery.take_failure().unwrap().contains("persistent configure failure"));
        assert!(recovery.take_failure().is_none());
        assert_eq!(recovery.blocked_status(), Some(RenderSurfaceStatus::Recovering));
    }

    #[test]
    fn minimizing_pauses_attempts_and_restoring_does_not_reset_them() {
        let mut applied = config();
        let mut recovery = SurfaceRecovery::new(&applied);
        let now = Instant::now();
        recovery.request(applied.clone(), SurfaceConfigureReason::Outdated);
        recovery.retry(&mut applied, &adapter(wgpu::Backend::Vulkan), now, |_| {
            anyhow::bail!("failure")
        });
        for size in [(0, 480), (640, 0), (0, 0)] {
            let mut desired = recovery.desired.clone();
            desired.width = size.0;
            desired.height = size.1;
            recovery.request(desired, SurfaceConfigureReason::Resize);
            assert!(!recovery.retry(
                &mut applied,
                &adapter(wgpu::Backend::Vulkan),
                now + Duration::from_secs(100),
                |_| panic!("zero size")
            ));
            assert_eq!(recovery.attempts, 1);
            assert_eq!(recovery.blocked_status(), Some(RenderSurfaceStatus::SkippedZeroSize));
            assert_eq!(recovery.retry_deadline(), None);
        }
        recovery.request(applied.clone(), SurfaceConfigureReason::Resize);
        assert_eq!(recovery.retry_deadline(), Some(now + RETRY_INTERVAL));
        assert!(recovery.retry(
            &mut applied,
            &adapter(wgpu::Backend::Vulkan),
            now + RETRY_INTERVAL,
            |_| Ok(())
        ));
        assert_eq!(recovery.attempts, 0);
    }
}
