use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bmz_render::assets::load_png_rgba;
use bmz_render::plan::TextureId;
use bmz_render::renderer::Renderer;
use bmz_render::skin::{
    DestinationListEntry, SkinContext, SkinDocument, SkinDocumentTexture, SkinManifest,
    SkinTextureId,
};

pub fn apply_skin_from_dir(renderer: &mut Renderer, skin_root: &Path) -> Result<()> {
    let manifest_path = skin_root.join("skin.toml");
    let manifest = SkinManifest::load(&manifest_path)
        .with_context(|| format!("failed to load skin manifest: {}", manifest_path.display()))?
        .with_texture_source_sizes(skin_root);

    for texture in manifest.resolve_textures(skin_root) {
        renderer.load_png_texture(texture.id, &texture.path).with_context(|| {
            format!("failed to load skin texture {}: {}", texture.id.0, texture.path.display())
        })?;
        tracing::info!(
            texture_id = texture.id.0,
            path = %texture.path.display(),
            "loaded skin texture"
        );
    }
    renderer.set_skin_context(SkinContext::from_manifest(manifest));

    Ok(())
}

pub fn default_skin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/skins/default")
}

pub fn apply_default_skin(renderer: &mut Renderer) -> Result<()> {
    apply_skin_from_dir(renderer, &default_skin_root())
}

/// `profile.toml` の `[skin] play` 設定からスキンをロードする。
/// 空文字列 → デフォルトスキン、`.json` 拡張子 → beatoraja JSON スキン、
/// それ以外 → `skin.toml` を含む bmz スキンディレクトリとして扱う。
pub fn apply_skin_from_config(renderer: &mut Renderer, play_skin_path: &str) -> Result<()> {
    if play_skin_path.is_empty() {
        return apply_default_skin(renderer);
    }
    let path = Path::new(play_skin_path);
    if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        apply_beatoraja_json_skin(renderer, path)
    } else {
        apply_skin_from_dir(renderer, path)
    }
}

pub fn apply_beatoraja_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    let default_root = default_skin_root();
    let manifest_path = default_root.join("skin.toml");
    let manifest = SkinManifest::load(&manifest_path)
        .with_context(|| format!("failed to load skin manifest: {}", manifest_path.display()))?
        .with_texture_source_sizes(&default_root);

    for texture in manifest.resolve_textures(&default_root) {
        renderer.load_png_texture(texture.id, &texture.path).with_context(|| {
            format!(
                "failed to load default skin texture {}: {}",
                texture.id.0,
                texture.path.display()
            )
        })?;
    }

    let document = SkinDocument::load_beatoraja_json(skin_path)
        .with_context(|| format!("failed to load beatoraja json skin: {}", skin_path.display()))?;
    let skin_root = skin_path.parent().unwrap_or_else(|| Path::new("."));
    let mut document_textures = Vec::new();
    let mut next_texture_id = 10_000;
    let required_sources = required_skin_source_ids(&document);

    for font in &document.font {
        if font.id.is_empty() || font.path.is_empty() {
            continue;
        }
        let Some(font_path) = resolve_json_skin_asset_path(skin_root, &font.path, &document) else {
            tracing::debug!(
                font_id = %font.id,
                path = %font.path,
                "skipping unresolved beatoraja skin font"
            );
            continue;
        };
        if !is_supported_font_path(&font_path) {
            tracing::debug!(
                font_id = %font.id,
                path = %font_path.display(),
                "skipping unsupported beatoraja skin font"
            );
            continue;
        }
        let result = if is_bitmap_font_path(&font_path) {
            renderer.load_bitmap_font(font.id.clone(), &font_path)
        } else {
            renderer.load_font(font.id.clone(), &font_path)
        };
        if let Err(error) = result {
            tracing::warn!(
                font_id = %font.id,
                path = %font_path.display(),
                %error,
                "failed to load beatoraja skin font"
            );
        } else {
            tracing::info!(
                font_id = %font.id,
                path = %font_path.display(),
                "loaded beatoraja skin font"
            );
        }
    }

    for source in &document.source {
        let Some(source_path) = resolve_json_skin_source_path(skin_root, &source.path, &document)
        else {
            tracing::debug!(
                source_id = %source.id,
                path = %source.path,
                "skipping unresolved beatoraja skin source"
            );
            continue;
        };
        if !source_path.to_string_lossy().to_ascii_lowercase().ends_with(".png") {
            tracing::debug!(
                source_id = %source.id,
                path = %source_path.display(),
                "skipping unsupported beatoraja skin source"
            );
            continue;
        }
        let asset = match load_png_rgba(&source_path) {
            Ok(asset) => asset,
            Err(error) => {
                if required_sources.contains(source.id.as_str()) {
                    tracing::warn!(
                        source_id = %source.id,
                        path = %source_path.display(),
                        %error,
                        "failed to load beatoraja skin source"
                    );
                } else {
                    tracing::debug!(
                        source_id = %source.id,
                        path = %source_path.display(),
                        %error,
                        "skipping unused missing beatoraja skin source"
                    );
                }
                continue;
            }
        };
        let texture = SkinTextureId(next_texture_id);
        next_texture_id += 1;
        renderer
            .upsert_image_asset(TextureId(texture.0), &asset)
            .with_context(|| format!("failed to upload skin source: {}", source_path.display()))?;
        document_textures.push(SkinDocumentTexture {
            source_id: source.id.clone(),
            texture,
            source_size: bmz_render::skin::SkinImageSize {
                width: asset.width as f32,
                height: asset.height as f32,
            },
        });
        tracing::info!(
            source_id = %source.id,
            texture_id = texture.0,
            path = %source_path.display(),
            "loaded beatoraja skin source"
        );
    }

    renderer.set_skin_context(SkinContext::from_manifest_and_document(
        manifest,
        document,
        document_textures,
    ));
    Ok(())
}

fn is_supported_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("ttf" | "otf" | "ttc" | "fnt")
    )
}

fn is_bitmap_font_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("fnt")
    )
}

fn resolve_json_skin_source_path(
    skin_root: &Path,
    source_path: &str,
    document: &SkinDocument,
) -> Option<PathBuf> {
    resolve_json_skin_asset_path(skin_root, source_path, document)
}

fn resolve_json_skin_asset_path(
    skin_root: &Path,
    asset_path: &str,
    document: &SkinDocument,
) -> Option<PathBuf> {
    let normalized = asset_path.replace('\\', "/");
    if !normalized.contains('*') {
        return Some(skin_root.join(normalized));
    }

    let preferred = document
        .filepath
        .iter()
        .find(|filepath| filepath.path.replace('\\', "/") == normalized)
        .and_then(|filepath| (!filepath.def.is_empty()).then_some(filepath.def.as_str()));
    resolve_wildcard_path(skin_root, &normalized, preferred)
}

fn resolve_wildcard_path(
    skin_root: &Path,
    pattern: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    let star = pattern.find('*')?;
    let (prefix, suffix_with_star) = pattern.split_at(star);
    let suffix = &suffix_with_star[1..];
    let slash = prefix.rfind('/').map(|index| index + 1).unwrap_or(0);
    let (directory, filename_prefix) = prefix.split_at(slash);
    let directory = skin_root.join(directory);

    if let Some(suffix) = suffix.strip_prefix('/') {
        return resolve_wildcard_directory_path(&directory, filename_prefix, suffix, preferred);
    }

    let mut candidates = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            file_name.starts_with(filename_prefix) && file_name.ends_with(suffix)
        })
        .collect::<Vec<_>>();
    candidates.sort();

    if let Some(preferred) = preferred
        && let Some(candidate) = candidates.iter().find(|path| {
            let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            let stem = path.file_stem().and_then(|name| name.to_str()).unwrap_or_default();
            file_name == preferred || stem == preferred
        })
    {
        return Some(candidate.clone());
    }

    candidates.into_iter().next()
}

fn required_skin_source_ids(document: &SkinDocument) -> HashSet<&str> {
    let destination_ids = destination_ids(document);
    let image_sources = document
        .image
        .iter()
        .map(|image| (image.id.as_str(), image.src.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut required = HashSet::new();

    for image in &document.image {
        if destination_ids.contains(image.id.as_str()) {
            required.insert(image.src.as_str());
        }
    }
    for imageset in &document.imageset {
        if destination_ids.contains(imageset.id.as_str()) {
            for image_id in &imageset.images {
                if let Some(source_id) = image_sources.get(image_id.as_str()) {
                    required.insert(*source_id);
                }
            }
        }
    }
    for value in &document.value {
        if destination_ids.contains(value.id.as_str()) {
            required.insert(value.src.as_str());
        }
    }
    for slider in &document.slider {
        if destination_ids.contains(slider.id.as_str()) {
            required.insert(slider.src.as_str());
        }
    }
    for graph in &document.graph {
        if destination_ids.contains(graph.id.as_str()) {
            required.insert(graph.src.as_str());
        }
    }
    for cover in &document.hidden_cover {
        if destination_ids.contains(cover.id.as_str()) {
            required.insert(cover.src.as_str());
        }
    }
    if let Some(note) = &document.note {
        for image_id in note
            .note
            .iter()
            .chain(note.lnstart.iter())
            .chain(note.lnend.iter())
            .chain(note.lnbody.iter())
            .chain(note.lnactive.iter())
            .chain(note.hcnstart.iter())
            .chain(note.hcnend.iter())
            .chain(note.hcnbody.iter())
            .chain(note.hcnactive.iter())
            .chain(note.hcndamage.iter())
            .chain(note.hcnreactive.iter())
            .chain(note.mine.iter())
            .chain(note.hidden.iter())
            .chain(note.processed.iter())
        {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    if let Some(gauge) = &document.gauge {
        for image_id in &gauge.nodes {
            if let Some(source_id) = image_sources.get(image_id.as_str()) {
                required.insert(*source_id);
            }
        }
    }
    for judge in &document.judge {
        for destination in judge.images.iter().chain(judge.numbers.iter()) {
            if let Some(source_id) = image_sources.get(destination.id.as_str()) {
                required.insert(*source_id);
            }
            if let Some(value) = document.value.iter().find(|value| value.id == destination.id) {
                required.insert(value.src.as_str());
            }
        }
    }

    required
}

fn destination_ids(document: &SkinDocument) -> HashSet<&str> {
    let mut ids = HashSet::new();
    for entry in &document.destination {
        match entry {
            DestinationListEntry::Single(destination) => {
                ids.insert(destination.id.as_str());
            }
            DestinationListEntry::Conditional { destinations, .. } => {
                for destination in destinations {
                    ids.insert(destination.id.as_str());
                }
            }
        }
    }
    ids
}

fn resolve_wildcard_directory_path(
    directory: &Path,
    directory_prefix: &str,
    suffix: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    let mut candidates = std::fs::read_dir(directory)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(directory_prefix))
        })
        .map(|path| path.join(suffix))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    candidates.sort();

    if let Some(preferred) = preferred
        && let Some(candidate) = candidates.iter().find(|path| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == preferred)
        })
    {
        return Some(candidate.clone());
    }

    candidates.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_render::renderer::Renderer;

    #[test]
    fn default_skin_root_contains_manifest() {
        assert!(default_skin_root().join("skin.toml").is_file());
    }

    #[test]
    fn default_skin_can_be_applied_to_renderer() {
        let mut renderer = Renderer::default();

        apply_default_skin(&mut renderer).unwrap();
    }

    #[test]
    fn beatoraja_default_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/play7.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn apply_skin_from_config_empty_path_uses_default_skin() {
        let mut renderer = Renderer::default();

        apply_skin_from_config(&mut renderer, "").unwrap();
    }

    #[test]
    fn apply_skin_from_config_json_path_loads_beatoraja_skin_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/play7.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_skin_from_config(&mut renderer, skin_path.to_str().unwrap()).unwrap();
    }

    #[test]
    fn wildcard_skin_source_prefers_filepath_default() {
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/default.png"), []).unwrap();
        std::fs::write(root.join("parts/blue.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    { "name": "Parts", "path": "parts/*.png", "def": "blue" }
                ]
            }
            "#,
        )
        .unwrap();

        let resolved = resolve_json_skin_source_path(&root, "parts/*.png", &document).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
    }

    #[test]
    fn wildcard_skin_source_falls_back_to_first_match() {
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/b.png"), []).unwrap();
        std::fs::write(root.join("parts/a.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let resolved = resolve_json_skin_source_path(&root, "parts/*.png", &document).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("a.png"));
    }

    #[test]
    fn wildcard_skin_font_resolves_nested_file() {
        let root = unique_test_dir("bmz-json-font");
        std::fs::create_dir_all(root.join("frame/SP/Default")).unwrap();
        std::fs::write(root.join("frame/SP/Default/song.fnt"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let resolved =
            resolve_json_skin_asset_path(&root, "frame/SP/*/song.fnt", &document).unwrap();

        assert_eq!(resolved.strip_prefix(&root).unwrap(), Path::new("frame/SP/Default/song.fnt"));
    }

    #[test]
    fn required_skin_sources_excludes_unused_images() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "source": [
                    { "id": 1, "path": "used.png" },
                    { "id": 2, "path": "unused.png" }
                ],
                "image": [
                    { "id": "used", "src": 1, "x": 0, "y": 0, "w": 8, "h": 8 },
                    { "id": "unused", "src": 2, "x": 0, "y": 0, "w": 8, "h": 8 }
                ],
                "destination": [
                    { "id": "used", "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let required = required_skin_source_ids(&document);

        assert!(required.contains("1"));
        assert!(!required.contains("2"));
    }

    #[test]
    fn supported_font_paths_include_vector_and_bitmap_fonts() {
        assert!(is_supported_font_path(Path::new("font.ttf")));
        assert!(is_supported_font_path(Path::new("font.OTF")));
        assert!(is_supported_font_path(Path::new("font.ttc")));
        assert!(is_supported_font_path(Path::new("font.fnt")));
        assert!(!is_supported_font_path(Path::new("font.png")));
        assert!(is_bitmap_font_path(Path::new("font.fnt")));
        assert!(!is_bitmap_font_path(Path::new("font.ttf")));
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        path
    }
}
