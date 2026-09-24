use super::*;

/// One last result per destination; owned by the document/source-scoped render
/// cache. No history of scores or Lua values accumulates as a song progresses.
#[derive(Debug, Default)]
pub(in crate::skin) struct NumberRenderCache {
    entries: Vec<Option<NumberRenderEntry>>,
}

#[derive(Debug)]
struct NumberRenderEntry {
    value_index: usize,
    key: Option<NumberRenderKey>,
    items: Vec<SkinRenderItem>,
}

#[derive(Debug, PartialEq)]
struct NumberRenderKey {
    number: i64,
    geometry_color: [i32; 8],
    texture: SkinTextureId,
    source_size: SkinImageSize,
    animation_phase: i32,
    signed: SignedNumberRender,
}

impl NumberRenderCache {
    pub(in crate::skin) fn render(
        &mut self,
        document: &SkinDocument,
        destination_index: usize,
        value_id: &str,
        number: i64,
        frame: ResolvedSkinFrame,
        elapsed_ms: i32,
        sources: &HashMap<String, SkinDocumentTexture>,
        signed: SignedNumberRender,
    ) -> Vec<SkinRenderItem> {
        if self.entries.len() <= destination_index {
            self.entries.resize_with(destination_index + 1, || None);
        }
        let slot = &mut self.entries[destination_index];
        if slot.is_none() {
            // Preserve value_number_render_items' first-definition lookup even
            // when duplicate IDs supply the property from the last definition.
            let Some(value_index) = document.value.iter().position(|value| value.id == value_id)
            else {
                return Vec::new();
            };
            *slot = Some(NumberRenderEntry { value_index, key: None, items: Vec::new() });
        }
        let entry = slot.as_mut().unwrap();
        let value = &document.value[entry.value_index];
        let Some(source) = sources.get(&value.src) else {
            return Vec::new();
        };
        let key = NumberRenderKey {
            number,
            geometry_color: [
                frame.x, frame.y, frame.w, frame.h, frame.r, frame.g, frame.b, frame.a,
            ],
            texture: source.texture,
            source_size: source.source_size,
            animation_phase: if value.cycle > 0 { elapsed_ms.rem_euclid(value.cycle) } else { 0 },
            signed,
        };
        if entry.key.as_ref() == Some(&key) {
            return entry.items.clone();
        }
        let items = document.value_number_render_items(
            value_id,
            number,
            ResolvedSkinFrame::default(),
            frame,
            elapsed_ms,
            sources,
            false,
            None,
            signed,
        );
        entry.items.clone_from(&items);
        entry.key = Some(key);
        items
    }
}
