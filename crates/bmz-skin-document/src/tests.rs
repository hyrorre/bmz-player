//! serde decode と include 展開の純 document テスト。
//! 描画評価を含むテストは `bmz-render/src/skin.rs` 側に残している。

use std::path::PathBuf;

use super::*;

fn unique_test_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    path
}

#[test]
fn skin_document_normalizes_numeric_and_string_ids() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 0,
            "source": [
                { "id": 100, "path": "a.png" },
                { "id": "100", "path": "b.png" }
            ],
            "image": [
                { "id": 200, "src": 100, "x": 0, "y": 0, "w": 8, "h": 8 },
                { "id": "300", "src": "100", "x": 8, "y": 0, "w": 8, "h": 8 }
            ],
            "imageset": [
                { "id": "set", "images": [200, "300"] }
            ],
            "destination": [
                { "id": 200, "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] }
            ]
        }
        "#,
    )
    .unwrap();

    assert_eq!(document.source[0].id, "100");
    assert_eq!(document.source[1].id, "100");
    assert_eq!(document.image[0].id, "200");
    assert_eq!(document.image[0].src, "100");
    assert_eq!(document.image[1].src, "100");
    assert_eq!(document.imageset[0].images, ["200", "300"]);
    let DestinationListEntry::Single(dst0) = &document.destination[0] else {
        panic!("expected Single destination");
    };
    assert_eq!(dst0.id, "200");
}

#[test]
fn skin_document_expands_conditions_and_includes() {
    let root = unique_test_dir("bmz-skin-document-json");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("included.json"),
        r#"
        [
            { "id": "included", "src": "1", "x": 0, "y": 0, "w": 8, "h": 8, },
            { "if": -901, "value": { "id": "disabled", "src": "1" } }
        ]
        "#,
    )
    .unwrap();
    std::fs::write(
        root.join("skin.json"),
        r#"
        {
            "type": 0,
            "property": [
                { "name": "Graph", "def": "On", "item": [
                    { "name": "Off", "op": 900 },
                    { "name": "On", "op": 901 }
                ]}
            ],
            "source": [{ "id": 1, "path": "system.png" }],
            "image": { "include": "included.json" },
            "destination": [
                { "if": 901, "value": { "id": "included", "dst": [{ "x": 1, "y": 2, "w": 3, "h": 4 }] } },
                { "if": -901, "value": { "id": "disabled", "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] } }
            ],
        }
        "#,
    )
    .unwrap();

    let document = SkinDocument::load_beatoraja_json(&root.join("skin.json")).unwrap();

    assert_eq!(document.source[0].id, "1");
    assert_eq!(document.image.len(), 1);
    assert_eq!(document.image[0].id, "included");
    assert_eq!(document.destination.len(), 1);
    let DestinationListEntry::Single(dst0) = &document.destination[0] else {
        panic!("expected Single destination");
    };
    assert_eq!(dst0.id, "included");
    let SkinDstEntry::Frame(frame) = &dst0.dst[0] else {
        panic!("expected Frame entry");
    };
    assert_eq!(frame.x, Some(1));
}
