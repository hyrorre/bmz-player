use super::*;

impl WgpuRenderer {
    pub(in crate::renderer) fn new_with_fallbacks<T, D>(
        (window, display): (T, D),
        size: SurfaceSize,
        present_mode: WgpuPresentMode,
        frame_latency_mode: WgpuFrameLatencyMode,
        backend: WgpuBackend,
        default_font_coverage: bmz_font::FontCoverage,
        default_font_search_paths: Vec<PathBuf>,
    ) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>> + Clone,
        D: wgpu::wgt::WgpuHasDisplayHandle + Clone,
    {
        try_backend_candidates(backend, wgpu::Instance::enabled_backend_features(), |candidate| {
            Self::new(
                Some((window.clone(), Box::new(display.clone()))),
                size,
                present_mode,
                frame_latency_mode,
                candidate,
                default_font_coverage,
                default_font_search_paths.clone(),
            )
        })
    }

    pub(in crate::renderer) fn new<T>(
        window: Option<(T, Box<dyn wgpu::wgt::WgpuHasDisplayHandle>)>,
        size: SurfaceSize,
        present_mode: WgpuPresentMode,
        frame_latency_mode: WgpuFrameLatencyMode,
        backend: WgpuBackend,
        default_font_coverage: bmz_font::FontCoverage,
        default_font_search_paths: Vec<PathBuf>,
    ) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>>,
    {
        let (window, display) = match window {
            Some((window, display)) => (Some(window), Some(display)),
            None => (None, None),
        };
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = backend.to_wgpu();
        descriptor.display = display;
        let instance = wgpu::Instance::new(descriptor);
        let surface = window
            .map(|window| instance.create_surface(window))
            .transpose()
            .context("failed to create wgpu surface")?;
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface.as_ref(),
        }))
        .context("no compatible GPU adapter found")?;
        let adapter_info = adapter.get_info();
        tracing::info!(
            candidate_backend = ?backend,
            adapter_name = %adapter_info.name,
            adapter_backend = ?adapter_info.backend,
            adapter_device_type = ?adapter_info.device_type,
            driver = %adapter_info.driver,
            driver_info = %adapter_info.driver_info,
            "selected renderer adapter for backend candidate"
        );
        // beatoraja スキンには 8192px を超える縦長/横長 PNG (背景アニメシート等) が
        // 含まれることがある。Apple Silicon / モダンGPU 環境では 16384px までは許容
        // されるので、アダプタが報告する上限まで広げて取得する。
        let adapter_limits = adapter.limits();
        let required_limits = if adapter_info.backend == wgpu::Backend::Gl {
            // BMZ and egui use render passes with uniform buffers and sampled textures.
            // GLES 3.0 can have no storage bindings or compute support.
            wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter_limits.clone())
                .using_alignment(adapter_limits.clone())
        } else {
            wgpu::Limits {
                max_texture_dimension_2d: adapter_limits.max_texture_dimension_2d,
                max_texture_dimension_1d: adapter_limits.max_texture_dimension_1d,
                ..Default::default()
            }
        };
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("bmz-render device"),
            required_features: wgpu::Features::empty(),
            required_limits,
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .context("failed to request wgpu device")?;
        let device_loss = std::sync::Arc::new(DeviceLoss::default());
        let callback_state = std::sync::Arc::clone(&device_loss);
        device.set_device_lost_callback(move |reason, message| {
            callback_state.notify(reason, message)
        });
        let capabilities = surface.as_ref().map(|surface| surface.get_capabilities(&adapter));
        let present_modes = capabilities
            .as_ref()
            .map(|capabilities| capabilities.present_modes.clone())
            .unwrap_or_else(|| vec![wgpu::PresentMode::Fifo]);
        let mut config = if let Some(surface) = &surface {
            surface
                .get_default_config(&adapter, size.width, size.height)
                .ok_or_else(|| anyhow!("surface is not supported by the selected adapter"))?
        } else {
            wgpu::SurfaceConfiguration {
                usage: screenshot_surface_usage(wgpu::TextureUsages::COPY_SRC),
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 1,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: vec![],
            }
        };
        // sRGB フレームバッファだと PNG の sRGB 値が二重 gamma エンコードされて白っぽくなる。
        // beatoraja (libGDX) は GL_FRAMEBUFFER_SRGB を使わないため値をそのまま表示する。
        // それと合わせるため sRGB サフィックスを除去して non-sRGB サーフェスとして使う。
        config.format = config.format.remove_srgb_suffix();
        if let Some(capabilities) = capabilities {
            config.usage = screenshot_surface_usage(capabilities.usages);
        }
        configure_surface_settings(&mut config, present_mode, frame_latency_mode, &present_modes);
        if let Some(surface) = &surface
            && size.is_drawable()
        {
            configure_surface_checked(surface, &device, &config, &adapter_info)
                .context("initial surface configuration failed")?;
        }
        anyhow::ensure!(!device_loss.is_lost(), "device lost during initial surface configuration");
        anyhow::ensure!(
            surface.is_some()
                || (size.width > 0
                    && size.height > 0
                    && size.width <= adapter_limits.max_texture_dimension_2d
                    && size.height <= adapter_limits.max_texture_dimension_2d),
            "unsupported export dimensions"
        );
        let resource_errors = RendererErrorScopes::push(&device);
        let export_target = surface.is_none().then(|| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bmz offline output"),
                size: wgpu::Extent3d {
                    width: size.width,
                    height: size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: config.format,
                usage: config.usage,
                view_formats: &[],
            })
        });
        tracing::info!(
            requested = ?present_mode,
            effective = ?config.present_mode,
            available = ?present_modes,
            desired_maximum_frame_latency = config.desired_maximum_frame_latency,
            candidate_backend = ?backend,
            surface_width = config.width,
            surface_height = config.height,
            surface_format = ?config.format,
            alpha_mode = ?config.alpha_mode,
            "configured initial renderer surface"
        );
        let rect_pipeline = create_rect_pipeline(&device, config.format);
        let image_bind_group_layout = create_image_bind_group_layout(&device);
        let image_sampler = create_image_sampler(&device);
        let image_sampler_linear = create_image_sampler_linear(&device);
        let image_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::Normal,
        );
        let image_add_pipeline =
            create_image_pipeline(&device, config.format, &image_bind_group_layout, BlendMode::Add);
        let image_subtract_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::Subtract,
        );
        let image_multiply_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::Multiply,
        );
        let image_premultiplied_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::Premultiplied,
        );
        let image_invert_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::InvertRgb,
        );
        let image_layer_pipeline = create_image_pipeline(
            &device,
            config.format,
            &image_bind_group_layout,
            BlendMode::LayerMask,
        );
        let image_composite_pipelines = (0..=12)
            .map(|mode| {
                create_image_pipeline(
                    &device,
                    config.format,
                    &image_bind_group_layout,
                    BlendMode::Composite(mode),
                )
            })
            .collect();
        let upscale_pipeline =
            create_upscale_pipeline(&device, config.format, &image_bind_group_layout);
        let upscale_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bmz-render internal upscale buffer"),
            size: IMAGE_INSTANCE_BYTES as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let image_textures = create_default_image_textures(&device, &queue);
        let text_bind_group_layout = create_text_bind_group_layout(&device);
        let text_sampler = create_text_sampler(&device);
        let text_pipeline = create_text_pipeline(&device, config.format, &text_bind_group_layout);
        let egui = EguiPainter::new(&device, config.format);
        if let Some(errors) = resource_errors.pop() {
            anyhow::bail!("failed to initialize renderer GPU resources: {errors}");
        }
        anyhow::ensure!(!device_loss.is_lost(), "device lost during renderer GPU initialization");

        let max_texture_dimension_2d = device.limits().max_texture_dimension_2d;
        Ok(Self {
            device,
            device_loss,
            queue,
            adapter_info,
            surface_recovery: surface.as_ref().map(|_| SurfaceRecovery::new(&config)),
            config,
            #[cfg(test)]
            surface_test: None,
            present_modes,
            rect_pipeline,
            rect_buffer: None,
            rect_buffer_capacity: 0,
            image_pipeline,
            image_add_pipeline,
            image_multiply_pipeline,
            image_subtract_pipeline,
            image_premultiplied_pipeline,
            image_layer_pipeline,
            image_invert_pipeline,
            image_composite_pipelines,
            image_bind_group_layout,
            image_sampler,
            image_sampler_linear,
            upscale_pipeline,
            upscale_buffer,
            upscale_rect: None,
            internal_scene_target: None,
            screenshot_target: None,
            screenshot_blit_buffer: None,
            image_textures,
            texture_uploads: TextureUploads::default(),
            image_bind_group_cache: HashMap::new(),
            image_bind_group_scratch: Vec::new(),
            geometry_scratch: PlanGeometry::default(),
            ambient: None,
            offscreen_rect_batches: HashMap::new(),
            next_offscreen_rect_batch_texture_id: OFFSCREEN_RECT_BATCH_TEXTURE_BASE,
            image_buffer: None,
            image_buffer_capacity: 0,
            text_pipeline,
            text_bind_group_layout,
            text_sampler,
            text_texture: None,
            text_texture_view: None,
            text_bind_group: None,
            text_texture_size: AtlasSize::default(),
            text_atlas: TextAtlasCache::for_texture_limit(max_texture_dimension_2d),
            text_buffer: None,
            text_buffer_capacity: 0,
            default_fonts: load_default_font_fallbacks(
                default_font_coverage,
                &default_font_search_paths,
            ),
            default_font_coverage,
            default_font_search_paths,
            egui,
            pending_screenshot_readbacks: Vec::new(),
            screenshot_save_jobs: Vec::new(),
            surface,
            export_target,
        })
    }
}

fn try_backend_candidates<T>(
    requested: WgpuBackend,
    enabled: wgpu::Backends,
    mut initialize: impl FnMut(WgpuBackend) -> Result<T>,
) -> Result<T> {
    let mut failures = Vec::new();
    for &candidate in fallback_wgpu_backends(requested) {
        if !enabled.intersects(candidate.to_wgpu()) {
            failures.push(format!("{candidate:?}: backend unavailable in this build"));
            continue;
        }
        tracing::info!(?requested, ?candidate, "initializing renderer backend candidate");
        match initialize(candidate) {
            Ok(renderer) => return Ok(renderer),
            Err(error) => {
                tracing::warn!(?requested, ?candidate, error = %format_args!("{error:#}"), "failed to initialize renderer backend candidate");
                failures.push(format!("{candidate:?}: {error:#}"));
            }
        }
    }
    anyhow::bail!("could not initialize requested renderer {requested:?}: {}", failures.join("\n"))
}

#[cfg(all(test, any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    fn native_candidate() -> WgpuBackend {
        #[cfg(target_os = "windows")]
        return WgpuBackend::Dx12;
        #[cfg(target_os = "linux")]
        return WgpuBackend::Vulkan;
        #[cfg(target_os = "macos")]
        return WgpuBackend::Metal;
    }

    #[test]
    fn opengl_failure_retries_and_stops_at_first_success() {
        let mut attempted = Vec::new();
        let chosen = try_backend_candidates(WgpuBackend::Gl, wgpu::Backends::all(), |candidate| {
            attempted.push(candidate);
            anyhow::ensure!(candidate != WgpuBackend::Gl, "injected surface configure failure");
            Ok(candidate)
        })
        .unwrap();
        assert_eq!(chosen, native_candidate());
        assert_eq!(attempted, [WgpuBackend::Gl, native_candidate()]);
    }

    #[test]
    fn opengl_success_does_not_try_other_backends() {
        let chosen = try_backend_candidates(WgpuBackend::Gl, wgpu::Backends::all(), |candidate| {
            assert_eq!(candidate, WgpuBackend::Gl);
            Ok(candidate)
        })
        .unwrap();
        assert_eq!(chosen, WgpuBackend::Gl);
    }

    #[test]
    fn reports_unavailable_backends_and_initialization_errors() {
        let native = native_candidate();
        let error =
            try_backend_candidates(WgpuBackend::Gl, native.to_wgpu(), |candidate| -> Result<()> {
                assert_eq!(candidate, native);
                anyhow::bail!("injected device creation failure");
            })
            .unwrap_err();
        let error = error.to_string();
        assert!(error.contains("Gl: backend unavailable in this build"));
        assert!(error.contains(&format!("{native:?}: injected device creation failure")));
    }
}
