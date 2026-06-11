//! 選曲スキン: 設定フォルダ内 Config 行向け destination ホワイトリスト。

use std::collections::HashMap;

use crate::scene::{SelectRowKind, SelectRowSnapshot, SelectSnapshot};
use crate::skin::{DestinationListEntry, SkinDestinationDef, SkinDocument, SkinDrawState};

const DETAIL_TEXT_REFS: [i32; 6] = [10, 11, 12, 14, 15, 16];
const BREADCRUMB_TEXT_REF: i32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SelectSettingsDestClass {
    /// 既存の `test_skin_op` に従う。
    Normal,
    /// Config 行では `op` に関係なく描画する。
    AllowConfig,
    /// Config 行では描画しない。
    DenyConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SelectDestEntryKey {
    id: String,
    ops: Vec<i32>,
}

impl SelectDestEntryKey {
    fn from(destination: &SkinDestinationDef) -> Self {
        Self { id: destination.id.clone(), ops: destination.op.clone() }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct SelectSettingsDestIndex {
    classes: HashMap<SelectDestEntryKey, SelectSettingsDestClass>,
}

pub fn build_select_settings_dest_index(document: &SkinDocument) -> SelectSettingsDestIndex {
    let mut classes = HashMap::new();
    for_each_destination(document, |destination| {
        let key = SelectDestEntryKey::from(destination);
        let class = classify_destination_element(document, &destination.id);
        classes.insert(key, class);
    });
    apply_config_duplicate_rules(document, &mut classes);
    SelectSettingsDestIndex { classes }
}

pub fn test_select_destination_visible(
    index: &SelectSettingsDestIndex,
    destination: &SkinDestinationDef,
    enabled_options: &[i32],
    state: &SkinDrawState,
    snapshot: &SelectSnapshot,
    selected_row: Option<&SelectRowSnapshot>,
    eval_draw: impl FnOnce(&str, &SkinDrawState) -> bool,
    test_ops: impl Fn(&[i32], &[i32], &SkinDrawState) -> bool,
) -> bool {
    if !eval_draw(&destination.draw, state) {
        return false;
    }
    let config_detail =
        snapshot.in_settings && selected_row.is_some_and(|row| row.kind == SelectRowKind::Config);
    if !config_detail {
        return test_ops(&destination.op, enabled_options, state);
    }
    match index.class_for(destination) {
        SelectSettingsDestClass::DenyConfig => false,
        SelectSettingsDestClass::AllowConfig => {
            config_allow_destination(destination, enabled_options, state, &test_ops)
        }
        SelectSettingsDestClass::Normal => test_ops(&destination.op, enabled_options, state),
    }
}

/// Config 行では選曲 op (1/2/3) は満たした扱いにし、スキン property op (192/193 等) だけ評価する。
fn config_allow_destination(
    destination: &SkinDestinationDef,
    enabled_options: &[i32],
    state: &SkinDrawState,
    test_ops: &impl Fn(&[i32], &[i32], &SkinDrawState) -> bool,
) -> bool {
    let filter_ops: Vec<i32> =
        destination.op.iter().copied().filter(|op| !(1..=3).contains(op)).collect();
    if filter_ops.is_empty() {
        return true;
    }
    test_ops(&filter_ops, enabled_options, state)
}

fn destination_id_is_detail_panel(destination_id: &str) -> bool {
    let id = destination_id.to_ascii_lowercase();
    matches!(
        id.as_str(),
        "title" | "subtitle" | "artist" | "dir" | "coursedir" | "fulltitle" | "fullartist"
    ) || id.contains("directory")
        || id.contains("default_info_title")
        || id.contains("default_info_subtitle")
        || id.contains("default_info_artist")
}

impl SelectSettingsDestIndex {
    pub(crate) fn class_for(&self, destination: &SkinDestinationDef) -> SelectSettingsDestClass {
        self.classes
            .get(&SelectDestEntryKey::from(destination))
            .copied()
            .unwrap_or(SelectSettingsDestClass::Normal)
    }
}

fn for_each_destination(document: &SkinDocument, mut visit: impl FnMut(&SkinDestinationDef)) {
    for entry in &document.destination {
        match entry {
            DestinationListEntry::Single(destination) => visit(destination),
            DestinationListEntry::Conditional { destinations, .. } => {
                for destination in destinations {
                    visit(destination);
                }
            }
        }
    }
}

fn classify_destination_element(
    document: &SkinDocument,
    destination_id: &str,
) -> SelectSettingsDestClass {
    if destination_id_is_detail_panel(destination_id) {
        return SelectSettingsDestClass::AllowConfig;
    }
    if destination_id_is_song_metadata(destination_id) {
        return SelectSettingsDestClass::DenyConfig;
    }
    if document.bpmgraph.iter().any(|graph| graph.id == destination_id)
        || document.judgegraph.iter().any(|graph| graph.id == destination_id)
        || document.gaugegraph.iter().any(|graph| graph.id == destination_id)
        || document.timingdistributiongraph.iter().any(|graph| graph.id == destination_id)
        || document.hiterror_visualizer.iter().any(|v| v.id == destination_id)
        || document.timingvisualizer.iter().any(|v| v.id == destination_id)
        || document.graph.iter().any(|graph| graph.id == destination_id)
        || document.slider.iter().any(|slider| slider.id == destination_id)
    {
        return SelectSettingsDestClass::DenyConfig;
    }
    if document.value.iter().any(|value| value.id == destination_id) {
        return SelectSettingsDestClass::DenyConfig;
    }
    if let Some(text) = document.text.iter().find(|text| text.id == destination_id) {
        if detail_text_ref(text.ref_id) {
            return SelectSettingsDestClass::AllowConfig;
        }
        if !text.constant_text.is_empty() && constant_text_is_song_metadata(&text.constant_text) {
            return SelectSettingsDestClass::DenyConfig;
        }
        return SelectSettingsDestClass::DenyConfig;
    }
    if document.image.iter().any(|image| image.id == destination_id) {
        // 背景・枠などは op 無し/共通 op で描画される。曲詳細 (op=2) 用だけ Deny する。
        return if destination_id_is_song_metadata(destination_id) {
            SelectSettingsDestClass::DenyConfig
        } else {
            SelectSettingsDestClass::Normal
        };
    }
    SelectSettingsDestClass::Normal
}

fn apply_config_duplicate_rules(
    document: &SkinDocument,
    classes: &mut HashMap<SelectDestEntryKey, SelectSettingsDestClass>,
) {
    let mut detail_op2_ids: HashMap<String, bool> = HashMap::new();
    for (key, class) in classes.iter() {
        if *class == SelectSettingsDestClass::AllowConfig && key.ops.contains(&2) {
            detail_op2_ids.insert(key.id.clone(), true);
        }
    }
    for (key, class) in classes.iter_mut() {
        if *class != SelectSettingsDestClass::AllowConfig {
            continue;
        }
        // 曲詳細/パンくずは op=2 を優先し、同一 id の op=1 / op 無しなどを抑止する。
        if !key.ops.contains(&2) && detail_op2_ids.get(&key.id) == Some(&true) {
            *class = SelectSettingsDestClass::DenyConfig;
        }
    }

    let breadcrumb_has_op2 = classes.iter().any(|(key, class)| {
        *class == SelectSettingsDestClass::AllowConfig
            && key.ops.contains(&2)
            && destination_uses_breadcrumb_ref(document, &key.id)
    });
    if !breadcrumb_has_op2 {
        return;
    }
    for (key, class) in classes.iter_mut() {
        if *class != SelectSettingsDestClass::AllowConfig {
            continue;
        }
        if key.ops.contains(&2) {
            continue;
        }
        if destination_uses_breadcrumb_ref(document, &key.id) {
            *class = SelectSettingsDestClass::DenyConfig;
        }
    }
}

fn destination_uses_breadcrumb_ref(document: &SkinDocument, destination_id: &str) -> bool {
    document
        .text
        .iter()
        .find(|text| text.id == destination_id)
        .is_some_and(|text| text.ref_id == BREADCRUMB_TEXT_REF)
}

fn detail_text_ref(ref_id: i32) -> bool {
    DETAIL_TEXT_REFS.contains(&ref_id) || ref_id == BREADCRUMB_TEXT_REF
}

fn constant_text_is_song_metadata(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    ["BPM", "TOTAL", "TIME", "NOTES", "SCORE", "RANK", "LEVEL", "JUDGE", "TARGET", "CLEAR"]
        .iter()
        .any(|token| upper.contains(token))
}

fn destination_id_is_song_metadata(destination_id: &str) -> bool {
    let id = destination_id.to_ascii_lowercase();
    const KEEP: &[&str] = &[
        "title",
        "subtitle",
        "artist",
        "subartist",
        "fulltitle",
        "fullartist",
        "dir",
        "directory",
    ];
    if KEEP.iter().any(|keep| id.contains(keep)) {
        return false;
    }
    const HIDE: &[&str] = &[
        "bpm",
        "total",
        "time",
        "notes",
        "judge",
        "graph",
        "level",
        "genre",
        "clear",
        "score",
        "lamp",
        "playlevel",
        "difficulty",
        "rank",
        "target",
        "replay",
        "song",
        "banner",
        "stage",
        "bms_",
        "flash",
        "minbpm",
        "maxbpm",
        "mainbpm",
    ];
    HIDE.iter().any(|hide| id.contains(hide))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin::{DestinationListEntry, SkinDrawState};

    fn eval_draw(_: &str, _: &SkinDrawState) -> bool {
        true
    }

    fn test_ops(ops: &[i32], enabled_options: &[i32], state: &SkinDrawState) -> bool {
        crate::skin::test_skin_ops(ops, enabled_options, state)
    }

    #[test]
    fn index_prefers_dir_op2_over_empty_op() {
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "w": 100, "h": 100,
                "text": [{ "id": "dir", "font": "f", "ref": 1000 }],
                "destination": [
                    { "id": "dir", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] },
                    { "id": "dir", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] }
                ]
            }"#,
        )
        .unwrap();
        let index = build_select_settings_dest_index(&document);
        let dir_empty = destination_at(&document, 0);
        let dir_op2 = destination_at(&document, 1);
        assert_eq!(index.class_for(dir_op2), SelectSettingsDestClass::AllowConfig);
        assert_eq!(index.class_for(dir_empty), SelectSettingsDestClass::DenyConfig);
    }

    #[test]
    fn config_breadcrumb_op2_filters_m_select_layout_ops() {
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "w": 100, "h": 100,
                "text": [{ "id": "default_info_directory", "font": "f", "ref": 1000 }],
                "destination": [
                    { "id": "default_info_directory", "op": [1, 192], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] },
                    { "id": "default_info_directory", "op": [2, 192], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] },
                    { "id": "default_info_directory", "op": [2, 193], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] }
                ]
            }"#,
        )
        .unwrap();
        let index = build_select_settings_dest_index(&document);
        let snapshot = SelectSnapshot {
            in_settings: true,
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                kind: SelectRowKind::Config,
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };
        let state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Config,
            in_settings: true,
            select_has_banner: false,
            ..SkinDrawState::default()
        };
        let row = &snapshot.rows[0];

        assert!(!test_select_destination_visible(
            &index,
            destination_at(&document, 0),
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
        assert!(test_select_destination_visible(
            &index,
            destination_at(&document, 1),
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
        assert!(!test_select_destination_visible(
            &index,
            destination_at(&document, 2),
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));

        let banner_state = SkinDrawState { select_has_banner: true, ..state };
        assert!(!test_select_destination_visible(
            &index,
            destination_at(&document, 1),
            &[],
            &banner_state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
        assert!(test_select_destination_visible(
            &index,
            destination_at(&document, 2),
            &[],
            &banner_state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
    }

    #[test]
    fn index_prefers_breadcrumb_op2_over_op1_bar() {
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "w": 100, "h": 100,
                "text": [
                    { "id": "dir_bar", "font": "f", "ref": 1000 },
                    { "id": "dir_detail", "font": "f", "ref": 1000 }
                ],
                "destination": [
                    { "id": "dir_bar", "op": [1], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] },
                    { "id": "dir_detail", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] }
                ]
            }"#,
        )
        .unwrap();
        let index = build_select_settings_dest_index(&document);
        assert_eq!(
            index.class_for(destination_at(&document, 0)),
            SelectSettingsDestClass::DenyConfig
        );
        assert_eq!(
            index.class_for(destination_at(&document, 1)),
            SelectSettingsDestClass::AllowConfig
        );
    }

    #[test]
    fn index_marks_dir_allow_and_bpm_deny() {
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "w": 100, "h": 100,
                "text": [{ "id": "dir", "font": "f", "ref": 1000 }],
                "destination": [
                    { "id": "dir", "op": [1], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] },
                    { "id": "dir", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] },
                    { "id": "bpm", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1, "h": 1 }] }
                ]
            }"#,
        )
        .unwrap();
        let index = build_select_settings_dest_index(&document);
        let dir_op1 = destination_at(&document, 0);
        let dir_op2 = destination_at(&document, 1);
        let bpm = destination_at(&document, 2);
        assert_eq!(index.class_for(dir_op2), SelectSettingsDestClass::AllowConfig);
        assert_eq!(index.class_for(dir_op1), SelectSettingsDestClass::DenyConfig);
        assert_eq!(index.class_for(bpm), SelectSettingsDestClass::DenyConfig);
    }

    #[test]
    fn config_keeps_background_image_without_op() {
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "w": 1920, "h": 1080,
                "image": [{ "id": "background", "src": "bg.png" }],
                "destination": [
                    { "id": "background", "dst": [{ "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "img": "background" }] },
                    { "id": "bpm_banner", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10, "img": "bpm_banner" }] }
                ]
            }"#,
        )
        .unwrap();
        let index = build_select_settings_dest_index(&document);
        let snapshot = SelectSnapshot {
            in_settings: true,
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                kind: SelectRowKind::Config,
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };
        let state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Config,
            in_settings: true,
            ..SkinDrawState::default()
        };
        let bg = destination_at(&document, 0);
        let bpm_banner = destination_at(&document, 1);
        assert_eq!(index.class_for(bg), SelectSettingsDestClass::Normal);
        assert!(test_select_destination_visible(
            &index,
            bg,
            &[],
            &state,
            &snapshot,
            Some(&snapshot.rows[0]),
            eval_draw,
            test_ops,
        ));
        assert!(!test_select_destination_visible(
            &index,
            bpm_banner,
            &[],
            &state,
            &snapshot,
            Some(&snapshot.rows[0]),
            eval_draw,
            test_ops,
        ));
    }

    #[test]
    fn config_shows_detail_text_and_hides_bpm() {
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "w": 1920, "h": 1080,
                "text": [
                    { "id": "title", "font": "f", "ref": 10 },
                    { "id": "artist", "font": "f", "ref": 16 },
                    { "id": "dir", "font": "f", "ref": 1000 },
                    { "id": "bpm_label", "font": "f", "constantText": "BPM" }
                ],
                "destination": [
                    { "id": "title", "op": [1], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "title", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "artist", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "dir", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "bpm_label", "op": [2], "dst": [{ "time": 0, "x": 0, "y": 0, "w": 10, "h": 10 }] }
                ]
            }"#,
        )
        .unwrap();
        let index = build_select_settings_dest_index(&document);
        let snapshot = SelectSnapshot {
            in_settings: true,
            selected_index: 0,
            rows: vec![SelectRowSnapshot {
                index: 0,
                title: "MASTER".to_string(),
                artist: "25".to_string(),
                kind: SelectRowKind::Config,
                ..SelectRowSnapshot::default()
            }],
            ..SelectSnapshot::default()
        };
        let row = &snapshot.rows[0];
        let state = SkinDrawState {
            select_screen: true,
            select_row_kind: SelectRowKind::Config,
            in_settings: true,
            ..SkinDrawState::default()
        };
        let title_op1 = destination_at(&document, 0);
        let title_op2 = destination_at(&document, 1);
        let artist = destination_at(&document, 2);
        let dir = destination_at(&document, 3);
        let bpm_label = destination_at(&document, 4);

        assert_eq!(index.class_for(title_op1), SelectSettingsDestClass::DenyConfig);
        assert!(test_select_destination_visible(
            &index,
            title_op2,
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
        assert!(test_select_destination_visible(
            &index,
            artist,
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
        assert!(test_select_destination_visible(
            &index,
            dir,
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
        assert!(!test_select_destination_visible(
            &index,
            bpm_label,
            &[],
            &state,
            &snapshot,
            Some(row),
            eval_draw,
            test_ops,
        ));
    }

    fn destination_at(document: &SkinDocument, index: usize) -> &SkinDestinationDef {
        match &document.destination[index] {
            DestinationListEntry::Single(destination) => destination,
            DestinationListEntry::Conditional { .. } => panic!("unexpected conditional"),
        }
    }
}
