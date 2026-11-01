//! G-BATTLE opponent panel added to the separately installed ADFX02/ECFN select skin.
use super::*;
use bmz_core::lane::KeyMode;
use bmz_render::plan::TextureId;
use bmz_render::scene::SelectBattleSnapshot;
use ecfn_detail_options::{decode, skin_path};

// Below the chart graph; the right edge meets the TYPE-A IR DATA window.
const LEFT: f32 = 441.0;
const TOP: f32 = 696.0;
const WIDTH: usize = 715;
const HEIGHT: usize = 108;

struct Panel {
    renderer: Renderer,
    keys: TextureId,
    system: TextureId,
}

impl Panel {
    fn new(experimental: bool) -> Self {
        let decoded = decode(experimental);
        let texture = |id: &str| {
            TextureId(decoded.sources.iter().find(|s| s.source_id == id).unwrap().texture.0)
        };
        let (keys, system) = (texture("ecfn_battle_keys"), texture("9"));
        let mut renderer = Renderer::default();
        install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
            .unwrap();
        Self { renderer, keys, system }
    }

    fn show(&mut self, battle: Option<SelectBattleSnapshot>) -> &[DrawCommand] {
        let snapshot = SelectSnapshot {
            time: TimeUs(2_000_000),
            chart_replication_mode: "RIVALCHART".into(),
            battle: battle.map(Arc::new),
            ..Default::default()
        };
        self.renderer.prepare_scene(AppSceneSnapshot::Select(snapshot));
        &self.renderer.last_plan().unwrap().commands
    }

    /// Text and opacity, in drawing order.
    fn texts(&self) -> Vec<(String, f32)> {
        self.renderer
            .last_plan()
            .unwrap()
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, style, .. } => Some((text.clone(), style.color.a)),
                _ => None,
            })
            .collect()
    }

    /// Lane labels per row, left to right: Starseeker tiles by their printed number,
    /// blank tiles by the window digit drawn on them.
    fn lanes(&self) -> Vec<Vec<u8>> {
        let mut rows: BTreeMap<i32, Vec<(f32, u8)>> = BTreeMap::new();
        for command in &self.renderer.last_plan().unwrap().commands {
            let DrawCommand::Image { texture, rect, uv, .. } = command else { continue };
            let (x, y, w, h) =
                (rect.x * 1920.0, rect.y * 1080.0, rect.width * 1920.0, rect.height * 1080.0);
            if *texture == self.keys && (w - 31.0).abs() < 0.01 {
                // Starseeker's result tiles stay at their native 31x29 size.
                assert!((h - 29.0).abs() < 0.01);
                let number = ((uv.x * 742.0 - 1.0) / 31.0).round() as u8 + 1;
                rows.entry(y.round() as i32).or_default().push((x, number));
            } else if *texture == self.system
                && (uv.y * 3810.0 - 797.0).abs() < 0.01
                && (w - 18.0).abs() < 0.01
                && (LEFT..LEFT + WIDTH as f32).contains(&x)
                && (TOP..TOP + HEIGHT as f32).contains(&y)
            {
                // PLAY DATA draws the same digits at 17 px; the panel keeps them at 18 px.
                let digit = ((uv.x * 1920.0 - 166.0) / 18.0).round() as u8;
                rows.entry((y - 5.0).round() as i32).or_default().push((x - 7.0, digit));
            }
        }
        rows.into_values()
            .map(|mut row| {
                row.sort_by(|a, b| a.0.total_cmp(&b.0));
                for pair in row.windows(2) {
                    let gap = pair[1].0 - pair[0].0;
                    assert!((gap % 28.0).abs() < 0.01, "Starseeker uses a 28 px pitch");
                }
                row.into_iter().map(|(_, label)| label).collect()
            })
            .collect()
    }

    /// How often each panel pixel is covered by the rebuilt SCORE DATA window.
    fn frame_coverage(&self) -> Vec<u8> {
        let mut coverage = vec![0u8; WIDTH * HEIGHT];
        for command in &self.renderer.last_plan().unwrap().commands {
            let DrawCommand::Image { texture, rect, uv, .. } = command else { continue };
            let (source_y, source_h) = (uv.y * 3810.0, uv.height * 3810.0);
            if *texture != self.system || !(612.5..796.0).contains(&source_y) {
                continue;
            }
            let (x, y) = ((rect.x * 1920.0 - LEFT).round(), (rect.y * 1080.0 - TOP).round());
            let (w, h) = ((rect.width * 1920.0).round(), (rect.height * 1080.0).round());
            // The native windows also come from this part of the atlas.
            if x < 0.0 || y < 0.0 || x + w > WIDTH as f32 || y + h > HEIGHT as f32 {
                continue;
            }
            // Only the smooth body gradient is shortened; the title bar, borders and
            // the edge shading keep their size.
            if (h - 62.0).abs() > 0.5 {
                assert!((h - source_h).abs() < 0.5, "frame band at {y} must stay unscaled");
            }
            if (uv.width * 1920.0 - 1.0).abs() > 0.01 {
                assert!(
                    (w - uv.width * 1920.0).abs() < 0.5,
                    "frame edge at {x} must stay unscaled"
                );
            }
            for row in y as usize..(y + h) as usize {
                for column in x as usize..(x + w) as usize {
                    coverage[row * WIDTH + column] += 1;
                }
            }
        }
        coverage
    }
}

fn pattern(keys: &[u8]) -> Vec<u8> {
    let mut pattern: Vec<u8> = (0..16).collect();
    pattern[1..=keys.len()].copy_from_slice(keys);
    pattern
}

fn single(arrange: &str, keys: &[u8]) -> SelectBattleSnapshot {
    SelectBattleSnapshot {
        name: "opponent name must stay hidden".into(),
        key_mode: KeyMode::K7,
        arrange: Some(arrange.into()),
        lane_shuffle_pattern: pattern(keys),
        ..Default::default()
    }
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_battle_info_draws_the_opponent_like_the_native_windows() {
    for experimental in [false, true] {
        let mut panel = Panel::new(experimental);
        panel.show(None);
        let closed = panel.texts();
        assert!(!closed.iter().any(|(text, _)| text == "G-BATTLE DATA"));
        assert!(panel.lanes().is_empty());

        panel.show(Some(single("RANDOM", &[2, 5, 3, 7, 1, 4, 6])));
        let texts = panel.texts();
        for expected in ["G-BATTLE DATA", "RANDOM", "RIVALCHART"] {
            assert!(texts.iter().any(|(text, a)| text == expected && *a == 1.0), "{expected}");
        }
        assert!(!texts.iter().any(|(text, _)| text == "opponent name must stay hidden"));
        assert_eq!(panel.lanes(), [[2, 5, 3, 7, 1, 4, 6]]);
        assert!(panel.frame_coverage().iter().all(|&count| count == 1));

        // DP FLIP: each side's tiles are numbered within that side.
        let mut flip: Vec<u8> = (0..16).collect();
        flip[0] = 15;
        flip[15] = 0;
        flip[1..8].copy_from_slice(&[9, 12, 10, 14, 8, 11, 13]);
        flip[8..15].copy_from_slice(&[7, 6, 5, 4, 3, 2, 1]);
        let double = SelectBattleSnapshot {
            key_mode: KeyMode::K14,
            arrange_2p: Some("MIRROR".into()),
            double_option: Some("FLIP".into()),
            lane_shuffle_pattern: flip,
            ..single("RANDOM", &[])
        };
        panel.show(Some(double.clone()));
        let texts = panel.texts();
        for expected in ["1P", "2P", "RANDOM", "MIRROR", "DP", "FLIP", "RIVALCHART"] {
            assert!(texts.iter().any(|(text, _)| text == expected), "DP: {expected}");
        }
        assert_eq!(panel.lanes(), [[2, 5, 3, 7, 1, 4, 6], [7, 6, 5, 4, 3, 2, 1]]);
        assert!(panel.frame_coverage().iter().all(|&count| count == 1));

        // 9K lanes 8 and 9, and a scratch moved by RANDOM-EX, have no Starseeker tile.
        panel.show(Some(SelectBattleSnapshot {
            key_mode: KeyMode::K9,
            lane_shuffle_pattern: vec![0, 9, 5, 3, 7, 1, 8, 4, 2, 6, 10, 11, 12, 13, 14, 15],
            ..single("RANDOM", &[])
        }));
        assert_eq!(panel.lanes(), [[9, 5, 3, 7, 1, 8, 4, 2, 6]]);
        assert!(!panel.texts().iter().any(|(text, _)| text == "S"));
        let mut scratch = pattern(&[2, 5, 3, 7, 1, 4, 6]);
        scratch.swap(0, 3);
        panel.show(Some(SelectBattleSnapshot {
            lane_shuffle_pattern: scratch,
            ..single("RANDOM-EX", &[])
        }));
        assert!(panel.texts().iter().any(|(text, _)| text == "S"));
        assert_eq!(panel.lanes(), [[2, 5, 7, 1, 4, 6]]);

        // Missing data is shown like the windows' dim leading zeros.
        for (battle, expected) in [
            (SelectBattleSnapshot { arrange: None, ..single("RANDOM", &[]) }, "NO DATA"),
            (
                SelectBattleSnapshot { arrange: None, loading: true, ..single("RANDOM", &[]) },
                "LOADING",
            ),
            (single("S-RANDOM", &[]), "NOT FIXED"),
            (
                SelectBattleSnapshot { lane_shuffle_pattern: vec![], ..single("RANDOM", &[]) },
                "NO DATA",
            ),
            (SelectBattleSnapshot { arrange_2p: None, ..double }, "NO DATA"),
        ] {
            let battle = SelectBattleSnapshot { lane_shuffle_pattern: vec![], ..battle };
            panel.show(Some(battle));
            assert!(panel.lanes().is_empty());
            let dim: Vec<_> = panel.texts().into_iter().filter(|(_, a)| *a < 1.0).collect();
            assert!(
                !dim.is_empty()
                    && dim
                        .iter()
                        .all(|(text, a)| text == expected && (a - 128.0 / 255.0).abs() < 0.01),
                "{expected}: {dim:?}"
            );
        }
        panel.show(None);
        assert_eq!(panel.texts(), closed, "closing must restore the native screen");
    }
}

#[test]
#[ignore = "requires separately installed ADFX02/ECFN assets"]
fn ecfn_battle_info_draws_among_the_windows_and_falls_back_atomically() {
    let destinations = |document: &SkinDocument| -> Vec<String> {
        document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                DestinationListEntry::Single(destination) => Some(destination.id.clone()),
                _ => None,
            })
            .collect()
    };
    for experimental in [false, true] {
        // Contiguous, just before the windows and so under the option panels.
        let ids = destinations(&decode(experimental).document);
        let first = ids.iter().position(|id| id.starts_with("ecfn_battle_")).unwrap();
        let last = ids.iter().rposition(|id| id.starts_with("ecfn_battle_")).unwrap();
        assert!(ids[first..=last].iter().all(|id| id.starts_with("ecfn_battle_")));
        assert_eq!(ids[last + 1], "score_window");
    }

    let source = skin_path();
    let source = source.parent().unwrap();
    for state in ["loaded", "missing", "failed", "non-bmz"] {
        let root = unique_test_dir("bmz-ecfn-battle-fallback");
        fs::create_dir_all(&root).unwrap();
        for name in ["select.lua", "bmz_select_extensions.lua", "bmz_detail_options.lua"] {
            fs::copy(source.join(name), root.join(name)).unwrap();
        }
        if state != "missing" {
            let code = fs::read_to_string(source.join("bmz_battle_info.lua")).unwrap();
            let code = if state == "failed" {
                code.replace("\treturn skin\n", "\terror('construction failed')\n\treturn skin\n")
            } else {
                code
            };
            fs::write(root.join("bmz_battle_info.lua"), code).unwrap();
        }
        fs::write(
            root.join("select.luaskin"),
            format!(
                "{} local t = require('select'); if skin_config then return t.main() else return t.header end",
                if state == "non-bmz" { "bmz = nil;" } else { "" }
            ),
        )
        .unwrap();
        let loaded = bmz_skin::load_lua_skin(
            &root.join("select.luaskin"),
            bmz_skin::SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let battle = state == "loaded";
        let ids = destinations(&loaded.document);
        assert_eq!(ids.iter().any(|id| id.starts_with("ecfn_battle_")), battle, "{state}");
        assert_eq!(loaded.document.font.iter().any(|f| f.id == "ecfn_battle_font"), battle);
        // The basic extensions load independently of the battle panel.
        assert_eq!(
            loaded.document.text.iter().any(|t| t.id == "bmz_select_mode"),
            state != "non-bmz"
        );
        assert_eq!(ids.iter().filter(|id| *id == "score_window").count(), 1);
    }
}
