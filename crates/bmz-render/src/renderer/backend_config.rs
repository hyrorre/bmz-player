#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WgpuBackend {
    #[default]
    Auto,
    Vulkan,
    Metal,
    Dx12,
    Gl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WgpuPresentMode {
    #[default]
    Fifo,
    FifoRelaxed,
    Immediate,
    Mailbox,
}

/// Surfaceに許可するin-flight frame数の決定方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WgpuFrameLatencyMode {
    /// macOSで実効present modeがImmediateの場合だけ2、それ以外は1を使う。
    #[default]
    Auto,
    /// 入力から表示までの待ちを優先し、常に1を使う。
    LowLatency,
    /// フレームペーシングの安定を優先し、常に2を使う。
    Stable,
}

/// ゲーム / スキン描画に使う解像度。
///
/// `Skin` は現在のスキン document の `w` / `h` が表示領域より小さい場合だけ
/// 中間 render target を使い、最終 surface へ拡大する。egui は常に surface の
/// native 解像度で描画する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InternalResolutionMode {
    #[default]
    Native,
    Skin,
}

/// Surfaceへ実際に適用されたpresent設定。要求modeがGPU/OSで利用できない場合、
/// `effective_mode`はfallback後の値になる。復旧待ち中は最後に適用できた値を返す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfacePresentationStatus {
    pub requested_mode: WgpuPresentMode,
    pub effective_mode: &'static str,
    pub maximum_frame_latency: u32,
}

/// Surfaceに許可するin-flight frame数。AutoはmacOSのImmediateだけStableを選び、
/// Windowsを含むそれ以外の環境ではLowLatencyを選ぶ。
pub(super) const LOW_LATENCY_MAXIMUM_FRAME_LATENCY: u32 = 1;
pub(super) const STABLE_MAXIMUM_FRAME_LATENCY: u32 = 2;

impl WgpuBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Vulkan => "Vulkan",
            Self::Metal => "Metal",
            Self::Dx12 => "DirectX 12",
            Self::Gl => "OpenGL",
        }
    }

    pub fn to_wgpu(self) -> wgpu::Backends {
        match self {
            Self::Auto => auto_wgpu_backends(),
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Metal => wgpu::Backends::METAL,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Gl => wgpu::Backends::GL,
        }
    }
}

/// 設定 UI に表示できるレンダリングバックエンドを、現在の OS / feature 構成から返す。
///
/// wgpu の `enabled_backend_features` は、対象プラットフォームとビルド時に有効な
/// backend feature を反映する。`Auto` は常に利用可能な論理選択肢として含める。
pub fn available_wgpu_backends() -> Vec<WgpuBackend> {
    [WgpuBackend::Auto, WgpuBackend::Vulkan, WgpuBackend::Metal, WgpuBackend::Dx12, WgpuBackend::Gl]
        .into_iter()
        .filter(|backend| {
            *backend == WgpuBackend::Auto
                || wgpu::Instance::enabled_backend_features().contains(backend.to_wgpu())
        })
        .collect()
}

pub(super) fn auto_wgpu_backends() -> wgpu::Backends {
    #[cfg(target_os = "linux")]
    {
        // Prefer Vulkan on Linux. GL/GLES remains available only as an
        // explicit fallback when Vulkan surface/device creation fails.
        wgpu::Backends::VULKAN
    }

    #[cfg(target_os = "windows")]
    {
        // Prefer DirectX 12 on Windows. Vulkan and GL remain available only as
        // explicit fallbacks when DirectX 12 surface/device creation fails.
        wgpu::Backends::DX12
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        wgpu::Backends::all()
    }
}

pub(super) fn fallback_wgpu_backends(backend: WgpuBackend) -> &'static [WgpuBackend] {
    match backend {
        #[cfg(target_os = "linux")]
        WgpuBackend::Auto => &[WgpuBackend::Vulkan, WgpuBackend::Gl],
        #[cfg(target_os = "windows")]
        WgpuBackend::Auto => &[WgpuBackend::Dx12, WgpuBackend::Vulkan, WgpuBackend::Gl],
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        WgpuBackend::Auto => &[WgpuBackend::Auto],
        WgpuBackend::Vulkan => &[WgpuBackend::Vulkan],
        WgpuBackend::Metal => &[WgpuBackend::Metal],
        WgpuBackend::Dx12 => &[WgpuBackend::Dx12],
        #[cfg(target_os = "linux")]
        WgpuBackend::Gl => &[WgpuBackend::Gl, WgpuBackend::Vulkan],
        #[cfg(target_os = "windows")]
        WgpuBackend::Gl => &[WgpuBackend::Gl, WgpuBackend::Dx12, WgpuBackend::Vulkan],
        #[cfg(target_os = "macos")]
        WgpuBackend::Gl => &[WgpuBackend::Gl, WgpuBackend::Metal],
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        WgpuBackend::Gl => &[WgpuBackend::Gl],
    }
}
