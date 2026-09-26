use super::*;

fn noto_data(font_index: u32) -> SystemFontData {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/fonts/noto-cjk/NotoSansCJK-Regular.ttc");
    SystemFontData { bytes: std::fs::read(path).expect("bundled Noto CJK font"), font_index }
}

#[test]
fn cjk_font_definitions_keep_latin_first_and_preserve_face_indices() {
    let defaults = FontDefinitions::default();
    let fonts = cjk_font_definitions(vec![
        (FontCoverage::Korean, noto_data(1)),
        (FontCoverage::Japanese, noto_data(5)),
    ]);
    let reverse = cjk_font_definitions(vec![
        (FontCoverage::Japanese, noto_data(5)),
        (FontCoverage::Korean, noto_data(1)),
    ]);
    for family in FAMILIES {
        let default_chain = &defaults.families[&family];
        let chain = &fonts.families[&family];
        assert_eq!(&chain[..default_chain.len()], default_chain);
        let names = [FontCoverage::Korean, FontCoverage::Japanese]
            .map(|coverage| cjk_font_name(coverage, &family));
        assert_eq!(&chain[default_chain.len()..], &names);
        for (name, index) in names.iter().zip([1, 5]) {
            assert_eq!(fonts.font_data[name].index, index);
            assert_eq!(fonts.font_data[name].tweak, reverse.font_data[name].tweak);
        }
    }
    for (name, data) in defaults.font_data {
        assert_eq!(fonts.font_data[&name], data);
    }
}

fn assert_alignment(data: SystemFontData, coverage: FontCoverage) {
    let definitions = cjk_font_definitions(vec![(coverage, data)]);
    let mut fonts = egui::epaint::Fonts::new(Default::default(), definitions);
    let mut defaults = egui::epaint::Fonts::new(Default::default(), FontDefinitions::default());
    for native_scale in [1.0, 1.5, 2.0] {
        for zoom in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let pixels_per_point = native_scale * zoom;
            let mut view = fonts.with_pixels_per_point(pixels_per_point);
            let mut default_view = defaults.with_pixels_per_point(pixels_per_point);
            for family in FAMILIES {
                for size in [12.0, 13.0, 14.0, 18.0, 24.0] {
                    let id = FontId::new(size, family.clone());
                    let latin =
                        view.layout_no_wrap(LATIN_SAMPLE.into(), id.clone(), Color32::WHITE);
                    let original = default_view.layout_no_wrap(
                        LATIN_SAMPLE.into(),
                        id.clone(),
                        Color32::WHITE,
                    );
                    assert_eq!(latin.rect, original.rect);
                    assert_eq!(latin.mesh_bounds, original.mesh_bounds);
                    let cjk = view.layout_no_wrap(cjk_sample(coverage).into(), id, Color32::WHITE);
                    let delta = cjk.mesh_bounds.center().y - latin.mesh_bounds.center().y;
                    // Optical bounds vary slightly with hinting and pixel rounding.
                    // The old fixed 0.26 tweak puts bundled Noto about 5 pt too low at 14 pt.
                    assert!(
                        delta.abs() <= 1.0 + 0.5 / pixels_per_point,
                        "{coverage:?} {family:?} size={size} scale={pixels_per_point}: delta={delta}"
                    );
                }
            }
        }
    }
}

#[test]
fn bundled_cjk_faces_align_at_ui_sizes_and_scales() {
    for (index, coverage) in [
        FontCoverage::Japanese,
        FontCoverage::Korean,
        FontCoverage::SimplifiedChinese,
        FontCoverage::TraditionalChinese,
        FontCoverage::HongKong,
    ]
    .into_iter()
    .enumerate()
    {
        assert_alignment(noto_data(index as u32), coverage);
    }
}

#[test]
fn system_japanese_font_aligns_at_ui_sizes_and_scales() {
    // Exercises Yu Gothic on Windows and the selected native fallback on other OSes.
    if let Some(data) =
        bmz_render::renderer::load_system_font_data_for_coverage(FontCoverage::Japanese)
    {
        assert_alignment(data, FontCoverage::Japanese);
    }
}

#[test]
fn alignment_preserves_advances_line_height_and_small_kana_positions() {
    let corrected = cjk_font_definitions(vec![(FontCoverage::Japanese, noto_data(0))]);
    let mut uncorrected = corrected.clone();
    for family in FAMILIES {
        let name = cjk_font_name(FontCoverage::Japanese, &family);
        Arc::make_mut(uncorrected.font_data.get_mut(&name).unwrap()).tweak.y_offset_factor = 0.0;
    }
    let mut corrected = egui::epaint::Fonts::new(Default::default(), corrected);
    let mut uncorrected = egui::epaint::Fonts::new(Default::default(), uncorrected);
    for family in FAMILIES {
        let id = FontId::new(18.0, family);
        let text = "日本語あぁゃ。、\n日本語";
        let before = uncorrected.with_pixels_per_point(2.0).layout_no_wrap(
            text.into(),
            id.clone(),
            Color32::WHITE,
        );
        let after =
            corrected.with_pixels_per_point(2.0).layout_no_wrap(text.into(), id, Color32::WHITE);
        assert_eq!(before.rect, after.rect);
        assert_eq!(before.rows.len(), after.rows.len());
        let shift = after.mesh_bounds.min.y - before.mesh_bounds.min.y;
        assert_ne!(shift, 0.0, "fixture must exercise a nonzero correction");
        for (before_row, after_row) in before.rows.iter().zip(&after.rows) {
            assert_eq!(before_row.size, after_row.size);
            let before_vertices = &before_row.visuals.mesh.vertices;
            let after_vertices = &after_row.visuals.mesh.vertices;
            assert_eq!(before_vertices.len(), after_vertices.len());
            for (before, after) in before_vertices.iter().zip(after_vertices) {
                assert_eq!(before.pos.x, after.pos.x);
                assert!((after.pos.y - before.pos.y - shift).abs() < 0.001);
            }
        }
    }
}

#[test]
fn missing_probe_glyphs_do_not_calibrate_against_replacement_glyphs() {
    let definitions = FontDefinitions::default();
    let latin_name = &definitions.families[&FontFamily::Proportional][0];
    let latin = &definitions.font_data[latin_name];
    assert_eq!(font_y_offsets(latin, FontCoverage::Japanese), [0.0; 2]);
}
