use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bmz_render::assets::load_png_rgba;
use bmz_render::plan::TextureId;
use bmz_render::renderer::Renderer;
use bmz_render::skin::{
    SkinContext, SkinDocument, SkinDocumentTexture, SkinManifest, SkinTextureId,
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

    for source in &document.source {
        if source.path.contains('*') || !source.path.to_ascii_lowercase().ends_with(".png") {
            tracing::debug!(
                source_id = %source.id,
                path = %source.path,
                "skipping unresolved beatoraja skin source"
            );
            continue;
        }
        let source_path = skin_root.join(&source.path);
        let asset = match load_png_rgba(&source_path) {
            Ok(asset) => asset,
            Err(error) => {
                tracing::warn!(
                    source_id = %source.id,
                    path = %source_path.display(),
                    %error,
                    "failed to load beatoraja skin source"
                );
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
}
