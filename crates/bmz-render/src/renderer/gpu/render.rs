use super::*;

impl WgpuRenderer {
    pub(in crate::renderer) fn render_plan(
        &mut self,
        plan: &DrawPlan,
        canvas_policy: CanvasRenderPolicy,
        internal_resolution_mode: InternalResolutionMode,
        fonts: &HashMap<String, FontArc>,
        bitmap_fonts: &HashMap<String, BitmapFont>,
        egui: Option<&EguiFrame>,
        screenshot_request: Option<&ScreenshotRequest>,
    ) -> Result<(RenderSurfaceStatus, GpuRenderTimings)> {
        let draw_start = Instant::now();
        let mut timings = GpuRenderTimings::default();
        self.poll_screenshot_work();
        // egui のテクスチャ更新は、描画をスキップするフレームでも必ず適用する。
        // TexturesDelta は累積ストリームのため、取りこぼすと後続フレームの
        // 部分更新が未確保テクスチャを参照して panic する。
        if let Some(frame) = egui {
            self.egui.update_textures(&self.device, &self.queue, frame);
        }

        let surface_size = SurfaceSize { width: self.config.width, height: self.config.height };
        if !surface_size.is_drawable() {
            timings.draw_us = draw_start.elapsed().as_micros();
            return Ok((RenderSurfaceStatus::SkippedZeroSize, timings));
        }
        let output_viewport = CanvasViewport::from_policy(surface_size, canvas_policy);
        let internal_render_size =
            canvas_policy.internal_render_size(surface_size, internal_resolution_mode);
        let render_size = internal_render_size.unwrap_or(surface_size);
        let canvas_viewport = CanvasViewport::from_policy(render_size, canvas_policy);

        let text_start = Instant::now();
        let mut text_frame =
            self.build_text_frame(plan, fonts, bitmap_fonts, canvas_viewport.content_size());
        if !canvas_viewport.is_identity() {
            canvas_viewport.transform_text_instances(&mut text_frame.instances);
            canvas_viewport.transform_text_caret_rects(&mut text_frame.command_caret_rects);
        }
        timings.text_us = text_start.elapsed().as_micros();
        let geometry_start = Instant::now();
        let mut geometry = std::mem::take(&mut self.geometry_scratch);
        encode_plan_geometry_into(
            plan,
            &text_frame,
            render_size,
            canvas_viewport,
            &mut |rects, cache| self.offscreen_rect_batch_texture(rects, cache, render_size),
            &mut geometry,
        );
        let geometry_stats = geometry.stats();
        timings.steps = geometry_stats.steps;
        timings.rect_steps = geometry_stats.rect_steps;
        timings.image_steps = geometry_stats.image_steps;
        timings.text_steps = geometry_stats.text_steps;
        timings.rect_instances = geometry_stats.rect_instances;
        timings.image_instances = geometry_stats.image_instances;
        timings.text_instances = geometry_stats.text_instances;
        timings.geometry_us = geometry_start.elapsed().as_micros();
        let upload_start = Instant::now();
        self.ensure_rect_buffer(geometry.rects.len());
        if let Some(buffer) = &self.rect_buffer
            && !geometry.rects.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &geometry.rects);
        }
        self.ensure_image_buffer(geometry.images.len());
        if let Some(buffer) = &self.image_buffer
            && !geometry.images.is_empty()
        {
            self.queue.write_buffer(buffer, 0, &geometry.images);
        }
        self.upload_text_frame(&text_frame);
        timings.upload_us = upload_start.elapsed().as_micros();

        let submit_start = Instant::now();
        let surface_start = Instant::now();
        let output = if let Some(surface) = &self.surface {
            Some(match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(output)
                | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    self.configure_surface()?;
                    timings.surface_us = surface_start.elapsed().as_micros();
                    timings.submit_us = submit_start.elapsed().as_micros();
                    timings.draw_us = draw_start.elapsed().as_micros();
                    self.geometry_scratch = geometry;
                    return Ok((RenderSurfaceStatus::Reconfigured, timings));
                }
                wgpu::CurrentSurfaceTexture::Timeout => {
                    timings.surface_us = surface_start.elapsed().as_micros();
                    timings.submit_us = submit_start.elapsed().as_micros();
                    timings.draw_us = draw_start.elapsed().as_micros();
                    self.geometry_scratch = geometry;
                    return Ok((RenderSurfaceStatus::TimedOut, timings));
                }
                wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Validation => {
                    timings.surface_us = surface_start.elapsed().as_micros();
                    timings.submit_us = submit_start.elapsed().as_micros();
                    timings.draw_us = draw_start.elapsed().as_micros();
                    self.geometry_scratch = geometry;
                    return Ok((RenderSurfaceStatus::TimedOut, timings));
                }
            })
        } else {
            None
        };
        timings.surface_us = surface_start.elapsed().as_micros();
        let texture = output
            .as_ref()
            .map(|o| o.texture.clone())
            .or_else(|| self.export_target.clone())
            .expect("render target exists");
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // image ステップごとの bind group を、レンダーパスが encoder を借りる前に作る。
        // steps 内の image ステップと同じ順序で並ぶ。
        let bind_start = Instant::now();
        let mut image_bind_groups = std::mem::take(&mut self.image_bind_group_scratch);
        image_bind_groups.clear();
        image_bind_groups.reserve(geometry_stats.image_steps);
        for step in &geometry.steps {
            if let DrawStep::Image { texture, linear, .. } = step {
                image_bind_groups.push(self.image_bind_group(*texture, *linear));
            }
        }
        let text_bind_group = self.text_bind_group();
        timings.bind_us = bind_start.elapsed().as_micros();
        let internal_target = internal_render_size.map(|size| {
            self.ensure_internal_scene_target(size);
            self.update_upscale_instance(output_viewport.rect, surface_size);
            let target =
                self.internal_scene_target.as_ref().expect("internal scene target was created");
            (target.view.clone(), target.bind_group.clone())
        });
        let encode_start = Instant::now();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bmz-render clear encoder"),
        });
        let draw_resources = PlanGeometryDrawResources {
            rect_pipeline: &self.rect_pipeline,
            rect_buffer: self.rect_buffer.as_ref(),
            image_pipeline: &self.image_pipeline,
            image_add_pipeline: &self.image_add_pipeline,
            image_multiply_pipeline: &self.image_multiply_pipeline,
            image_premultiplied_pipeline: &self.image_premultiplied_pipeline,
            image_layer_pipeline: &self.image_layer_pipeline,
            image_bind_groups: &image_bind_groups,
            image_buffer: self.image_buffer.as_ref(),
            text_pipeline: &self.text_pipeline,
            text_bind_group: text_bind_group.as_ref(),
            text_buffer: self.text_buffer.as_ref(),
        };
        if let Some((internal_view, upscale_bind_group)) = &internal_target {
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bmz-render internal scene pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: internal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(plan.clear.to_wgpu()),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                draw_plan_geometry(&mut pass, &geometry, draw_resources);
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bmz-render internal upscale pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(plan.clear.to_wgpu()),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.upscale_pipeline);
                pass.set_bind_group(0, upscale_bind_group, &[]);
                pass.set_vertex_buffer(0, self.upscale_buffer.slice(..));
                pass.draw(0..6, 0..1);
            }
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bmz-render scene surface pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(plan.clear.to_wgpu()),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            draw_plan_geometry(&mut pass, &geometry, draw_resources);
        }

        // ゲーム / スキン描画の上に egui を重ねる。staging 用 CommandBuffer は
        // egui パスを含む encoder より前に submit する必要がある。
        let egui_staging = match egui {
            Some(frame) => self.egui.paint(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                frame,
                [self.config.width, self.config.height],
            ),
            None => Vec::new(),
        };

        let screenshot = screenshot_request.map(|request| {
            let capture = ScreenshotCapture::new(
                &self.device,
                self.config.width,
                self.config.height,
                self.config.format,
            );
            capture.copy_from_surface(&mut encoder, &texture);
            (request.clone(), capture)
        });
        let command_buffer = encoder.finish();
        timings.encode_us = encode_start.elapsed().as_micros();
        let queue_start = Instant::now();
        self.queue.submit(egui_staging.into_iter().chain(std::iter::once(command_buffer)));
        timings.queue_us = queue_start.elapsed().as_micros();
        if let Some((request, capture)) = screenshot {
            self.enqueue_screenshot_readback(request, capture);
        }
        if let Some(frame) = egui {
            self.egui.free_textures(frame);
        }
        self.image_bind_group_scratch = image_bind_groups;
        self.geometry_scratch = geometry;
        let present_start = Instant::now();
        if let Some(output) = output {
            output.present();
        }
        timings.present_us = present_start.elapsed().as_micros();
        timings.submit_us = submit_start.elapsed().as_micros();
        timings.draw_us = draw_start.elapsed().as_micros();
        Ok((RenderSurfaceStatus::Rendered, timings))
    }
}
