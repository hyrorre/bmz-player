use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use encoding_rs::SHIFT_JIS;

use crate::assets::{RgbaImageAsset, load_static_rgba_image};

#[derive(Debug, Clone, PartialEq)]
pub struct BitmapFont {
    pub size: i32,
    pub line_height: i32,
    pub base: i32,
    pub ascent: f32,
    pub scale_width: u32,
    pub scale_height: u32,
    // Fonts are shared by the decoded cache and renderer. Large CIM page sets
    // must not duplicate their pixels when a cached font is installed again.
    pub pages: Arc<HashMap<i32, BitmapFontPage>>,
    pub glyphs: Arc<HashMap<char, BitmapFontGlyph>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitmapFontPage {
    pub id: i32,
    pub path: PathBuf,
    pub image: RgbaImageAsset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitmapFontGlyph {
    pub id: char,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub xoffset: i32,
    pub yoffset: i32,
    pub xadvance: i32,
    pub page: i32,
}

pub fn load_bitmap_font(path: &Path) -> Result<BitmapFont> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to read bitmap font: {}", path.display()))?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("lr2font"))
    {
        let (text, _, _) = SHIFT_JIS.decode(&bytes);
        parse_lr2_bitmap_font(&text, base_dir)
    } else {
        parse_bitmap_font(&String::from_utf8_lossy(&bytes), base_dir)
    }
    .with_context(|| format!("failed to parse bitmap font: {}", path.display()))
}

/// Resolve page dependencies without decoding their pixels, for cache validation.
pub fn bitmap_font_page_paths(path: &Path) -> Result<Vec<PathBuf>> {
    let bytes = std::fs::read(path)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let lr2 = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2font"));
    let text = if lr2 { SHIFT_JIS.decode(&bytes).0 } else { String::from_utf8_lossy(&bytes) };
    let mut paths = HashMap::new();
    for line in text.lines().map(str::trim) {
        if lr2 {
            let fields =
                line.trim_start_matches('\u{feff}').split(',').map(str::trim).collect::<Vec<_>>();
            if fields.first().is_some_and(|command| command.eq_ignore_ascii_case("#T"))
                && let Some((id, path)) = lr2_page_path(&fields, base_dir)
            {
                paths.insert(id, path);
            }
        } else if line.starts_with("page ") {
            let fields = parse_fields(line);
            let id = parse_i32(&fields, "id")?;
            let file =
                fields.get("file").ok_or_else(|| anyhow!("bitmap font page missing file"))?;
            paths
                .insert(id, resolve_case_insensitive_path(&base_dir.join(file.replace('\\', "/"))));
        }
    }
    let mut paths: Vec<_> = paths.into_values().collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn lr2_page_path(fields: &[&str], base_dir: &Path) -> Option<(i32, PathBuf)> {
    let page = parse_lr2_font_i32(fields.get(1).copied())?;
    let file = fields.get(2).filter(|file| !file.is_empty())?;
    Some((
        page,
        resolve_case_insensitive_path(&base_dir.join(file.trim_matches('"').replace('\\', "/"))),
    ))
}

fn parse_lr2_bitmap_font(text: &str, base_dir: &Path) -> Result<BitmapFont> {
    let mut size = 0;
    let mut margin = 0;
    let mut page_paths = HashMap::new();
    let mut glyphs = HashMap::new();

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let line = line.trim_start_matches('\u{feff}');
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        let command = fields.first().and_then(|field| field.strip_prefix('#')).unwrap_or_default();
        match command.to_ascii_uppercase().as_str() {
            "S" => {
                if let Some(value) = parse_lr2_font_i32(fields.get(1).copied()) {
                    size = value;
                }
            }
            "M" => {
                if let Some(value) = parse_lr2_font_i32(fields.get(1).copied()) {
                    margin = value;
                }
            }
            "T" => {
                let Some((page, path)) = lr2_page_path(&fields, base_dir) else {
                    continue;
                };
                if path.is_file() {
                    page_paths.insert(page, path);
                }
            }
            "R" => {
                let values = (1..=6)
                    .map(|index| parse_lr2_font_i32(fields.get(index).copied()).unwrap_or_default())
                    .collect::<Vec<_>>();
                let [code, page, x, y, width, height] = values.as_slice() else {
                    continue;
                };
                if !page_paths.contains_key(page) || *x < 0 || *y < 0 || *width < 0 || *height < 0 {
                    continue;
                }
                for ch in lr2_font_chars(*code) {
                    glyphs.insert(
                        ch,
                        BitmapFontGlyph {
                            id: ch,
                            x: *x as u32,
                            y: *y as u32,
                            width: *width as u32,
                            height: *height as u32,
                            xoffset: 0,
                            yoffset: 0,
                            xadvance: width.saturating_add(margin),
                            page: *page,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    if size <= 0 {
        bail!("LR2 bitmap font size must be positive");
    }
    if page_paths.is_empty() {
        bail!("LR2 bitmap font has no pages");
    }
    let line_height = glyphs
        .values()
        .map(|glyph| glyph.height as i32)
        .max()
        .filter(|height| *height > 0)
        .ok_or_else(|| anyhow!("LR2 bitmap font has no glyphs"))?;
    let base = line_height;
    let ascent = base as f32 - bitmap_font_cap_height(&glyphs, 0);

    let mut pages = HashMap::new();
    let mut scale_width = 0;
    let mut scale_height = 0;
    for (id, path) in page_paths {
        let image = load_static_rgba_image(&path)
            .with_context(|| format!("failed to load bitmap font page: {}", path.display()))?;
        scale_width = scale_width.max(image.width);
        scale_height = scale_height.max(image.height);
        pages.insert(id, BitmapFontPage { id, path, image });
    }

    Ok(BitmapFont {
        size,
        line_height,
        base,
        ascent,
        scale_width,
        scale_height,
        pages: pages.into(),
        glyphs: glyphs.into(),
    })
}

fn parse_lr2_font_i32(value: Option<&str>) -> Option<i32> {
    value?.replace('!', "-").replace(' ', "").parse().ok()
}

fn lr2_font_chars(code: i32) -> Vec<char> {
    if code == 288 {
        return vec!['\u{301c}', '\u{ff5e}'];
    }
    let bytes = if code >= 8127 {
        (code.wrapping_add(49281) as u16).to_be_bytes().to_vec()
    } else if code >= 256 {
        (code.wrapping_add(32832) as u16).to_be_bytes().to_vec()
    } else if code >= 0 {
        vec![code as u8]
    } else {
        return Vec::new();
    };
    let (decoded, had_errors) = SHIFT_JIS.decode_without_bom_handling(&bytes);
    if had_errors { Vec::new() } else { decoded.chars().collect() }
}

fn parse_bitmap_font(text: &str, base_dir: &Path) -> Result<BitmapFont> {
    let mut size = 0;
    let mut pad_y = 0;
    let mut line_height = 0;
    let mut base = 0;
    let mut scale_width = 0;
    let mut scale_height = 0;
    let mut page_paths = HashMap::new();
    let mut glyphs = HashMap::new();

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let fields = parse_fields(line);
        if line.starts_with("info ") {
            size = parse_i32(&fields, "size").unwrap_or_default().unsigned_abs() as i32;
            pad_y = parse_padding_y(&fields).unwrap_or_default();
        } else if line.starts_with("common ") {
            line_height = parse_i32(&fields, "lineHeight")?;
            base = parse_i32(&fields, "base").unwrap_or(line_height);
            scale_width = parse_i32(&fields, "scaleW")?.max(1) as u32;
            scale_height = parse_i32(&fields, "scaleH")?.max(1) as u32;
        } else if line.starts_with("page ") {
            let id = parse_i32(&fields, "id")?;
            let file =
                fields.get("file").ok_or_else(|| anyhow!("bitmap font page missing file"))?;
            page_paths
                .insert(id, resolve_case_insensitive_path(&base_dir.join(file.replace('\\', "/"))));
        } else if line.starts_with("char ") {
            let id = parse_i32(&fields, "id")?;
            let Some(ch) = char::from_u32(id as u32) else {
                continue;
            };
            let glyph = BitmapFontGlyph {
                id: ch,
                x: parse_i32(&fields, "x")?.max(0) as u32,
                y: parse_i32(&fields, "y")?.max(0) as u32,
                width: parse_i32(&fields, "width")?.max(0) as u32,
                height: parse_i32(&fields, "height")?.max(0) as u32,
                xoffset: parse_i32(&fields, "xoffset")?,
                yoffset: parse_i32(&fields, "yoffset")?,
                xadvance: parse_i32(&fields, "xadvance")?,
                page: parse_i32(&fields, "page")?,
            };
            glyphs.insert(ch, glyph);
        }
    }

    if page_paths.is_empty() {
        bail!("bitmap font has no pages");
    }
    if line_height <= 0 {
        bail!("bitmap font lineHeight must be positive");
    }

    let ascent = base as f32 - bitmap_font_cap_height(&glyphs, pad_y);

    let mut pages = HashMap::new();
    for (id, path) in page_paths {
        let image = load_static_rgba_image(&path)
            .with_context(|| format!("failed to load bitmap font page: {}", path.display()))?;
        pages.insert(id, BitmapFontPage { id, path, image });
    }

    Ok(BitmapFont {
        size,
        line_height,
        base,
        ascent,
        scale_width,
        scale_height,
        pages: pages.into(),
        glyphs: glyphs.into(),
    })
}

fn resolve_case_insensitive_path(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };
    let Some(parent) = path.parent() else {
        return path.to_path_buf();
    };
    let parent = resolve_case_insensitive_path(parent);
    let Ok(entries) = std::fs::read_dir(&parent) else {
        return parent.join(file_name);
    };
    entries
        .filter_map(|entry| entry.ok())
        .find(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(file_name))
        })
        .map(|entry| entry.path())
        .unwrap_or_else(|| parent.join(file_name))
}

fn parse_fields(line: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for token in split_fnt_tokens(line).into_iter().skip(1) {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        fields.insert(key.to_string(), value.trim_matches('"').to_string());
    }
    fields
}

fn split_fnt_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut in_quote = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                token.push(ch);
            }
            ' ' | '\t' if !in_quote => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            _ => token.push(ch),
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

fn parse_i32(fields: &HashMap<String, String>, key: &str) -> Result<i32> {
    let value = fields.get(key).ok_or_else(|| anyhow!("bitmap font missing {key}"))?;
    value.parse::<i32>().with_context(|| format!("invalid bitmap font {key}: {value}"))
}

fn parse_padding_y(fields: &HashMap<String, String>) -> Result<i32> {
    let Some(value) = fields.get("padding") else {
        return Ok(0);
    };
    let parts = value
        .split(',')
        .map(|part| part.parse::<i32>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("invalid bitmap font padding: {value}"))?;
    if parts.len() != 4 {
        bail!("invalid bitmap font padding: {value}");
    }
    Ok(parts[0] + parts[2])
}

fn bitmap_font_cap_height(glyphs: &HashMap<char, BitmapFontGlyph>, pad_y: i32) -> f32 {
    const CAP_CHARS: [char; 26] = [
        'M', 'N', 'B', 'D', 'C', 'E', 'F', 'K', 'A', 'G', 'H', 'I', 'J', 'L', 'O', 'P', 'Q', 'R',
        'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
    ];
    let cap_height = CAP_CHARS
        .iter()
        .find_map(|ch| glyphs.get(ch).filter(|glyph| glyph.width > 0 && glyph.height > 0))
        .map(|glyph| glyph.height as i32)
        .unwrap_or_else(|| {
            glyphs
                .values()
                .filter(|glyph| glyph.width > 0 && glyph.height > 0)
                .map(|glyph| glyph.height as i32)
                .max()
                .unwrap_or(1)
        });
    (cap_height - pad_y) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_font_loads_pages_and_glyphs() {
        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        write_test_png(&root.join("font page.png"));
        std::fs::write(
            root.join("font.fnt"),
            r#"info face="test" size=16 padding=0,0,0,0
common lineHeight=20 base=15 scaleW=2 scaleH=2 pages=1 packed=0
page id=0 file="font page.png"
chars count=1
char id=65 x=0 y=0 width=1 height=1 xoffset=1 yoffset=2 xadvance=9 page=0 chnl=0
"#,
        )
        .unwrap();

        let font = load_bitmap_font(&root.join("font.fnt")).unwrap();

        assert_eq!(font.line_height, 20);
        assert_eq!(font.size, 16);
        assert_eq!(font.base, 15);
        assert_eq!(font.ascent, 14.0);
        assert_eq!(font.pages[&0].image.width, 2);
        assert_eq!(font.glyphs[&'A'].xadvance, 9);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bitmap_font_loads_cim_pages_and_shares_decoded_data() {
        use std::io::Write;

        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        let colors = [[12, 34, 56, 78], [90, 123, 200, 255]];
        for (id, color) in colors.iter().enumerate() {
            let file = std::fs::File::create(root.join(format!("page{id}.cim"))).unwrap();
            let mut encoder = flate2::write::ZlibEncoder::new(file, flate2::Compression::default());
            for value in [1_i32, 1, 4] {
                encoder.write_all(&value.to_be_bytes()).unwrap();
            }
            encoder.write_all(color).unwrap();
            encoder.finish().unwrap();
        }
        let path = root.join("font.fnt");
        std::fs::write(
            &path,
            r#"info face="test" size=16 padding=0,0,0,0
common lineHeight=20 base=15 scaleW=1 scaleH=1 pages=2 packed=0
page id=0 file="page0.cim"
page id=1 file="page1.cim"
char id=65 x=0 y=0 width=1 height=1 xoffset=1 yoffset=2 xadvance=9 page=0 chnl=0
char id=66 x=0 y=0 width=1 height=1 xoffset=3 yoffset=4 xadvance=11 page=1 chnl=0
"#,
        )
        .unwrap();
        let font = load_bitmap_font(&path).unwrap();
        assert_eq!(font.pages[&0].image.pixels, colors[0]);
        assert_eq!(font.pages[&1].image.pixels, colors[1]);
        assert_eq!(font.glyphs[&'B'].page, 1);
        assert_eq!(font.glyphs[&'B'].xadvance, 11);
        assert_eq!((font.size, font.base, font.ascent), (16, 15, 14.0));
        assert_eq!(
            bitmap_font_page_paths(&path).unwrap(),
            vec![root.join("page0.cim"), root.join("page1.cim")]
        );
        let cached = font.clone();
        assert!(Arc::ptr_eq(&font.pages, &cached.pages));
        assert!(Arc::ptr_eq(&font.glyphs, &cached.glyphs));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bitmap_font_page_paths_resolve_case_insensitively() {
        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        write_test_png(&root.join("Artist1.png"));
        std::fs::write(
            root.join("artist.fnt"),
            r#"info face="test" size=16 padding=0,0,0,0
common lineHeight=20 base=15 scaleW=2 scaleH=2 pages=1 packed=0
page id=0 file="artist1.png"
chars count=1
char id=65 x=0 y=0 width=1 height=1 xoffset=1 yoffset=2 xadvance=9 page=0 chnl=0
"#,
        )
        .unwrap();

        let font = load_bitmap_font(&root.join("artist.fnt")).unwrap();

        assert_eq!(font.pages[&0].path, root.join("Artist1.png"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bitmap_font_loads_tga_pages() {
        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        write_test_tga(&root.join("font_00.tga"));
        std::fs::write(
            root.join("font.fnt"),
            r#"info face="test" size=16 padding=0,0,0,0
common lineHeight=20 base=15 scaleW=2 scaleH=2 pages=1 packed=0
page id=0 file="font_00.tga"
chars count=1
char id=65 x=0 y=0 width=1 height=1 xoffset=1 yoffset=2 xadvance=9 page=0 chnl=0
"#,
        )
        .unwrap();

        let font = load_bitmap_font(&root.join("font.fnt")).unwrap();

        assert_eq!(font.pages[&0].image.width, 2);
        assert_eq!(font.pages[&0].image.height, 2);
        assert_eq!(font.glyphs[&'A'].xadvance, 9);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lr2_bitmap_font_loads_shift_jis_definition_and_character_map() {
        let root = temp_dir();
        std::fs::create_dir_all(&root).unwrap();
        let page_path = root.join("ページ.TGA");
        write_test_tga(&page_path);
        let definition = r#"// LR2 font
#S,4,
#M,2,
#T,0,ページ.tga
#R,65,0,0,0,1,2,
#R,256,0,0,0,1,2,
#R,288,0,0,0,1,2,
#R,8127,0,0,0,1,2,
"#;
        let (encoded, _, had_errors) = SHIFT_JIS.encode(definition);
        assert!(!had_errors);
        std::fs::write(root.join("font.lr2font"), encoded.as_ref()).unwrap();

        let font = load_bitmap_font(&root.join("font.lr2font")).unwrap();

        assert_eq!(font.size, 4);
        assert_eq!(font.line_height, 2);
        assert_eq!(font.base, 2);
        assert_eq!(font.ascent, 0.0);
        assert_eq!(font.scale_width, 2);
        assert_eq!(font.scale_height, 2);
        assert_eq!(font.pages[&0].path, page_path);
        assert_eq!(font.glyphs[&'A'].xadvance, 3);
        assert!(font.glyphs.contains_key(&'\u{3000}'));
        assert!(font.glyphs.contains_key(&'\u{301c}'));
        assert!(font.glyphs.contains_key(&'\u{ff5e}'));
        assert!(font.glyphs.contains_key(&'\u{6f3e}'));
        std::fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir() -> PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("bmz-bitmap-font-test-{stamp}"))
    }

    fn write_test_png(path: &Path) {
        let buffer = image::RgbaImage::from_raw(
            2,
            2,
            vec![255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 128],
        )
        .expect("rgba buffer dimensions match pixels");
        buffer.save_with_format(path, image::ImageFormat::Png).unwrap();
    }

    fn write_test_tga(path: &Path) {
        let buffer = image::RgbaImage::from_raw(
            2,
            2,
            vec![255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 128],
        )
        .expect("rgba buffer dimensions match pixels");
        buffer.save_with_format(path, image::ImageFormat::Tga).unwrap();
    }
}
