use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

// Three reusable slots, at most 24 MiB. Large images use Queue::write_texture.
const MAX_SLOTS: usize = 3;
const MAX_SLOT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct TextureUploads {
    slots: Vec<UploadSlot>,
    copies: Vec<UploadCopy>,
}

struct UploadCopy {
    slot: usize,
    texture: wgpu::Texture,
    width: u32,
    height: u32,
    padded_row_bytes: u32,
}

struct UploadSlot {
    buffer: wgpu::Buffer,
    capacity: u64,
    ready: Arc<AtomicBool>,
    pending: bool,
}

impl TextureUploads {
    pub(super) fn write(
        &mut self,
        device: &wgpu::Device,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> bool {
        let row_bytes = width as u64 * 4;
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64;
        let size = padded_row_bytes * height as u64;
        if !(64 * 1024..=MAX_SLOT_BYTES).contains(&size) {
            return false;
        }
        if !self.slots.is_empty() {
            // Nonblocking: a busy GPU falls back to queue writes, without waiting
            // for a slot or growing the pool beyond its fixed memory budget.
            if device.poll(wgpu::PollType::Poll).is_err() {
                return false;
            }
        }
        let index = self.slots.iter().position(|slot| slot.ready.load(Ordering::Acquire));
        let index = match index {
            Some(index) => index,
            None if self.slots.len() < MAX_SLOTS => self.slots.len(),
            None => return false,
        };
        if index == self.slots.len() || self.slots[index].capacity < size {
            let slot = UploadSlot {
                buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bmz-render reusable texture upload"),
                    size,
                    usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: true,
                }),
                capacity: size,
                ready: Arc::new(AtomicBool::new(true)),
                pending: false,
            };
            if index == self.slots.len() {
                self.slots.push(slot);
            } else {
                self.slots[index] = slot;
            }
        }
        let slot = &mut self.slots[index];
        slot.ready.store(false, Ordering::Release);
        {
            let mut mapped = slot.buffer.slice(..size).get_mapped_range_mut();
            if row_bytes == padded_row_bytes {
                mapped.copy_from_slice(rgba);
            } else {
                for (row, source) in rgba.chunks_exact(row_bytes as usize).enumerate() {
                    let start = row * padded_row_bytes as usize;
                    mapped.slice(start..start + row_bytes as usize).copy_from_slice(source);
                }
            }
        }
        slot.buffer.unmap();
        slot.pending = true;
        self.copies.push(UploadCopy {
            slot: index,
            texture: texture.clone(),
            width,
            height,
            padded_row_bytes: padded_row_bytes as u32,
        });
        true
    }

    /// Put texture copies before the draws in the same command buffer.
    pub(super) fn encode(&mut self, encoder: &mut wgpu::CommandEncoder) {
        for copy in self.copies.drain(..) {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.slots[copy.slot].buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(copy.padded_row_bytes),
                        rows_per_image: Some(copy.height),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &copy.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d { width: copy.width, height: copy.height, depth_or_array_layers: 1 },
            );
        }
    }

    pub(super) fn finish(&mut self, device: &wgpu::Device) -> Option<wgpu::CommandBuffer> {
        if self.copies.is_empty() {
            return None;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bmz-render texture upload overflow"),
        });
        self.encode(&mut encoder);
        Some(encoder.finish())
    }

    pub(super) fn submitted(&mut self) {
        for slot in &mut self.slots {
            if std::mem::take(&mut slot.pending) {
                let ready = Arc::clone(&slot.ready);
                slot.buffer.slice(..).map_async(wgpu::MapMode::Write, move |result| {
                    ready.store(result.is_ok(), Ordering::Release);
                });
            }
        }
    }
}
