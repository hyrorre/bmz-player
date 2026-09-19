use super::*;

/// Window-system clipboard supplied by the application when a display connection
/// and input focus are required (for example, native Wayland).
pub trait ScreenshotClipboard: fmt::Debug + Send + Sync {
    fn copy_image(&self, width: u32, height: u32, rgba: &[u8], png: &[u8]) -> Result<()>;
}

pub(super) struct ScreenshotCapture {
    pub(super) buffer: wgpu::Buffer,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) padded_bytes_per_row: u32,
    pub(super) format: wgpu::TextureFormat,
}

#[derive(Debug, Clone)]
pub(super) struct ScreenshotRequest {
    pub(super) path: PathBuf,
    pub(super) copy_to_clipboard: bool,
    pub(super) clipboard: Option<std::sync::Arc<dyn ScreenshotClipboard>>,
}

pub(super) struct ScreenshotReadback {
    pub(super) request: ScreenshotRequest,
    pub(super) capture: ScreenshotCapture,
    pub(super) rx: mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>,
}

pub(super) struct ScreenshotSaveJob {
    pub(super) path: PathBuf,
    pub(super) handle: thread::JoinHandle<Result<ScreenshotSaveOutcome>>,
}

pub(super) struct ScreenshotSaveOutcome {
    pub(super) path: PathBuf,
    pub(super) clipboard_result: Option<Result<()>>,
}

impl ScreenshotCapture {
    pub(super) fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Self {
        let bytes_per_pixel = 4;
        let unpadded_bytes_per_row = width.saturating_mul(bytes_per_pixel);
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align).saturating_mul(align);
        let buffer_size = u64::from(padded_bytes_per_row).saturating_mul(u64::from(height));
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render screenshot buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self { buffer, width, height, padded_bytes_per_row, format }
    }

    pub(super) fn copy_from_surface(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    pub(super) fn start_readback(&self) -> mpsc::Receiver<Result<(), wgpu::BufferAsyncError>> {
        let slice = self.buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        rx
    }

    pub(super) fn mapped_rgba(&self) -> Vec<u8> {
        let slice = self.buffer.slice(..);
        let mapped = slice.get_mapped_range();
        let rgba = unpack_screenshot_rgba(
            &mapped,
            self.width,
            self.height,
            self.padded_bytes_per_row,
            self.format,
        );
        drop(mapped);
        self.buffer.unmap();
        rgba
    }
}

pub(super) fn unpack_screenshot_rgba(
    mapped: &[u8],
    width: u32,
    height: u32,
    padded_bytes_per_row: u32,
    format: wgpu::TextureFormat,
) -> Vec<u8> {
    let mut rgba = vec![0; width as usize * height as usize * 4];
    let row_bytes = width as usize * 4;
    let padded_row_bytes = padded_bytes_per_row as usize;
    for y in 0..height as usize {
        let src_offset = y * padded_row_bytes;
        let dst_offset = y * row_bytes;
        rgba[dst_offset..dst_offset + row_bytes]
            .copy_from_slice(&mapped[src_offset..src_offset + row_bytes]);
    }

    if matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
    }

    rgba
}

pub(super) fn encode_screenshot_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .context("failed to encode screenshot as PNG")?;
    Ok(png)
}

pub(super) fn save_screenshot_png(path: &Path, png: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(path, png)
        .with_context(|| format!("failed to save screenshot {}", path.display()))
}

#[cfg(not(windows))]
pub(super) fn copy_screenshot_to_clipboard(
    width: u32,
    height: u32,
    rgba: &[u8],
    _png: &[u8],
) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().context("failed to open clipboard")?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Borrowed(rgba),
        })
        .context("failed to copy screenshot to clipboard")
}

#[cfg(any(windows, test))]
pub(super) fn screenshot_dibv5(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    const HEADER_SIZE: usize = 124;
    const BI_BITFIELDS: u32 = 3;
    const LCS_SRGB: u32 = 0x7352_4742;
    const LCS_GM_IMAGES: u32 = 4;

    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .context("screenshot width is too large for DIBV5")?;
    let expected_len = row_bytes
        .checked_mul(height as usize)
        .context("screenshot dimensions are too large for DIBV5")?;
    if rgba.len() != expected_len {
        return Err(anyhow!(
            "invalid screenshot RGBA length for DIBV5: expected {expected_len}, got {}",
            rgba.len()
        ));
    }
    let image_size = u32::try_from(expected_len).context("screenshot DIBV5 exceeds 4 GiB")?;
    let width = i32::try_from(width).context("screenshot width exceeds DIBV5 range")?;
    let height = i32::try_from(height).context("screenshot height exceeds DIBV5 range")?;

    let mut dib = vec![0; HEADER_SIZE];
    dib[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
    dib[4..8].copy_from_slice(&width.to_le_bytes());
    dib[8..12].copy_from_slice(&height.to_le_bytes());
    dib[12..14].copy_from_slice(&1_u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32_u16.to_le_bytes());
    dib[16..20].copy_from_slice(&BI_BITFIELDS.to_le_bytes());
    dib[20..24].copy_from_slice(&image_size.to_le_bytes());
    dib[40..44].copy_from_slice(&0x00ff_0000_u32.to_le_bytes());
    dib[44..48].copy_from_slice(&0x0000_ff00_u32.to_le_bytes());
    dib[48..52].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());
    dib[52..56].copy_from_slice(&0xff00_0000_u32.to_le_bytes());
    dib[56..60].copy_from_slice(&LCS_SRGB.to_le_bytes());
    dib[108..112].copy_from_slice(&LCS_GM_IMAGES.to_le_bytes());

    dib.reserve(expected_len);
    for row in rgba.chunks_exact(row_bytes).rev() {
        for pixel in row.as_chunks::<4>().0 {
            dib.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(dib)
}

#[cfg(windows)]
pub(super) fn copy_screenshot_to_clipboard(
    width: u32,
    height: u32,
    rgba: &[u8],
    png: &[u8],
) -> Result<()> {
    const CF_DIBV5: u32 = 17;
    const OPEN_ATTEMPTS: usize = 50;
    const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

    // Prepare every large allocation before taking the process-global Windows clipboard lock.
    let dib = screenshot_dibv5(width, height, rgba)?;
    let png_format = clipboard_win::register_format("PNG")
        .context("failed to register Windows PNG clipboard format")?;
    let mut attempt = 0;
    let clipboard = loop {
        match clipboard_win::Clipboard::new() {
            Ok(clipboard) => break clipboard,
            Err(_) if attempt < OPEN_ATTEMPTS => {
                attempt += 1;
                thread::sleep(OPEN_RETRY_DELAY);
            }
            Err(error) => {
                return Err(anyhow!(
                    "failed to open Windows clipboard after {} attempts: {error}",
                    attempt + 1
                ));
            }
        }
    };
    clipboard_win::empty().context("failed to empty Windows clipboard")?;
    clipboard_win::raw::set_without_clear(png_format.get(), png)
        .context("failed to set Windows PNG clipboard data")?;
    clipboard_win::raw::set_without_clear(CF_DIBV5, &dib)
        .context("failed to set Windows DIBV5 clipboard data")?;
    drop(clipboard);
    Ok(())
}

pub(super) fn spawn_screenshot_save_job(
    request: ScreenshotRequest,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<ScreenshotSaveJob> {
    let path = request.path.clone();
    let thread_path = path.clone();
    let handle = thread::Builder::new()
        .name("bmz-screenshot-save".to_string())
        .spawn(move || {
            let started = Instant::now();
            let png = encode_screenshot_png(width, height, &rgba)?;
            let png_encode_ms = started.elapsed().as_millis() as u64;
            let write_started = Instant::now();
            save_screenshot_png(&request.path, &png)?;
            let png_write_ms = write_started.elapsed().as_millis() as u64;
            let clipboard_started = Instant::now();
            let clipboard_result = request.copy_to_clipboard.then(|| match request.clipboard {
                Some(clipboard) => clipboard.copy_image(width, height, &rgba, &png),
                None => copy_screenshot_to_clipboard(width, height, &rgba, &png),
            });
            let clipboard_ms = clipboard_started.elapsed().as_millis() as u64;
            tracing::debug!(
                path = %request.path.display(),
                png_encode_ms,
                png_write_ms,
                clipboard_ms,
                "screenshot save job completed"
            );
            Ok(ScreenshotSaveOutcome { path: request.path, clipboard_result })
        })
        .with_context(|| {
            format!("failed to spawn screenshot save thread for {}", thread_path.display())
        })?;
    Ok(ScreenshotSaveJob { path, handle })
}

pub(super) fn finish_screenshot_save_job(job: ScreenshotSaveJob) {
    match job.handle.join() {
        Ok(Ok(outcome)) => match outcome.clipboard_result {
            Some(Ok(())) => tracing::info!(
                path = %outcome.path.display(),
                "screenshot saved and copied to clipboard"
            ),
            Some(Err(error)) => tracing::warn!(
                error = %format!("{error:#}"),
                path = %outcome.path.display(),
                "screenshot saved but clipboard copy failed"
            ),
            None => tracing::info!(path = %outcome.path.display(), "screenshot saved"),
        },
        Ok(Err(error)) => tracing::warn!(
            error = %format!("{error:#}"),
            path = %job.path.display(),
            "failed to save screenshot"
        ),
        Err(_) => tracing::warn!(
            path = %job.path.display(),
            "screenshot save thread panicked"
        ),
    }
}
