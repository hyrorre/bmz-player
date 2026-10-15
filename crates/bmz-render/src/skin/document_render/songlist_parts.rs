use super::*;
use std::collections::HashSet;

/// Immutable marker metadata, shared across frames with the document cache.
#[derive(Debug)]
pub(in crate::skin) struct SongListBarParts {
    markers: Vec<SkinLr2BarPartDef>,
    orders: Option<HashSet<usize>>,
}

impl SongListBarParts {
    pub(in crate::skin) fn new(markers: Vec<SkinLr2BarPartDef>) -> Self {
        let orders = markers.iter().map(|part| part.order).collect();
        Self { markers, orders }
    }

    pub(super) fn bucket(
        &self,
        items: Vec<(SongListPartKey, SkinRenderItem)>,
    ) -> SongListPartBuckets {
        let Some(orders) = &self.orders else {
            // Older documents can mix index-based and order-based markers.
            return SongListPartBuckets::Legacy(items);
        };
        let mut buckets = HashMap::<usize, Vec<(usize, SkinRenderItem)>>::new();
        let mut unmarked = Vec::new();
        for (sequence, (key, item)) in items.into_iter().enumerate() {
            if let Some(order) = key.order.filter(|order| orders.contains(order)) {
                buckets.entry(order).or_default().push((sequence, item));
            } else {
                unmarked.push((sequence, item));
            }
        }
        SongListPartBuckets::Ordered { buckets, unmarked }
    }
}

pub(super) enum SongListPartBuckets {
    Ordered {
        buckets: HashMap<usize, Vec<(usize, SkinRenderItem)>>,
        unmarked: Vec<(usize, SkinRenderItem)>,
    },
    Legacy(Vec<(SongListPartKey, SkinRenderItem)>),
}

impl SongListPartBuckets {
    pub(super) fn take(
        &mut self,
        part: &SkinLr2BarPartDef,
        first: bool,
        markers: &SongListBarParts,
    ) -> Vec<SkinRenderItem> {
        match self {
            Self::Ordered { buckets, unmarked } => {
                let mut items =
                    part.order.and_then(|order| buckets.remove(&order)).unwrap_or_default();
                if first && !unmarked.is_empty() {
                    // Unmarked parts share the first visible marker. Preserve
                    // their original position among that marker's own parts.
                    items.append(unmarked);
                    items.sort_unstable_by_key(|(sequence, _)| *sequence);
                }
                items.into_iter().map(|(_, item)| item).collect()
            }
            Self::Legacy(pending) => pending
                .extract_if(.., |(key, _)| {
                    key.matches(part)
                        || (first && !markers.markers.iter().any(|marker| key.matches(marker)))
                })
                .map(|(_, item)| item)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct CountingRuntime(Mutex<usize>);

    impl SkinLuaDrawRuntime for CountingRuntime {
        fn evaluate_draw(
            &self,
            _: usize,
            _: &SkinDrawState,
            _: &[i32],
            _: &BTreeMap<i32, String>,
        ) -> bool {
            *self.0.lock().unwrap() += 1;
            true
        }
    }

    #[test]
    fn row_body_tag_keeps_lua_condition_evaluation() {
        let document: SkinDocument = serde_json::from_value(serde_json::json!({
            "type":5,"lr2":true,"w":100,"h":100,
            "image":[{"id":"body","src":"src","w":10,"h":10}],
            "imageset":[{"id":"bars","images":["body"]}],
            "songlist":{"id":"list","center":0,"liston":[{
                "id":"bars","op_expr":["bmz:lua_draw_callback:0"],
                "lr2BarPart":{"part":"liston","indices":[0],"order":7},
                "dst":[{"x":0,"y":0,"w":10,"h":10}]
            }]}
        }))
        .unwrap();
        let sources = HashMap::from([(
            "src".into(),
            SkinDocumentTexture {
                source_id: "src".into(),
                texture: SkinTextureId(1),
                source_size: SkinImageSize { width: 10.0, height: 10.0 },
            },
        )]);
        let runtime = Arc::new(CountingRuntime::default());
        let state = SkinDrawState {
            lua_runtime: Some(SkinLuaRuntimeContext {
                runtime: runtime.clone(),
                enabled_options: Arc::from([]),
                text_values: Arc::new(BTreeMap::new()),
            }),
            ..Default::default()
        };
        let snapshot =
            SelectSnapshot { rows: vec![SelectRowSnapshot::default()], ..Default::default() };
        let items = document.select_songlist_part_items(
            &sources,
            &snapshot,
            &document.image_map().into(),
            &[],
            &state,
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0.order, Some(7));
        // The original path selects the row once for its body and once for its
        // tag. Skipping the second call would change a stateful Lua callback.
        assert_eq!(*runtime.0.lock().unwrap(), 2);
    }

    fn item(index: usize) -> SkinRenderItem {
        SkinRenderItem::Rect {
            rect: Rect { x: index as f32, y: 0.0, width: 1.0, height: 1.0 },
            color: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
        }
    }

    fn marker(order: Option<usize>, indices: Vec<usize>) -> SkinLr2BarPartDef {
        SkinLr2BarPartDef { part: "listoff".into(), indices, order }
    }

    #[test]
    fn buckets_preserve_row_order_unmarked_parts_and_first_visible_marker() {
        let markers = SongListBarParts::new(vec![
            marker(Some(10), vec![]),
            marker(Some(20), vec![]),
            marker(Some(10), vec![]),
        ]);
        let mut parts = markers.bucket(
            [Some(10), None, Some(20), Some(10), Some(99)]
                .into_iter()
                .enumerate()
                .map(|(index, order)| {
                    (SongListPartKey { part: "listoff", index, order }, item(index))
                })
                .collect(),
        );
        // The first declared marker can be clipped/hidden. Its items must stay
        // pending for a later matching marker; only unmarked parts move here.
        assert_eq!(
            parts.take(&markers.markers[1], true, &markers),
            vec![item(1), item(2), item(4)]
        );
        assert_eq!(parts.take(&markers.markers[2], false, &markers), vec![item(0), item(3)]);
        assert!(parts.take(&markers.markers[0], false, &markers).is_empty());
    }

    #[test]
    fn mixed_legacy_markers_keep_index_matching_and_original_sequence() {
        let markers =
            SongListBarParts::new(vec![marker(None, vec![0, 2]), marker(Some(20), vec![])]);
        let mut parts = markers.bucket(
            [Some(10), Some(20), None, Some(99)]
                .into_iter()
                .enumerate()
                .map(|(index, order)| {
                    (SongListPartKey { part: "listoff", index, order }, item(index))
                })
                .collect(),
        );
        assert_eq!(
            parts.take(&markers.markers[0], true, &markers),
            vec![item(0), item(2), item(3)]
        );
        assert_eq!(parts.take(&markers.markers[1], false, &markers), vec![item(1)]);
    }
}
