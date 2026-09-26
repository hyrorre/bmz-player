use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use bmz_render::FontCoverage;
use bmz_render::renderer::SystemFontData;
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};
use sha2::{Digest, Sha256};

const FAMILIES: [FontFamily; 2] = [FontFamily::Proportional, FontFamily::Monospace];
const ALIGNMENT_SIZE: f32 = 128.0;
const LATIN_SAMPLE: &str = "Hx0";

type AlignmentKey = ([u8; 32], u32, FontCoverage);
static ALIGNMENTS: OnceLock<Mutex<HashMap<AlignmentKey, [f32; 2]>>> = OnceLock::new();

pub(super) fn cjk_font_definitions(
    fallbacks: Vec<(FontCoverage, SystemFontData)>,
) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (coverage, data) in fallbacks {
        let mut font_data = FontData::from_owned(data.bytes);
        font_data.index = data.font_index;
        let font_data = Arc::new(font_data);
        let offsets = font_y_offsets(&font_data, coverage);
        let font_data = Arc::unwrap_or_clone(font_data);
        // egui stores the tweak on FontData, so the two Latin families need their own copy.
        let family_data = [font_data.clone(), font_data];
        for ((family, mut data), offset) in FAMILIES.into_iter().zip(family_data).zip(offsets) {
            data.tweak.y_offset_factor = offset;
            let name = cjk_font_name(coverage, &family);
            fonts.font_data.insert(name.clone(), Arc::new(data));
            // Keep Latin fonts first and preserve the locale's CJK fallback order.
            fonts.families.get_mut(&family).expect("default font family").push(name);
        }
    }
    fonts
}

fn font_y_offsets(data: &Arc<FontData>, coverage: FontCoverage) -> [f32; 2] {
    // Include the bytes and collection face: the same locale may switch from an OS font
    // to a bundled font. Cache only coefficients, not the large CJK font data.
    let key = (Sha256::digest(data.font.as_ref()).into(), data.index, coverage);
    let cache = ALIGNMENTS.get_or_init(Mutex::default);
    if let Some(offsets) = cache.lock().expect("font alignment cache").get(&key) {
        return *offsets;
    }
    let offsets = measure_y_offsets(data, coverage);
    cache.lock().expect("font alignment cache").insert(key, offsets);
    offsets
}

fn measure_y_offsets(data: &Arc<FontData>, coverage: FontCoverage) -> [f32; 2] {
    let name = "cjk_alignment_probe";
    let probe_family = FontFamily::Name(name.into());
    let mut definitions = FontDefinitions::default();
    definitions.font_data.insert(name.into(), Arc::clone(data));
    definitions.families.insert(probe_family.clone(), vec![name.into()]);
    for family in FAMILIES {
        definitions.families.get_mut(&family).unwrap().push(name.into());
    }
    let mut fonts = egui::epaint::Fonts::new(Default::default(), definitions);
    let sample = cjk_sample(coverage);
    if !fonts.has_glyphs(&FontId::new(ALIGNMENT_SIZE, probe_family), sample) {
        return [0.0; 2];
    }

    // Measure each face in isolation using egui's actual fallback layout. A larger
    // size reduces pixel-rounding error. A size-relative tweak follows UI zoom/DPI.
    // Shift the whole face, preserving punctuation and small-kana baseline positions.
    let mut view = fonts.with_pixels_per_point(1.0);
    FAMILIES.map(|family| {
        let id = FontId::new(ALIGNMENT_SIZE, family);
        let latin = view.layout_no_wrap(LATIN_SAMPLE.into(), id.clone(), Color32::WHITE);
        let cjk = view.layout_no_wrap(sample.into(), id, Color32::WHITE);
        if latin.mesh_bounds.is_positive() && cjk.mesh_bounds.is_positive() {
            let offset =
                (latin.mesh_bounds.center().y - cjk.mesh_bounds.center().y) / ALIGNMENT_SIZE;
            if offset.is_finite() {
                return offset;
            }
        }
        0.0
    })
}

fn cjk_sample(coverage: FontCoverage) -> &'static str {
    match coverage {
        FontCoverage::Japanese => "日本語",
        FontCoverage::Korean => "한글",
        FontCoverage::SimplifiedChinese => "汉语",
        FontCoverage::TraditionalChinese => "繁體",
        FontCoverage::HongKong => "嘅喺",
    }
}

fn cjk_font_name(coverage: FontCoverage, family: &FontFamily) -> String {
    let name = match coverage {
        FontCoverage::Japanese => "bmz_cjk_japanese",
        FontCoverage::Korean => "bmz_cjk_korean",
        FontCoverage::SimplifiedChinese => "bmz_cjk_simplified_chinese",
        FontCoverage::TraditionalChinese => "bmz_cjk_traditional_chinese",
        FontCoverage::HongKong => "bmz_cjk_hong_kong",
    };
    if *family == FontFamily::Monospace { format!("{name}_monospace") } else { name.into() }
}

#[cfg(test)]
mod tests;
