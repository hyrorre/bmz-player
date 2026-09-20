use super::*;

impl WgpuRenderer {
    pub(in crate::renderer) fn upsert_rgba_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<()> {
        validate_rgba_texture_for_device(
            self.device.limits().max_texture_dimension_2d,
            width,
            height,
            rgba,
        )?;
        if let Some(texture) = self.image_textures.get(&id)
            && texture.width == width
            && texture.height == height
        {
            if self.texture_uploads.write(&self.device, &texture.texture, width, height, rgba) {
                return Ok(());
            }
            // Preserve write order when the same texture is updated more than
            // once before rendering and the bounded pool runs out of slots.
            if let Some(commands) = self.texture_uploads.finish(&self.device) {
                self.queue.submit([commands]);
                self.texture_uploads.submitted();
            }
            write_rgba_texture(&self.queue, &texture.texture, width, height, rgba);
            return Ok(());
        }
        let texture = create_rgba_texture(&self.device, &self.queue, id, width, height, rgba)?;
        self.image_textures.insert(id, texture);
        self.image_bind_group_cache.retain(|(texture_id, _), _| *texture_id != id);
        Ok(())
    }

    pub(in crate::renderer) fn image_bind_group(
        &mut self,
        texture_id: TextureId,
        linear: bool,
    ) -> wgpu::BindGroup {
        let resolved_texture_id =
            if self.image_textures.contains_key(&texture_id) { texture_id } else { TextureId(0) };
        if let Some(bind_group) =
            self.image_bind_group_cache.get(&(resolved_texture_id, linear)).cloned()
        {
            return bind_group;
        }
        let texture =
            self.image_textures.get(&resolved_texture_id).expect("fallback texture is registered");
        let _keep_texture_alive = &texture.texture;
        let sampler = if linear { &self.image_sampler_linear } else { &self.image_sampler };
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bmz-render image bind group"),
            layout: &self.image_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        self.image_bind_group_cache.insert((resolved_texture_id, linear), bind_group.clone());
        bind_group
    }
}
