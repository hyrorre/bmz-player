use egui_wgpu::{Renderer as EguiWgpuRenderer, RendererOptions, ScreenDescriptor};

/// bmz-player 側で構築した egui の 1 フレーム分の描画データ。
///
/// `egui::Context` の状態管理は bmz-player が持ち、bmz-render はこのコンテナ経由で
/// 受け取った描画プリミティブをペイントするだけにする。
pub struct EguiFrame {
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub textures_delta: egui::TexturesDelta,
    pub pixels_per_point: f32,
}

/// egui のプリミティブを wgpu サーフェスへペイントするグルー。
///
/// `egui_wgpu::Renderer` をラップし、ゲーム / スキン描画の上へ egui を重ねる。
pub struct EguiPainter {
    renderer: EguiWgpuRenderer,
}

impl EguiPainter {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let renderer = EguiWgpuRenderer::new(device, surface_format, RendererOptions::default());
        Self { renderer }
    }

    /// egui のテクスチャ確保 / 再アップロードを適用する。
    ///
    /// `TexturesDelta` は累積的なストリームで、フレームを取りこぼすと後続の
    /// 部分更新が未確保テクスチャを参照して panic する。そのため描画を
    /// スキップするフレームでも `paint` の有無に関わらず必ず呼ぶこと。
    pub fn update_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &EguiFrame,
    ) {
        for (id, delta) in &frame.textures_delta.set {
            self.renderer.update_texture(device, queue, *id, delta);
        }
    }

    /// 不要になった egui テクスチャを解放する。`paint` の後に呼ぶこと。
    pub fn free_textures(&mut self, frame: &EguiFrame) {
        for id in &frame.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }

    /// `encoder` に egui の描画コマンドを記録する。
    ///
    /// 事前に `update_textures` を呼んでおくこと。既存のゲーム描画パスの後・
    /// `queue.submit` の前に呼ぶ。返り値の `CommandBuffer` 群 (バッファ
    /// ステージング用) は `encoder` の finish より前に submit する必要がある。
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        frame: &EguiFrame,
        size_in_pixels: [u32; 2],
    ) -> Vec<wgpu::CommandBuffer> {
        let screen = ScreenDescriptor { size_in_pixels, pixels_per_point: frame.pixels_per_point };
        let staging =
            self.renderer.update_buffers(device, queue, encoder, &frame.primitives, &screen);

        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bmz-render egui pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // ゲーム描画をクリアせず、その上に重ねて描く。
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                // egui_wgpu の render は RenderPass<'static> を要求する。
                .forget_lifetime();
            self.renderer.render(&mut pass, &frame.primitives, &screen);
        }

        staging
    }
}
