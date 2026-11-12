use super::*;

impl Renderer {
    pub(super) fn forget_deferred_image(&mut self, id: TextureId) {
        self.deferred_images.remove(&id);
        self.deferred_resident.remove(&id);
    }

    pub fn upsert_deferred_image(
        &mut self,
        id: TextureId,
        image: std::sync::Arc<crate::assets::deferred::DeferredImage>,
    ) {
        self.remove_image_texture(id);
        self.deferred_images.insert(id, image);
    }

    pub(super) fn prepare_deferred_images(&mut self) -> Result<()> {
        if self.deferred_images.is_empty() || self.gpu.is_none() {
            return Ok(());
        }
        fn collect(
            commands: &[crate::plan::DrawCommand],
            ids: &mut std::collections::HashSet<TextureId>,
        ) {
            for command in commands {
                match command {
                    crate::plan::DrawCommand::Image { texture, .. }
                    | crate::plan::DrawCommand::RotatedImage { texture, .. } => {
                        ids.insert(*texture);
                    }
                    crate::plan::DrawCommand::Ambient { layers, .. } => collect(layers, ids),
                    _ => {}
                }
            }
        }
        let mut required = std::collections::HashSet::new();
        if let Some(plan) = &self.last_plan {
            collect(&plan.commands, &mut required);
        }
        required.retain(|id| self.deferred_images.contains_key(id));
        let bytes: u64 = required
            .iter()
            .map(|id| {
                (u64::from(self.deferred_images[id].width)
                    * u64::from(self.deferred_images[id].height))
                .saturating_mul(4)
            })
            .fold(0, u64::saturating_add);
        anyhow::ensure!(bytes <= 128 * 1024 * 1024, "visible deferred skin frames exceed 128 MiB");
        // At most the frames used by this draw plan remain resident. Compressed
        // frames survive eviction, retries and seeking without reading the ZIP.
        let gpu = self.gpu.as_mut().unwrap();
        for id in self.deferred_resident.difference(&required) {
            gpu.image_textures.remove(id);
            gpu.image_bind_group_cache.retain(|(texture, _), _| texture != id);
        }
        self.deferred_resident.retain(|id| required.contains(id));
        for id in required {
            if self.deferred_resident.contains(&id) {
                continue;
            }
            let image = self.deferred_images[&id].decode()?;
            gpu.upsert_rgba_texture(id, image.width, image.height, &image.pixels)?;
            self.deferred_resident.insert(id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU"]
    fn qwilight_deferred_gpu_frames_evict_replay_and_composite() {
        let mut renderer = Renderer::default();
        renderer.attach_offscreen(SurfaceSize { width: 16, height: 16 }).unwrap();
        for (id, color) in [(900, [255, 0, 0, 255]), (901, [0, 255, 0, 255])] {
            let image =
                crate::assets::RgbaImageAsset { width: 1, height: 1, pixels: color.to_vec() };
            renderer.upsert_deferred_image(
                TextureId(id),
                std::sync::Arc::new(
                    crate::assets::deferred::DeferredImage::encode(&image).unwrap(),
                ),
            );
        }
        for id in [900, 901, 900] {
            renderer.last_plan = Some(DrawPlan {
                clear: Color::rgb(0.0, 0.0, 0.0),
                commands: vec![DrawCommand::Image {
                    rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    texture: TextureId(id),
                    tint: Color::rgb(1.0, 1.0, 1.0),
                    blend: BlendMode::Normal,
                    source_size: None,
                    linear_filter: false,
                }],
            });
            renderer.render_last_plan().unwrap();
            let pixels = renderer.read_offscreen_rgba().unwrap();
            assert_eq!(&pixels[..4], if id == 900 { &[255, 0, 0, 255] } else { &[0, 255, 0, 255] });
            assert_eq!(renderer.deferred_resident.len(), 1);
            assert!(renderer.deferred_resident.contains(&TextureId(id)));
            assert!(
                !renderer
                    .gpu
                    .as_ref()
                    .unwrap()
                    .image_textures
                    .contains_key(&TextureId(if id == 900 { 901 } else { 900 }))
            );
        }
        // All Porter-Duff pipelines must accept partial alpha and produce their
        // specified output alpha. Source/destination are both 50% opaque.
        for (mode, alpha) in [
            (0, 191),
            (1, 191),
            (2, 64),
            (3, 64),
            (4, 64),
            (5, 64),
            (6, 128),
            (7, 128),
            (8, 128),
            (9, 255),
            (10, 128),
            (11, 128),
            (12, 128),
        ] {
            let command = |id, blend| DrawCommand::Image {
                rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                texture: TextureId(id),
                tint: Color::rgba(1.0, 1.0, 1.0, 0.5),
                blend,
                source_size: None,
                linear_filter: false,
            };
            renderer.last_plan = Some(DrawPlan {
                clear: Color::rgba(0.0, 0.0, 0.0, 0.0),
                commands: vec![
                    command(900, BlendMode::Normal),
                    command(901, BlendMode::Composite(mode)),
                ],
            });
            renderer.render_last_plan().unwrap();
            let pixels = renderer.read_offscreen_rgba().unwrap();
            assert!(pixels[3].abs_diff(alpha) <= 1, "mode {mode}: {:?}", &pixels[..4]);
        }
        // A subsequent ordinary skin can reuse the ID. Its texture must not be
        // overwritten by the old deferred frame, or evicted with that frame.
        renderer.upsert_rgba_texture_ref(TextureId(900), 1, 1, &[0, 0, 255, 255]).unwrap();
        assert!(!renderer.deferred_images.contains_key(&TextureId(900)));
        let plan = renderer.last_plan.as_mut().unwrap();
        plan.commands.truncate(1);
        let DrawCommand::Image { tint, .. } = &mut plan.commands[0] else { unreachable!() };
        *tint = Color::rgb(1.0, 1.0, 1.0);
        renderer.render_last_plan().unwrap();
        assert_eq!(&renderer.read_offscreen_rgba().unwrap()[..4], &[0, 0, 255, 255]);
        renderer.last_plan = Some(DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands: vec![] });
        renderer.render_last_plan().unwrap();
        assert!(renderer.deferred_resident.is_empty());
        renderer.remove_image_texture(TextureId(900));
        assert!(!renderer.deferred_images.contains_key(&TextureId(900)));
    }
}
