//! Large skin animation frames kept compressed on the CPU (`deferredSources`).
//!
//! Visible frames are uploaded before drawing. Recently used frames stay on the
//! GPU within a byte budget, so held or looping frames are not uploaded again,
//! and the next frames of each animation are decoded on a worker thread so the
//! window thread usually only uploads. A frame that cannot be decoded or
//! uploaded is left out of the plan; the rest of the frame is still drawn.
use super::*;
use crate::assets::deferred::DeferredImage;
use std::collections::HashSet;

/// GPU bytes of deferred frames kept resident, the visible ones included.
const RESIDENT_BUDGET_BYTES: u64 = 128 * 1024 * 1024;
/// Upcoming frames decoded ahead for each visible animation frame.
const PREFETCH_FRAMES: usize = 4;
/// Decoded RGBA waiting for upload, or being decoded.
const DECODED_BUDGET_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Default)]
pub(super) struct DeferredState {
    /// GPU-resident frames and the draw counter at their last use.
    resident: HashMap<TextureId, u64>,
    tick: u64,
    /// Frames that could not be decoded or uploaded; skipped until replaced.
    failed: HashSet<TextureId>,
    /// Next distinct frames of the animation each frame belongs to.
    successors: Option<HashMap<TextureId, Vec<TextureId>>>,
    decoder: Option<DeferredDecoder>,
    decoder_unavailable: bool,
    reported_budget: bool,
}

impl DeferredState {
    /// The GPU textures are gone (new device or surface); compressed frames stay.
    pub(super) fn forget_gpu(&mut self) {
        self.resident.clear();
        self.failed.clear();
    }

    fn forget(&mut self, id: TextureId) {
        self.resident.remove(&id);
        self.failed.remove(&id);
        self.successors = None;
        if let Some(decoder) = &mut self.decoder {
            decoder.forget(id);
        }
    }

    pub(super) fn forget_sequences(&mut self) {
        self.successors = None;
        self.reported_budget = false;
    }

    #[cfg(test)]
    pub(super) fn resident_ids(&self) -> HashSet<TextureId> {
        self.resident.keys().copied().collect()
    }
}

fn frame_bytes(image: &DeferredImage) -> u64 {
    (u64::from(image.width) * u64::from(image.height)).saturating_mul(4)
}

/// Next distinct frames after each frame of one animation, wrapping if it loops.
pub(super) fn sequence_successors(
    frames: &[TextureId],
    wraps: bool,
    limit: usize,
    successors: &mut HashMap<TextureId, Vec<TextureId>>,
) {
    for (index, id) in frames.iter().enumerate() {
        let following = frames[index + 1..]
            .iter()
            .chain(if wraps { &frames[..index] } else { &[][..] })
            .copied();
        let mut next = Vec::with_capacity(limit);
        for frame in following {
            if next.len() == limit {
                break;
            }
            if frame != *id && !next.contains(&frame) {
                next.push(frame);
            }
        }
        successors.entry(*id).or_insert(next);
    }
}

/// Least recently used frames to drop so that resident frames fit `budget`.
/// Required frames are never chosen. Nothing is kept once no frame is visible.
pub(super) fn deferred_evictions(
    resident: &HashMap<TextureId, u64>,
    required: &HashSet<TextureId>,
    bytes: impl Fn(TextureId) -> u64,
    budget: u64,
) -> Vec<TextureId> {
    if required.is_empty() {
        return resident.keys().copied().collect();
    }
    let mut total = resident.keys().map(|id| bytes(*id)).fold(0, u64::saturating_add);
    let mut candidates: Vec<_> = resident
        .iter()
        .filter(|(id, _)| !required.contains(id))
        .map(|(id, tick)| (*tick, *id))
        .collect();
    candidates.sort_by_key(|(tick, id)| (*tick, id.0));
    let mut evicted = Vec::new();
    for (_, id) in candidates {
        if total <= budget {
            break;
        }
        total = total.saturating_sub(bytes(id));
        evicted.push(id);
    }
    evicted
}

struct DecodeResult {
    id: TextureId,
    image: Arc<DeferredImage>,
    decoded: Result<RgbaImageAsset>,
}

/// Decodes requested frames in order on one worker thread. Results are matched to
/// the image they were requested for, so a replaced frame's result is discarded.
pub(super) struct DeferredDecoder {
    requests: mpsc::Sender<(TextureId, Arc<DeferredImage>)>,
    results: mpsc::Receiver<DecodeResult>,
    in_flight: HashMap<TextureId, Arc<DeferredImage>>,
    ready: HashMap<TextureId, (Arc<DeferredImage>, Result<RgbaImageAsset>)>,
}

impl DeferredDecoder {
    pub(super) fn spawn() -> Option<Self> {
        let (requests, receiver) = mpsc::channel::<(TextureId, Arc<DeferredImage>)>();
        let (sender, results) = mpsc::channel();
        thread::Builder::new()
            .name("bmz-deferred-decode".into())
            .spawn(move || {
                // Ends when the renderer drops its request sender.
                for (id, image) in receiver {
                    let decoded = image.decode();
                    if sender.send(DecodeResult { id, image, decoded }).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self { requests, results, in_flight: HashMap::new(), ready: HashMap::new() })
    }

    fn accept(&mut self, result: DecodeResult) {
        if self.in_flight.get(&result.id).is_some_and(|image| Arc::ptr_eq(image, &result.image)) {
            self.in_flight.remove(&result.id);
            self.ready.insert(result.id, (result.image, result.decoded));
        }
    }

    fn drain(&mut self) {
        while let Ok(result) = self.results.try_recv() {
            self.accept(result);
        }
    }

    fn forget(&mut self, id: TextureId) {
        self.in_flight.remove(&id);
        self.ready.remove(&id);
    }

    fn pending_bytes(&self) -> u64 {
        self.in_flight
            .values()
            .map(|image| frame_bytes(image))
            .chain(self.ready.values().map(|(image, _)| frame_bytes(image)))
            .fold(0, u64::saturating_add)
    }

    fn is_pending(&self, id: TextureId, image: &Arc<DeferredImage>) -> bool {
        self.in_flight.get(&id).is_some_and(|pending| Arc::ptr_eq(pending, image))
            || self.ready.get(&id).is_some_and(|(ready, _)| Arc::ptr_eq(ready, image))
    }

    /// Returns `false` if the worker has stopped.
    pub(super) fn request(&mut self, id: TextureId, image: &Arc<DeferredImage>) -> bool {
        if self.is_pending(id, image) {
            return true;
        }
        if self.requests.send((id, image.clone())).is_err() {
            return false;
        }
        self.in_flight.insert(id, image.clone());
        true
    }

    /// The decoded frame, waiting for it if it is being decoded. `None` when it
    /// was not requested or the worker has stopped; the caller decodes it itself.
    pub(super) fn take(
        &mut self,
        id: TextureId,
        image: &Arc<DeferredImage>,
    ) -> Option<Result<RgbaImageAsset>> {
        self.drain();
        while !self.ready.get(&id).is_some_and(|(ready, _)| Arc::ptr_eq(ready, image)) {
            if !self.in_flight.get(&id).is_some_and(|pending| Arc::ptr_eq(pending, image)) {
                return None;
            }
            let result = self.results.recv().ok()?;
            self.accept(result);
        }
        self.ready.remove(&id).map(|(_, decoded)| decoded)
    }

    fn retain_ready(&mut self, keep: impl Fn(&TextureId) -> bool) {
        self.ready.retain(|id, _| keep(id));
    }
}

fn collect_textures(commands: &[DrawCommand], ids: &mut Vec<TextureId>) {
    for command in commands {
        match command {
            DrawCommand::Image { texture, .. } | DrawCommand::RotatedImage { texture, .. } => {
                if !ids.contains(texture) {
                    ids.push(*texture);
                }
            }
            DrawCommand::Ambient { layers, .. } => collect_textures(layers, ids),
            _ => {}
        }
    }
}

/// An unavailable texture would otherwise draw the white fallback texture.
fn drop_textures(commands: &mut Vec<DrawCommand>, ids: &HashSet<TextureId>) {
    commands.retain_mut(|command| match command {
        DrawCommand::Image { texture, .. } | DrawCommand::RotatedImage { texture, .. } => {
            !ids.contains(texture)
        }
        DrawCommand::Ambient { layers, .. } => {
            drop_textures(layers, ids);
            true
        }
        _ => true,
    });
}

impl Renderer {
    pub(super) fn forget_deferred_image(&mut self, id: TextureId) {
        self.deferred_images.remove(&id);
        self.deferred.forget(id);
    }

    pub fn upsert_deferred_image(&mut self, id: TextureId, image: Arc<DeferredImage>) {
        self.remove_image_texture(id);
        self.deferred_images.insert(id, image);
        self.deferred.forget_sequences();
    }

    fn deferred_successors(&self) -> HashMap<TextureId, Vec<TextureId>> {
        let mut successors = HashMap::new();
        for context in [
            &self.play_skin_context,
            &self.select_skin_context,
            &self.decide_skin_context,
            &self.result_skin_context,
        ] {
            for (textures, wraps) in context.qwilight_animation_texture_sequences() {
                let mut frames: Vec<_> = textures
                    .into_iter()
                    .map(|texture| TextureId(texture.0))
                    .filter(|id| self.deferred_images.contains_key(id))
                    .collect();
                // Sparse frames repeat the previous image.
                frames.dedup();
                sequence_successors(&frames, wraps, PREFETCH_FRAMES, &mut successors);
            }
        }
        successors
    }

    /// Uploads the deferred frames the current plan draws. Never fails the frame:
    /// a frame over the visible budget or failing to decode/upload is skipped.
    pub(super) fn prepare_deferred_images(&mut self) {
        if self.deferred_images.is_empty() || self.gpu.is_none() {
            return;
        }
        let mut drawn = Vec::new();
        if let Some(plan) = &self.last_plan {
            collect_textures(&plan.commands, &mut drawn);
        }
        drawn.retain(|id| self.deferred_images.contains_key(id));
        let mut unavailable: HashSet<_> =
            drawn.iter().copied().filter(|id| self.deferred.failed.contains(id)).collect();
        let mut required = Vec::with_capacity(drawn.len());
        let mut visible_bytes = 0u64;
        for id in drawn.iter().copied().filter(|id| !self.deferred.failed.contains(id)) {
            let bytes = frame_bytes(&self.deferred_images[&id]);
            if visible_bytes.saturating_add(bytes) > RESIDENT_BUDGET_BYTES {
                unavailable.insert(id);
            } else {
                visible_bytes += bytes;
                required.push(id);
            }
        }
        if unavailable.iter().any(|id| !self.deferred.failed.contains(id))
            && !self.deferred.reported_budget
        {
            self.deferred.reported_budget = true;
            tracing::warn!(
                budget_mib = RESIDENT_BUDGET_BYTES / 1024 / 1024,
                "visible deferred skin frames exceed the GPU budget; skipping the rest"
            );
        }

        if self.deferred.successors.is_none() {
            self.deferred.successors = Some(self.deferred_successors());
        }
        if self.deferred.decoder.is_none()
            && !self.deferred.decoder_unavailable
            && !required.is_empty()
        {
            self.deferred.decoder = DeferredDecoder::spawn();
            if self.deferred.decoder.is_none() {
                self.deferred.decoder_unavailable = true;
                tracing::warn!("deferred skin frames are decoded on the window thread");
            }
        }

        self.deferred.tick += 1;
        let tick = self.deferred.tick;
        let gpu = self.gpu.as_mut().expect("checked above");
        for &id in &required {
            if let Some(last_used) = self.deferred.resident.get_mut(&id) {
                *last_used = tick;
                continue;
            }
            let image = self.deferred_images[&id].clone();
            let decoded = self
                .deferred
                .decoder
                .as_mut()
                .and_then(|decoder| decoder.take(id, &image))
                .unwrap_or_else(|| image.decode());
            match decoded.and_then(|frame| {
                gpu.upsert_rgba_texture(id, frame.width, frame.height, &frame.pixels)
            }) {
                Ok(()) => {
                    self.deferred.resident.insert(id, tick);
                }
                Err(error) => {
                    tracing::warn!(
                        texture_id = id.0,
                        error = %format!("{error:#}"),
                        "skipping deferred skin frame"
                    );
                    self.deferred.failed.insert(id);
                    unavailable.insert(id);
                }
            }
        }

        let required: HashSet<_> =
            required.into_iter().filter(|id| !self.deferred.failed.contains(id)).collect();
        let images = &self.deferred_images;
        let bytes = |id: TextureId| images.get(&id).map_or(0, |image| frame_bytes(image));
        for id in
            deferred_evictions(&self.deferred.resident, &required, bytes, RESIDENT_BUDGET_BYTES)
        {
            self.deferred.resident.remove(&id);
            gpu.image_textures.remove(&id);
            gpu.image_bind_group_cache.retain(|(texture, _), _| *texture != id);
        }

        // Decode the next frames of each visible animation off the window thread.
        let successors = self.deferred.successors.as_ref().expect("built above");
        let mut upcoming = Vec::new();
        for id in &required {
            for next in successors.get(id).into_iter().flatten() {
                if !self.deferred.resident.contains_key(next)
                    && !self.deferred.failed.contains(next)
                    && !required.contains(next)
                    && !upcoming.contains(next)
                {
                    upcoming.push(*next);
                }
            }
        }
        if let Some(decoder) = &mut self.deferred.decoder {
            decoder.drain();
            decoder.retain_ready(|id| upcoming.contains(id));
            let mut stopped = false;
            for id in &upcoming {
                let Some(image) = images.get(id) else { continue };
                if !decoder.is_pending(*id, image)
                    && decoder.pending_bytes().saturating_add(frame_bytes(image))
                        > DECODED_BUDGET_BYTES
                {
                    break;
                }
                if !decoder.request(*id, image) {
                    stopped = true;
                    break;
                }
            }
            if stopped {
                tracing::warn!("deferred skin frame decoder stopped");
                self.deferred.decoder = None;
                self.deferred.decoder_unavailable = true;
            }
        }

        if !unavailable.is_empty()
            && let Some(plan) = &mut self.last_plan
        {
            drop_textures(&mut plan.commands, &unavailable);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[u32]) -> Vec<TextureId> {
        values.iter().copied().map(TextureId).collect()
    }

    #[test]
    fn successors_follow_playback_and_wrap_only_for_looping_animations() {
        let mut once = HashMap::new();
        sequence_successors(&ids(&[1, 2, 3, 2, 4]), false, 2, &mut once);
        assert_eq!(once[&TextureId(1)], ids(&[2, 3]));
        assert_eq!(once[&TextureId(3)], ids(&[2, 4]));
        assert_eq!(once[&TextureId(4)], ids(&[]));
        let mut looping = HashMap::new();
        sequence_successors(&ids(&[1, 2, 3]), true, 4, &mut looping);
        assert_eq!(looping[&TextureId(3)], ids(&[1, 2]));
    }

    #[test]
    fn eviction_keeps_visible_and_recent_frames_within_budget() {
        let resident: HashMap<_, _> =
            [(TextureId(1), 1), (TextureId(2), 3), (TextureId(3), 2), (TextureId(4), 4)].into();
        let required: HashSet<_> = [TextureId(4)].into();
        // Four 10-byte frames in a 25-byte budget: drop the two least recently used.
        let mut evicted = deferred_evictions(&resident, &required, |_| 10, 25);
        evicted.sort_by_key(|id| id.0);
        assert_eq!(evicted, ids(&[1, 3]));
        assert!(deferred_evictions(&resident, &required, |_| 10, 40).is_empty());
        // Visible frames stay even beyond the budget.
        assert_eq!(
            deferred_evictions(&resident, &resident.keys().copied().collect(), |_| 10, 0),
            ids(&[])
        );
        let mut all = deferred_evictions(&resident, &HashSet::new(), |_| 10, 40);
        all.sort_by_key(|id| id.0);
        assert_eq!(all, ids(&[1, 2, 3, 4]));
    }

    #[test]
    fn decoder_returns_requested_frames_and_drops_replaced_results() {
        let encode = |color: u8| {
            Arc::new(
                DeferredImage::encode(&RgbaImageAsset {
                    width: 1,
                    height: 1,
                    pixels: vec![color, 0, 0, 255],
                })
                .unwrap(),
            )
        };
        let mut decoder = DeferredDecoder::spawn().unwrap();
        let first = encode(1);
        assert!(decoder.request(TextureId(7), &first));
        assert_eq!(decoder.take(TextureId(7), &first).unwrap().unwrap().pixels[0], 1);
        assert!(decoder.take(TextureId(7), &first).is_none(), "taken once");
        // The ID now names another image: the earlier request must not be used.
        assert!(decoder.request(TextureId(8), &first));
        let replacement = encode(2);
        assert!(decoder.take(TextureId(8), &replacement).is_none());
        assert!(decoder.request(TextureId(8), &replacement));
        assert_eq!(decoder.take(TextureId(8), &replacement).unwrap().unwrap().pixels[0], 2);
    }

    #[test]
    fn unavailable_frames_are_removed_from_nested_layers() {
        let image = |id| DrawCommand::Image {
            rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            texture: TextureId(id),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            source_size: None,
            linear_filter: false,
        };
        let mut commands = vec![
            image(1),
            DrawCommand::Ambient {
                rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                blur: 0.0,
                fade_edges: false,
                layers: vec![image(2), image(3)],
            },
        ];
        drop_textures(&mut commands, &[TextureId(1), TextureId(2)].into());
        let mut remaining = Vec::new();
        collect_textures(&commands, &mut remaining);
        assert_eq!(remaining, ids(&[3]));
    }

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
            assert!(renderer.deferred.resident_ids().contains(&TextureId(id)));
        }
        // Both small frames fit the budget, so returning to 900 reused its texture.
        assert_eq!(renderer.deferred.resident_ids(), [TextureId(900), TextureId(901)].into());
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
        // A frame that cannot be decoded is skipped; the frame is still drawn
        // without the white fallback texture in its place.
        let mut broken =
            crate::assets::deferred::DeferredImage::encode(&crate::assets::RgbaImageAsset {
                width: 1,
                height: 1,
                pixels: vec![0, 0, 0, 255],
            })
            .unwrap();
        broken.width = 2;
        renderer.upsert_deferred_image(TextureId(902), std::sync::Arc::new(broken));
        renderer.last_plan = Some(DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Image {
                rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                texture: TextureId(902),
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                source_size: None,
                linear_filter: false,
            }],
        });
        renderer.render_last_plan().unwrap();
        assert_eq!(&renderer.read_offscreen_rgba().unwrap()[..4], &[0, 0, 0, 255]);
        renderer.render_last_plan().unwrap();
        assert!(renderer.last_plan.as_ref().unwrap().commands.is_empty());
        // A subsequent ordinary skin can reuse the ID. Its texture must not be
        // overwritten by the old deferred frame, or evicted with that frame.
        renderer.upsert_rgba_texture_ref(TextureId(900), 1, 1, &[0, 0, 255, 255]).unwrap();
        assert!(!renderer.deferred_images.contains_key(&TextureId(900)));
        renderer.last_plan = Some(DrawPlan {
            clear: Color::rgb(0.0, 0.0, 0.0),
            commands: vec![DrawCommand::Image {
                rect: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                uv: crate::plan::UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                texture: TextureId(900),
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                source_size: None,
                linear_filter: false,
            }],
        });
        renderer.render_last_plan().unwrap();
        assert_eq!(&renderer.read_offscreen_rgba().unwrap()[..4], &[0, 0, 255, 255]);
        // No deferred frame is drawn any more: their GPU textures are released.
        renderer.last_plan = Some(DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands: vec![] });
        renderer.render_last_plan().unwrap();
        assert!(renderer.deferred.resident_ids().is_empty());
        renderer.remove_image_texture(TextureId(901));
        assert!(!renderer.deferred_images.contains_key(&TextureId(901)));
    }
}
