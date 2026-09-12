impl Renderer {
    /// Create a GPU target without a window or presentation clock.
    pub fn attach_offscreen(&mut self, size: SurfaceSize) -> Result<()> {
        let mut gpu = WgpuRenderer::new::<wgpu::SurfaceTarget<'static>>(
            None,
            size,
            self.present_mode,
            self.frame_latency_mode,
            self.backend,
            self.default_font_coverage,
            self.default_font_search_paths.clone(),
        )?;
        for texture in self.pending_textures.drain(..) {
            gpu.upsert_rgba_texture(texture.id, texture.width, texture.height, &texture.rgba)?;
        }
        self.gpu = Some(gpu);
        Ok(())
    }

    /// Wait for the complete rendered frame; never substitute or drop frames.
    pub fn read_offscreen_rgba(&self) -> Result<Vec<u8>> {
        let gpu = self.gpu.as_ref().context("GPU is not attached")?;
        let target = gpu.export_target.as_ref().context("not an offscreen renderer")?;
        let capture = ScreenshotCapture::new(
            &gpu.device,
            gpu.config.width,
            gpu.config.height,
            gpu.config.format,
        );
        let mut encoder =
            gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        capture.copy_from_surface(&mut encoder, target);
        gpu.queue.submit([encoder.finish()]);
        let rx = capture.start_readback();
        gpu.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv().context("frame readback disconnected")??;
        Ok(capture.mapped_rgba())
    }

    pub fn attach_surface<T>(&mut self, window: T, size: SurfaceSize) -> Result<()>
    where
        T: Into<wgpu::SurfaceTarget<'static>> + Clone,
    {
        if !size.is_drawable() {
            self.gpu = None;
            return Ok(());
        }

        let mut gpu = WgpuRenderer::new_with_fallbacks(
            window,
            size,
            self.present_mode,
            self.frame_latency_mode,
            self.backend,
            self.default_font_coverage,
            self.default_font_search_paths.clone(),
        )?;
        for texture in self.pending_textures.drain(..) {
            if let Err(error) =
                gpu.upsert_rgba_texture(texture.id, texture.width, texture.height, &texture.rgba)
            {
                tracing::warn!(texture_id = texture.id.0, %error, "skipping unsupported pending texture");
            }
        }
        self.gpu = Some(gpu);
        Ok(())
    }

    /// Drop GPU resources that depend on the window surface while the app still
    /// owns the native window.
    pub fn detach_surface(&mut self) {
        self.pending_egui = None;
        let Some(gpu) = self.gpu.take() else {
            return;
        };
        gpu.wait_idle_before_drop();
    }

    pub fn upsert_rgba_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<()> {
        validate_rgba_texture(width, height, &rgba)?;
        if let Some(gpu) = &mut self.gpu {
            gpu.upsert_rgba_texture(id, width, height, &rgba)?;
        } else {
            self.pending_textures.push(PendingTexture { id, width, height, rgba });
        }
        Ok(())
    }

    pub fn upsert_rgba_texture_ref(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<()> {
        validate_rgba_texture(width, height, rgba)?;
        if let Some(gpu) = &mut self.gpu {
            gpu.upsert_rgba_texture(id, width, height, rgba)?;
        } else {
            self.pending_textures.push(PendingTexture { id, width, height, rgba: rgba.to_vec() });
        }
        Ok(())
    }

    pub fn upsert_image_asset(&mut self, id: TextureId, asset: &RgbaImageAsset) -> Result<()> {
        asset.validate()?;
        self.upsert_rgba_texture_ref(id, asset.width, asset.height, &asset.pixels)
    }

    /// skin upload worker 用に、GPU アップロード機能の clone を取り出す。
    /// surface 未接続 (`gpu` が None) の間は `None`。
    /// 返り値は `Send + Clone` なので別スレッドへ渡せる。
    pub fn gpu_uploader(&self) -> Option<GpuUploader> {
        self.gpu
            .as_ref()
            .map(|gpu| GpuUploader { device: gpu.device.clone(), queue: gpu.queue.clone() })
    }

    /// worker でアップロード済みの `PreparedTexture` をテクスチャ表へ差し込む。
    /// surface 未接続時は (worker が存在しないため通常起きないが) 無視する。
    pub fn insert_prepared_texture(&mut self, id: TextureId, prepared: PreparedTexture) {
        if let Some(gpu) = &mut self.gpu {
            gpu.image_textures.insert(id, prepared);
            gpu.image_bind_group_cache.retain(|(texture_id, _), _| *texture_id != id);
        } else {
            tracing::warn!(
                texture_id = id.0,
                "dropping prepared texture because gpu surface is not attached"
            );
        }
    }

    pub fn active_skin_texture_ids(&self) -> std::collections::HashSet<crate::skin::SkinTextureId> {
        [
            &self.play_skin_context,
            &self.select_skin_context,
            &self.decide_skin_context,
            &self.result_skin_context,
        ]
        .into_iter()
        .flat_map(|context| context.texture_ids())
        .collect()
    }

    pub fn remove_image_texture(&mut self, id: TextureId) {
        if id == TextureId(0) {
            return;
        }
        self.pending_textures.retain(|texture| texture.id != id);
        if let Some(gpu) = &mut self.gpu {
            gpu.image_textures.remove(&id);
            gpu.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
        }
    }

    pub fn load_png_texture(&mut self, id: TextureId, path: &std::path::Path) -> Result<()> {
        let asset = load_png_rgba(path)?;
        self.upsert_image_asset(id, &asset)
    }

    pub fn load_font(&mut self, id: impl Into<String>, path: &std::path::Path) -> Result<()> {
        let id = id.into();
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read font: {}", path.display()))?;
        let font = FontArc::try_from_vec(bytes)
            .map_err(|error| anyhow!("failed to parse font {}: {error}", path.display()))?;
        self.insert_vector_font(id, font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
        Ok(())
    }

    pub fn load_bitmap_font(
        &mut self,
        id: impl Into<String>,
        path: &std::path::Path,
    ) -> Result<()> {
        let font = load_bitmap_font(path)?;
        self.insert_bitmap_font_entry(id.into(), font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
        Ok(())
    }

    /// 事前に読み込んだフォントバイト列を登録する。
    /// バックグラウンドスレッドで I/O を済ませた後に main スレッドから登録する用途。
    pub fn install_font_bytes(&mut self, id: impl Into<String>, bytes: Vec<u8>) -> Result<()> {
        let font = FontArc::try_from_vec(bytes)
            .map_err(|error| anyhow!("failed to parse font bytes: {error}"))?;
        self.insert_vector_font(id.into(), font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
        Ok(())
    }

    /// 事前にパース済みの bitmap font を登録する。
    pub fn install_bitmap_font(&mut self, id: impl Into<String>, font: BitmapFont) {
        self.insert_bitmap_font_entry(id.into(), font);
        if let Some(gpu) = &mut self.gpu {
            gpu.reset_text_atlas();
        }
    }

    pub(super) fn insert_vector_font(&mut self, id: String, font: FontArc) {
        self.bitmap_fonts.remove(&id);
        self.fonts.insert(id, font);
    }

    pub(super) fn insert_bitmap_font_entry(&mut self, id: String, font: BitmapFont) {
        self.fonts.remove(&id);
        self.bitmap_fonts.insert(id, font);
    }

    pub fn set_skin_context(&mut self, skin_context: SkinContext) {
        self.set_play_skin_context(skin_context, false);
    }

    /// `preserve_dynamic_timers` が true のとき、プレイ中のスキン差し替え向けに
    /// `timer_observe_boolean` の経過時刻を維持する。
    pub fn set_play_skin_context(
        &mut self,
        skin_context: SkinContext,
        preserve_dynamic_timers: bool,
    ) {
        if !preserve_dynamic_timers {
            self.play_dynamic_timer_runtime.reset();
        }
        self.play_skin_context = skin_context;
    }

    pub fn set_select_skin_context(&mut self, skin_context: SkinContext) {
        self.select_dynamic_timer_runtime.reset();
        self.select_skin_context = skin_context;
    }

    pub fn set_decide_skin_context(&mut self, skin_context: SkinContext) {
        self.decide_dynamic_timer_runtime.reset();
        self.decide_skin_context = skin_context;
    }

    pub fn set_result_skin_context(&mut self, skin_context: SkinContext) {
        self.result_skin_context = skin_context;
        self.result_dynamic_timer_runtime.reset_for_document(self.result_skin_context.document());
    }

    /// リザルトスキンが定義する内部 runtime event を dispatch する。
    ///
    /// クリック入力などを app 層が解決した後に呼ぶ。event が未定義なら false。
    pub fn dispatch_result_skin_runtime_event(&mut self, event_id: i32) -> bool {
        let Some(document) = self.result_skin_context.document() else {
            return false;
        };
        self.result_dynamic_timer_runtime.dispatch_runtime_event(document, event_id)
    }

    /// 同じリザルトスキンで新しい scene に入る際、runtime state を初期化する。
    pub fn reset_result_skin_runtime(&mut self) {
        self.result_dynamic_timer_runtime.reset_for_document(self.result_skin_context.document());
    }

    /// リザルトスキンが宣言する終了フェードアウト時間 (ms)。
    /// ドキュメントスキンが無い場合や未指定の場合は 0 を返す。
    pub fn result_skin_fadeout_ms(&self) -> i32 {
        self.result_skin_context.document().map(|document| document.fadeout).unwrap_or(0).max(0)
    }

    pub fn result_skin_timer_animation_duration_ms(&self, timer: i32) -> i32 {
        self.result_skin_context.timer_animation_duration_ms(timer)
    }

    /// 選曲スキンの document (設定 UI が property/offset 定義を読むため公開)。
    pub fn select_skin_document(&self) -> Option<&SkinDocument> {
        self.select_skin_context.document()
    }

    /// Surface 全体の左上原点正規化座標を select skin canvas 座標へ変換する。
    /// レターボックス領域なら `None` を返す。
    pub fn select_skin_mouse_position(
        &self,
        surface_position: Option<(f32, f32)>,
    ) -> Option<(f32, f32)> {
        let (x, y) = surface_position?;
        self.select_skin_canvas_point(x, y)
    }

    /// Surface 全体の左上原点正規化座標を result skin canvas 座標へ変換する。
    /// レターボックス領域なら `None` を返す。
    pub fn result_skin_mouse_position(
        &self,
        surface_position: Option<(f32, f32)>,
    ) -> Option<(f32, f32)> {
        let (x, y) = surface_position?;
        self.result_skin_canvas_point(x, y)
    }

    pub fn select_skin_click_hit(
        &self,
        snapshot: &crate::scene::SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        let (x, y) = self.select_skin_canvas_point(x, y)?;
        self.select_skin_context.select_click_hit(snapshot, x, y)
    }

    /// Search input bounds in normalized surface coordinates, including the
    /// select skin canvas viewport and letterboxing.
    pub fn select_skin_search_input_rect(
        &self,
        snapshot: &crate::scene::SelectSnapshot,
    ) -> Option<Rect> {
        let rect = self.select_skin_context.select_search_input_rect(snapshot)?;
        let Some(surface) = self.gpu.as_ref().map(WgpuRenderer::surface_size) else {
            return Some(rect);
        };
        let viewport =
            CanvasViewport::from_policy(surface, self.select_skin_canvas_render_policy());
        Some(viewport.transform_rect(rect))
    }

    pub fn result_skin_click_hit(
        &self,
        snapshot: &crate::scene::ResultSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinClickHit> {
        let (x, y) = self.result_skin_canvas_point(x, y)?;
        let document = self.result_skin_context.document()?;
        let mut state = crate::plan::result_skin_draw_state_for_document(snapshot, document);
        state.start_input_ms =
            crate::skin::skin_start_input_elapsed_ms(state.elapsed_ms, document.input);
        self.result_skin_context.result_click_hit(&state, x, y)
    }

    pub fn result_skin_slider_hit(
        &self,
        snapshot: &crate::scene::ResultSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let (x, y) = self.result_skin_canvas_point(x, y)?;
        let document = self.result_skin_context.document()?;
        let mut state = crate::plan::result_skin_draw_state_for_document(snapshot, document);
        state.start_input_ms =
            crate::skin::skin_start_input_elapsed_ms(state.elapsed_ms, document.input);
        self.result_skin_context.result_slider_hit(&state, x, y)
    }

    pub fn select_skin_slider_hit(
        &self,
        snapshot: &crate::scene::SelectSnapshot,
        x: f32,
        y: f32,
    ) -> Option<SkinSliderHit> {
        let (x, y) = self.select_skin_canvas_point(x, y)?;
        self.select_skin_context.select_slider_hit(snapshot, x, y)
    }

    /// プレイスキンの document。
    pub fn play_skin_document(&self) -> Option<&SkinDocument> {
        self.play_skin_context.document()
    }

    /// Play skin の beatoraja `practice` destination を、surface 全体の
    /// 左上原点正規化座標へ変換して返す。
    pub fn play_skin_practice_position(&self) -> Option<(f32, f32)> {
        let position = self.play_skin_context.document()?.practice_destination_position()?;
        let Some(surface) = self.gpu.as_ref().map(WgpuRenderer::surface_size) else {
            return Some(position);
        };
        let viewport = CanvasViewport::from_policy(surface, self.play_skin_canvas_render_policy());
        Some((
            viewport.rect.x + position.0 * viewport.rect.width,
            viewport.rect.y + position.1 * viewport.rect.height,
        ))
    }

    pub fn set_play_skin_user_selected_options(&mut self, enabled_options: Vec<i32>) -> bool {
        self.play_skin_context.set_user_selected_options(enabled_options)
    }

    pub fn play_skin_timer_animation_duration_ms(&self, timer: i32) -> i32 {
        self.play_skin_context.timer_animation_duration_ms(timer)
    }

    /// 決定スキンの document。
    pub fn decide_skin_document(&self) -> Option<&SkinDocument> {
        self.decide_skin_context.document()
    }

    /// リザルトスキンの document。
    pub fn result_skin_document(&self) -> Option<&SkinDocument> {
        self.result_skin_context.document()
    }

    pub fn resize_surface(&mut self, size: SurfaceSize) -> Result<()> {
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };
        if !size.is_drawable() {
            return Ok(());
        }

        gpu.resize(size)
    }

    pub fn render_scene(&mut self, scene: AppSceneSnapshot) -> Result<()> {
        self.render_scene_status(scene).map(|_| ())
    }

    pub fn render_scene_status(&mut self, scene: AppSceneSnapshot) -> Result<RenderSurfaceStatus> {
        let entering_scene = self.last_scene.as_ref().is_none_or(|previous| {
            std::mem::discriminant(previous) != std::mem::discriminant(&scene)
        });
        if entering_scene {
            match &scene {
                AppSceneSnapshot::Select(_) => self
                    .select_dynamic_timer_runtime
                    .reset_for_document(self.select_skin_context.document()),
                AppSceneSnapshot::Decide(_) => self
                    .decide_dynamic_timer_runtime
                    .reset_for_document(self.decide_skin_context.document()),
                AppSceneSnapshot::Play(_) => self
                    .play_dynamic_timer_runtime
                    .reset_for_document(self.play_skin_context.document()),
                AppSceneSnapshot::Result(_) => self
                    .result_dynamic_timer_runtime
                    .reset_for_document(self.result_skin_context.document()),
            }
        }
        let plan_start = Instant::now();
        let plan = match &scene {
            AppSceneSnapshot::Select(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.select_skin_context,
                &mut self.select_dynamic_timer_runtime,
            ),
            AppSceneSnapshot::Decide(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.decide_skin_context,
                &mut self.decide_dynamic_timer_runtime,
            ),
            AppSceneSnapshot::Play(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.play_skin_context,
                &mut self.play_dynamic_timer_runtime,
            ),
            AppSceneSnapshot::Result(_) => DrawPlan::from_scene_with_skin(
                &scene,
                &self.result_skin_context,
                &mut self.result_dynamic_timer_runtime,
            ),
        };
        let plan_us = plan_start.elapsed().as_micros();
        let commands = plan.commands.len();
        self.last_plan_canvas_policy = self.canvas_policy_for_scene(&scene);
        self.last_scene = Some(scene);
        self.last_plan = Some(plan);

        let status = self.render_last_plan()?;
        self.last_frame_timings = Some(RenderFrameTimings {
            plan_us,
            commands,
            ..self.last_frame_timings.unwrap_or_default()
        });
        Ok(status)
    }

    /// 次の描画フレームで重ねる egui の描画データを差し込む。
    ///
    /// `render_scene_status` / `render_last_plan` の呼び出しで消費される。
    pub fn set_egui_frame(&mut self, frame: EguiFrame) {
        self.pending_egui = Some(frame);
    }

    pub fn set_present_mode(&mut self, present_mode: WgpuPresentMode) -> Result<()> {
        if self.present_mode == present_mode {
            return Ok(());
        }
        self.present_mode = present_mode;
        if let Some(gpu) = &mut self.gpu {
            gpu.configure_presentation(present_mode, self.frame_latency_mode)?;
            tracing::info!(requested = ?present_mode, "present mode updated");
        }
        Ok(())
    }

    pub fn set_frame_latency_mode(&mut self, mode: WgpuFrameLatencyMode) -> Result<()> {
        if self.frame_latency_mode == mode {
            return Ok(());
        }
        self.frame_latency_mode = mode;
        if let Some(gpu) = &mut self.gpu {
            gpu.configure_presentation(self.present_mode, mode)?;
        }
        tracing::info!(?mode, "frame latency mode updated");
        Ok(())
    }

    pub fn set_internal_resolution_mode(&mut self, mode: InternalResolutionMode) {
        if self.internal_resolution_mode == mode {
            return;
        }
        self.internal_resolution_mode = mode;
        if let Some(gpu) = &mut self.gpu {
            gpu.clear_internal_scene_target();
        }
        tracing::info!(?mode, "internal resolution mode updated");
    }

    pub fn surface_presentation_status(&self) -> Option<SurfacePresentationStatus> {
        let gpu = self.gpu.as_ref()?;
        Some(SurfacePresentationStatus {
            requested_mode: self.present_mode,
            effective_mode: wgpu_present_mode_label(gpu.config.present_mode),
            maximum_frame_latency: gpu.config.desired_maximum_frame_latency,
        })
    }

    pub fn set_backend(&mut self, backend: WgpuBackend) {
        self.backend = backend;
    }

    /// 未指定テキストの CJK 字形で最優先する地域 coverage を変更する。
    ///
    /// 優先 face に無い文字は、他の全 CJK coverage と一般 sans-serif へ
    /// 文字単位で fallback する。スキンが明示指定したフォントには影響しない。
    pub fn set_default_font_coverage(&mut self, coverage: bmz_font::FontCoverage) {
        if self.default_font_coverage == coverage {
            return;
        }
        self.default_font_coverage = coverage;
        if let Some(gpu) = &mut self.gpu {
            gpu.set_default_font_coverage(coverage);
        }
    }

    /// 未指定テキストの fallback として使う、アプリ同梱フォントの検索ディレクトリを設定する。
    ///
    /// ここで指定した resource font は OS フォントより先に解決される。明示指定された
    /// スキンフォントの選択には影響しない。
    pub fn set_default_font_search_paths(&mut self, paths: Vec<PathBuf>) {
        if self.default_font_search_paths == paths {
            return;
        }
        self.default_font_search_paths = paths;
        if let Some(gpu) = &mut self.gpu {
            gpu.set_default_font_search_paths(self.default_font_search_paths.clone());
        }
    }

    pub fn render_last_plan(&mut self) -> Result<RenderSurfaceStatus> {
        let egui = self.pending_egui.take();
        let screenshot = self.pending_screenshot.take();
        let Some(gpu) = &mut self.gpu else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };
        let Some(plan) = &self.last_plan else {
            return Ok(RenderSurfaceStatus::SkippedNoSurface);
        };

        let (status, gpu_timings) = gpu.render_plan(
            plan,
            self.last_plan_canvas_policy,
            self.internal_resolution_mode,
            &self.fonts,
            &self.bitmap_fonts,
            egui.as_ref(),
            screenshot.as_ref(),
        )?;
        self.last_frame_timings = Some(RenderFrameTimings {
            draw_us: gpu_timings.draw_us,
            text_us: gpu_timings.text_us,
            geometry_us: gpu_timings.geometry_us,
            upload_us: gpu_timings.upload_us,
            submit_us: gpu_timings.submit_us,
            surface_us: gpu_timings.surface_us,
            bind_us: gpu_timings.bind_us,
            encode_us: gpu_timings.encode_us,
            queue_us: gpu_timings.queue_us,
            present_us: gpu_timings.present_us,
            steps: gpu_timings.steps,
            rect_steps: gpu_timings.rect_steps,
            image_steps: gpu_timings.image_steps,
            text_steps: gpu_timings.text_steps,
            rect_instances: gpu_timings.rect_instances,
            image_instances: gpu_timings.image_instances,
            text_instances: gpu_timings.text_instances,
            ..self.last_frame_timings.unwrap_or_default()
        });
        Ok(status)
    }

    pub fn request_screenshot(&mut self, path: impl Into<PathBuf>) {
        self.pending_screenshot =
            Some(ScreenshotRequest { path: path.into(), copy_to_clipboard: false });
    }

    pub fn request_screenshot_with_clipboard(&mut self, path: impl Into<PathBuf>) {
        self.pending_screenshot =
            Some(ScreenshotRequest { path: path.into(), copy_to_clipboard: true });
    }

    /// 次の描画フレームでスクリーンショットを撮る予定があるか。
    ///
    /// 撮影フレームではトースト等の一時 UI を隠す判定に使う。
    pub fn has_pending_screenshot(&self) -> bool {
        self.pending_screenshot.is_some()
    }

    pub fn flush_pending_screenshots(&mut self) -> Result<()> {
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };
        gpu.flush_pending_screenshots()
    }

    pub fn last_scene(&self) -> Option<&AppSceneSnapshot> {
        self.last_scene.as_ref()
    }

    pub fn last_plan(&self) -> Option<&DrawPlan> {
        self.last_plan.as_ref()
    }

    pub fn last_frame_timings(&self) -> Option<RenderFrameTimings> {
        self.last_frame_timings
    }

    fn select_skin_canvas_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let Some(surface) = self.gpu.as_ref().map(WgpuRenderer::surface_size) else {
            return Some((x, y));
        };
        let viewport =
            CanvasViewport::from_policy(surface, self.select_skin_canvas_render_policy());
        viewport.surface_to_canvas_point(x, y)
    }

    fn result_skin_canvas_point(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let Some(surface) = self.gpu.as_ref().map(WgpuRenderer::surface_size) else {
            return Some((x, y));
        };
        let viewport =
            CanvasViewport::from_policy(surface, self.result_skin_canvas_render_policy());
        viewport.surface_to_canvas_point(x, y)
    }

    fn canvas_policy_for_scene(&self, scene: &AppSceneSnapshot) -> CanvasRenderPolicy {
        match scene {
            AppSceneSnapshot::Select(_) => self.select_skin_canvas_render_policy(),
            AppSceneSnapshot::Decide(_) => self.decide_skin_canvas_render_policy(),
            AppSceneSnapshot::Play(_) => self.play_skin_canvas_render_policy(),
            AppSceneSnapshot::Result(_) => self.result_skin_canvas_render_policy(),
        }
    }

    fn select_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.select_skin_context
            .document()
            .filter(|document| document.skin_type == 5)
            .map(CanvasRenderPolicy::skin_document)
            .unwrap_or_default()
    }

    fn decide_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.decide_skin_context
            .document()
            .filter(|document| document.skin_type == 6)
            .map(CanvasRenderPolicy::skin_document)
            .unwrap_or_default()
    }

    fn play_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.play_skin_context.document().map(CanvasRenderPolicy::skin_document).unwrap_or_default()
    }

    fn result_skin_canvas_render_policy(&self) -> CanvasRenderPolicy {
        self.result_skin_context
            .document()
            .filter(|document| matches!(document.skin_type, 7 | 15))
            .map(CanvasRenderPolicy::skin_document)
            .unwrap_or_default()
    }
}
use super::*;
