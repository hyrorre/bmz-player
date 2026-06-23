use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::assets::{RgbaImageAsset, load_png_rgba};

#[derive(Debug, Clone, PartialEq)]
pub struct BitmapFont {
    pub size: i32,
    pub line_height: i32,
    pub base: i32,
    pub ascent: f32,
    pub scale_width: u32,
    pub scale_height: u32,
    pub pages: HashMap<i32, BitmapFontPage>,
    pub glyphs: HashMap<char, BitmapFontGlyph>,
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
    let text = String::from_utf8_lossy(&bytes);
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    parse_bitmap_font(&text, base_dir)
        .with_context(|| format!("failed to parse bitmap font: {}", path.display()))
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
        let image = load_png_rgba(&path)
            .with_context(|| format!("failed to load bitmap font page: {}", path.display()))?;
        pages.insert(id, BitmapFontPage { id, path, image });
    }

    Ok(BitmapFont { size, line_height, base, ascent, scale_width, scale_height, pages, glyphs })
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
}
