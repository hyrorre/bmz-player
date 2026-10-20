//! 複数コードポイントの絵文字シーケンスを、合字・置換が成立した face だけで描く回帰テスト。

use super::unicode_text::*;
use super::*;

fn parts_font() -> TextFace {
    fixture_face("bmz-test-emoji-parts.ttf")
}

fn keycap_mark_font() -> TextFace {
    fixture_face("bmz-test-keycap-mark.ttf")
}

fn chain(faces: &[TextFace]) -> VectorFontSet<'_> {
    VectorFontSet { id: "chain", faces: faces_only(faces) }
}

const FAMILY: &str = "👨\u{200D}👩\u{200D}👧\u{200D}👦";
const TECHNOLOGIST: &str = "👩\u{200D}💻";
const THUMBS_LIGHT: &str = "👍🏻";
const FLAG_JP: &str = "🇯🇵";
const KEYCAP_1: &str = "1\u{FE0F}\u{20E3}";

/// 合字が必要なシーケンス。部品だけのフォントでは描けない。
const LIGATURE_SEQUENCES: [&str; 5] = [FAMILY, TECHNOLOGIST, THUMBS_LIGHT, FLAG_JP, KEYCAP_1];

/// 1つの cluster として、`face` のカラー合字1グリフで描かれていること。
fn assert_ligature(text: &str, faces: &[TextFace], face: u16) {
    let shaped = shape_vector_text(text, chain(faces));
    assert_eq!(cluster_texts(text, &shaped), [text], "{text}");
    let glyphs = shaped.glyphs(&shaped.clusters[0]);
    assert_eq!(glyphs.len(), 1, "{text} must be drawn as one ligature");
    let GlyphSource::Vector { face: found, glyph, color, .. } = glyphs[0].source else {
        panic!("{text}: vector glyph expected, got {:?}", glyphs[0].source);
    };
    assert_eq!((found, color), (face, true), "{text}");
    let font = faces[usize::from(face)].font();
    assert!(
        !font.codepoint_ids().any(|(id, _)| id == glyph),
        "{text} must use the substituted glyph, not a component"
    );
}

/// cluster 全体が1つの欠字表示になり、部品や ZWJ を描かないこと。
fn assert_single_placeholder(text: &str, faces: &[TextFace]) {
    let shaped = shape_vector_text(text, chain(faces));
    assert_eq!(cluster_texts(text, &shaped), [text], "{text}");
    let glyphs = shaped.glyphs(&shaped.clusters[0]);
    assert_eq!(glyphs.len(), 1, "{text} must not be split into its components");
    // 欠字表示は □ 等の代替候補か、生成した枠。
    assert_eq!(glyphs[0], chain(faces).missing_glyph().1, "{text}");
}

#[test]
fn component_only_font_does_not_count_as_supporting_a_sequence() {
    let faces = [latin(), parts_font()];
    for text in LIGATURE_SEQUENCES {
        assert_single_placeholder(text, &faces);
    }
    // ❤️ は1文字＋VS16で、合字を必要としない。部品フォントのカラーグリフで描く。
    let shaped = shape_vector_text("❤\u{FE0F}", chain(&faces));
    assert_eq!(glyph_paint(&shaped, 0), (1, true));
}

#[test]
fn ligature_font_draws_each_sequence_as_one_glyph() {
    let faces = [latin(), color_font()];
    for text in LIGATURE_SEQUENCES {
        assert_ligature(text, &faces, 1);
    }
    let shaped = shape_vector_text("❤\u{FE0F}", chain(&faces));
    assert_eq!(glyph_paint(&shaped, 0), (1, true));
}

#[test]
fn later_font_with_the_ligature_wins_over_an_earlier_component_only_font() {
    let faces = [latin(), parts_font(), color_font()];
    for text in LIGATURE_SEQUENCES {
        assert_ligature(text, &faces, 2);
    }
}

#[test]
fn unsupported_sequences_are_one_placeholder_inside_text() {
    let faces = [latin(), parts_font()];
    let text = format!("A{FAMILY}B");
    let shaped = shape_vector_text(&text, chain(&faces));
    assert_eq!(cluster_texts(&text, &shaped), ["A", FAMILY, "B"]);
    assert_eq!(tofu_glyphs(&shaped), 1);
    let glyphs: usize = shaped.clusters.iter().map(|cluster| shaped.glyphs(cluster).len()).sum();
    assert_eq!(glyphs, 3, "no component or ZWJ glyph may be drawn");
    // Truncate・caret も cluster を分割しない。
    let a = shaped.clusters[0].advance;
    let tofu = shaped.clusters[1].advance;
    assert_eq!(truncated_cluster_count(&shaped.clusters, 1.0, a + tofu * 0.5), 1);
    assert_approx(prefix_width_px(&shaped.clusters, "A👨\u{200D}".len(), 1.0), a);
    let lines = wrap_clusters(&shaped.clusters, 1.0, a + tofu * 0.5);
    assert!(lines.iter().all(|line| line.end - line.start >= 1));
    assert!(lines.contains(&(1..2)), "{lines:?}");

    let faces = [latin(), parts_font(), color_font()];
    let shaped = shape_vector_text(&text, chain(&faces));
    assert_eq!(tofu_glyphs(&shaped), 0);
    assert_eq!(glyph_paint(&shaped, 1), (2, true));
}

#[test]
fn sequences_without_any_capable_font_are_one_placeholder() {
    for faces in [vec![latin()], vec![latin(), parts_font()], vec![latin(), symbols()]] {
        for text in LIGATURE_SEQUENCES {
            assert_single_placeholder(text, &faces);
        }
    }
}

/// キーキャップは合字だけでなく、数字＋結合用囲み（U+20E3）のマーク配置でも正しく描ける。
#[test]
fn keycap_may_be_drawn_with_a_combining_mark() {
    let faces = [latin(), keycap_mark_font()];
    let shaped = shape_vector_text(KEYCAP_1, chain(&faces));
    assert_eq!(shaped.clusters.len(), 1);
    let glyphs = shaped.glyphs(&shaped.clusters[0]);
    assert_eq!(glyphs.len(), 2, "{glyphs:?}");
    assert!(
        glyphs
            .iter()
            .all(|glyph| matches!(glyph.source, GlyphSource::Vector { face: 1, color: false, .. }))
    );
    // 結合記号は送り幅を持たず、数字に重ねる。
    let digit = &faces[1];
    let units = chain(&faces).units_scale(1);
    assert_approx(
        shaped.clusters[0].advance,
        digit.h_advance_unscaled(digit.glyph_id('1')) * units,
    );
}

/// 結合文字の cluster は複数グリフのまま描く（1グリフでなくてもよい）。
#[test]
fn combining_mark_clusters_keep_multiple_glyphs() {
    let faces = [keycap_mark_font()];
    let shaped = shape_vector_text("1\u{20E3}", chain(&faces));
    assert_eq!(shaped.clusters.len(), 1);
    assert_eq!(shaped.glyphs(&shaped.clusters[0]).len(), 2);
    assert_eq!(tofu_glyphs(&shaped), 0);
}

#[test]
fn bitmap_text_rejects_component_only_sequences_too() {
    let primary = test_bitmap_font();
    let entries = bitmap_entries(&[&primary]);
    let parts = [parts_font()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&parts) };
    for text in LIGATURE_SEQUENCES {
        let shaped = shape_bitmap_text(text, fonts);
        assert_eq!(shaped.clusters.len(), 1, "{text}");
        assert_eq!(tofu_glyphs(&shaped), 1, "{text}");
        assert_eq!(shaped.glyphs(&shaped.clusters[0]).len(), 1, "{text}");
    }
    let both = [parts_font(), color_font()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&both) };
    for text in LIGATURE_SEQUENCES {
        let shaped = shape_bitmap_text(text, fonts);
        assert_eq!(glyph_paint(&shaped, 0), (1, true), "{text}");
    }
}

/// OS の Noto Color Emoji（Linux、CBDT）が入っていれば、合字の判定が実際のフォントでも
/// 成立すること。入っていない環境では何も確認しない。
#[test]
fn os_color_emoji_font_forms_rgi_sequences_when_installed() {
    let Some(symbol) =
        bmz_font::symbol_font_fallbacks().iter().find(|font| font.family() == "Noto Color Emoji")
    else {
        return;
    };
    let faces = [latin(), load_symbol_faces(std::slice::from_ref(symbol)).remove(0)];
    for text in [FAMILY, TECHNOLOGIST, THUMBS_LIGHT, FLAG_JP, KEYCAP_1, ENGLAND, SCOTLAND, WALES] {
        let shaped = shape_vector_text(text, chain(&faces));
        assert_eq!(shaped.clusters.len(), 1, "{text}");
        assert_eq!(shaped.glyphs(&shaped.clusters[0]).len(), 1, "{text}");
        assert_eq!(glyph_paint(&shaped, 0), (1, true), "{text}");
    }
}

const ENGLAND: &str = "🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";
const SCOTLAND: &str = "🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}";
const WALES: &str = "🏴\u{E0067}\u{E0062}\u{E0077}\u{E006C}\u{E0073}\u{E007F}";
const TAG_FLAGS: [&str; 3] = [ENGLAND, SCOTLAND, WALES];

#[test]
fn tag_sequences_are_shaped_as_whole_clusters() {
    let faces = [latin(), color_font()];
    for text in TAG_FLAGS {
        assert_ligature(text, &faces, 1);
    }
    // 黒い旗だけの cluster とは別のグリフになる。
    let black_flag = shape_vector_text("🏴", chain(&faces));
    let england = shape_vector_text(ENGLAND, chain(&faces));
    assert_ne!(shaped_glyph_id(&black_flag), shaped_glyph_id(&england));
    // 後続の対応 face を、黒い旗だけを持つ face より優先する。
    let faces = [latin(), parts_font(), color_font()];
    for text in TAG_FLAGS {
        assert_ligature(text, &faces, 2);
    }
}

fn shaped_glyph_id(shaped: &ShapedText) -> GlyphId {
    match shaped.glyphs(&shaped.clusters[0])[0].source {
        GlyphSource::Vector { glyph, .. } => glyph,
        other => panic!("{other:?}"),
    }
}

#[test]
fn unsupported_tag_sequences_are_one_placeholder_not_a_bare_black_flag() {
    // 部品フォントは黒い旗とタグ文字を持つが、タグシーケンスの合字は持たない。
    for faces in [vec![latin()], vec![latin(), parts_font()]] {
        for text in TAG_FLAGS {
            assert_single_placeholder(text, &faces);
        }
    }
    // 黒い旗だけなら部品フォントで描ける。
    let faces = [latin(), parts_font()];
    let shaped = shape_vector_text("🏴", chain(&faces));
    assert_eq!(glyph_paint(&shaped, 0), (1, true));
}

#[test]
fn tag_sequences_between_letters_keep_one_cluster() {
    for (faces, placeholder) in
        [(vec![latin(), color_font()], false), (vec![latin(), parts_font()], true)]
    {
        let text = format!("A{ENGLAND}B");
        let shaped = shape_vector_text(&text, chain(&faces));
        assert_eq!(cluster_texts(&text, &shaped), ["A", ENGLAND, "B"]);
        assert_eq!(tofu_glyphs(&shaped), usize::from(placeholder));
        let glyphs: usize =
            shaped.clusters.iter().map(|cluster| shaped.glyphs(cluster).len()).sum();
        assert_eq!(glyphs, 3, "tag characters must never be drawn on their own");
        let a = shaped.clusters[0].advance;
        assert_approx(prefix_width_px(&shaped.clusters, "A🏴\u{E0067}".len(), 1.0), a);
    }
}

#[test]
fn bitmap_text_shapes_tag_sequences_through_vector_faces() {
    let primary = test_bitmap_font();
    let entries = bitmap_entries(&[&primary]);
    let parts = [parts_font()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&parts) };
    for text in TAG_FLAGS {
        let shaped = shape_bitmap_text(text, fonts);
        assert_eq!(shaped.clusters.len(), 1, "{text}");
        assert_eq!(tofu_glyphs(&shaped), 1, "{text}");
    }
    let both = [parts_font(), color_font()];
    let fonts = BitmapFontSet { id: "bitmap", fonts: &entries, faces: faces_only(&both) };
    for text in TAG_FLAGS {
        let shaped = shape_bitmap_text(text, fonts);
        assert_eq!(shaped.glyphs(&shaped.clusters[0]).len(), 1, "{text}");
        assert_eq!(glyph_paint(&shaped, 0), (1, true), "{text}");
    }
}
