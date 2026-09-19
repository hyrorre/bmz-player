use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::mpsc;
use std::task::{Context as TaskContext, Poll, RawWaker, RawWakerVTable, Waker};
use std::thread;
#[cfg(windows)]
use std::time::Duration;
use std::time::Instant;

use ab_glyph::{Font, FontArc, FontVec, Glyph, PxScale, ScaleFont, point};
use anyhow::{Context, Result, anyhow};
use image::ImageEncoder;

use crate::assets::{RgbaImageAsset, load_png_rgba};
use crate::bitmap_font::{BitmapFont, load_bitmap_font};
use crate::plan::{
    Color, DrawCommand, DrawPlan, Point, Rect, RectBatchCache, RectBatchCacheKey, RectCommand,
    TextAlign, TextCaret, TextOverflow, TextStyle, TextureId, UvRect,
};
use crate::scene::AppSceneSnapshot;
use crate::skin::{
    BlendMode, DynamicTimerRuntime, SkinClickHit, SkinContext, SkinDocument, SkinImageSize,
    SkinSliderHit,
};
use crate::ui::{EguiFrame, EguiPainter};

mod font;
mod geometry;
mod gpu;
mod pipeline;
mod screenshot;
mod text;
#[path = "renderer/text/cached_builder.rs"]
mod text_cached_builder;
#[path = "renderer/text/layout.rs"]
mod text_layout;
#[path = "renderer/text/raster_builder.rs"]
mod text_raster_builder;

#[cfg(test)]
use font::load_default_font;
pub use font::{
    SystemFontData, load_cjk_font_fallback_data, load_font_bytes_for_coverage,
    load_japanese_font_bytes, load_system_font_data_for_coverage,
};
use font::{block_on, load_default_font_fallbacks};
use geometry::*;
pub use pipeline::GpuUploader;
use pipeline::*;
pub use screenshot::ScreenshotClipboard;
use screenshot::*;
use text::*;
use text_cached_builder::*;
use text_layout::*;
use text_raster_builder::*;

mod api;
mod backend_config;
mod canvas;
mod state;
mod surface;

#[cfg(test)]
use backend_config::auto_wgpu_backends;
pub use backend_config::{
    InternalResolutionMode, SurfacePresentationStatus, WgpuBackend, WgpuFrameLatencyMode,
    WgpuPresentMode, available_wgpu_backends,
};
use backend_config::{
    LOW_LATENCY_MAXIMUM_FRAME_LATENCY, STABLE_MAXIMUM_FRAME_LATENCY, fallback_wgpu_backends,
};
#[cfg(test)]
use canvas::{CanvasFitMode, CanvasSize};
use canvas::{CanvasRenderPolicy, CanvasViewport, GpuRenderTimings, validate_rgba_texture};
pub use canvas::{RenderFrameTimings, RenderSurfaceStatus, Renderer, SurfaceSize};
pub use state::PreparedTexture;
use state::{
    InternalSceneTarget, OFFSCREEN_RECT_BATCH_TEXTURE_BASE,
    OFFSCREEN_RECT_BATCH_TEXTURE_MAX_ENTRIES, OffscreenRectBatchTextureKey, PendingTexture,
    WgpuRenderer,
};
#[cfg(test)]
use surface::resolve_maximum_frame_latency;
#[cfg(test)]
use surface::resolve_wgpu_present_mode;
use surface::{configure_surface_checked, configure_surface_settings, wgpu_present_mode_label};

#[cfg(test)]
#[path = "renderer/tests.rs"]
mod tests;
