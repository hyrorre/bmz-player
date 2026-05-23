use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bmz_render::assets::{RgbaImageAsset, load_png_rgba};
use bmz_render::bitmap_font::{BitmapFont, load_bitmap_font};
use bmz_render::plan::TextureId;
use bmz_render::renderer::Renderer;
use bmz_render::skin::{
    DestinationListEntry, SkinContext, SkinDocument, SkinDocumentTexture, SkinFilepathDef,
    SkinImageSize, SkinManifest, SkinTextureId,
};
use bmz_skin::SkinKind as DecodeSkinKind;
use rayon::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinKind {
    Play,
    Select,
    Result,
}

impl SkinKind {
    fn first_texture_id(self) -> u32 {
        match self {
            SkinKind::Play => 10_000,
            SkinKind::Select => 20_000,
            SkinKind::Result => 30_000,
        }
    }

    fn warn_missing_required_sources(self) -> bool {
        matches!(self, SkinKind::Play)
    }

    fn font_namespace(self) -> &'static str {
        match self {
            SkinKind::Play => "play",
            SkinKind::Select => "select",
            SkinKind::Result => "result",
        }
    }
}

/// バックグラウンドスレッドでデコード可能な 1 スキンぶんの中間データ。
/// Renderer に触らず Send-safe な値だけを保持する。
pub struct DecodedSkin {
    pub kind: SkinKind,
    pub document: SkinDocument,
    pub fonts: Vec<DecodedFont>,
    pub sources: Vec<DecodedSource>,
}

pub struct DecodedFont {
    pub stored_id: String,
    pub path: PathBuf,
    pub data: DecodedFontData,
}

pub enum DecodedFontData {
    Vector(Vec<u8>),
    Bitmap(BitmapFont),
}

pub struct DecodedSource {
    pub source_id: String,
    pub path: PathBuf,
    pub texture: SkinTextureId,
    pub asset: RgbaImageAsset,
}

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
    renderer.set_play_skin_context(SkinContext::from_manifest(manifest));

    Ok(())
}

pub fn default_skin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/skins/default")
}

pub fn apply_default_skin(renderer: &mut Renderer) -> Result<()> {
    apply_skin_from_dir(renderer, &default_skin_root())
}

/// `profile.toml` の `[skin] play` 設定からスキンをロードする。
/// 空文字列 → デフォルトスキン、`.json`/`.luaskin`/`.lua` 拡張子 → beatoraja スキン、
/// それ以外 → `skin.toml` を含む bmz スキンディレクトリとして扱う。
pub fn apply_skin_from_config(renderer: &mut Renderer, play_skin_path: &str) -> Result<()> {
    if play_skin_path.is_empty() {
        return apply_default_skin(renderer);
    }
    let path = Path::new(play_skin_path);
    if is_decodable_skin_path(path) {
        apply_beatoraja_json_skin(renderer, path)
    } else {
        apply_skin_from_dir(renderer, path)
    }
}

pub fn apply_beatoraja_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Play)
}

pub fn apply_beatoraja_select_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Select)
}

pub fn apply_beatoraja_result_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Result)
}

fn apply_beatoraja_json_skin_for_kind(
    renderer: &mut Renderer,
    skin_path: &Path,
    kind: SkinKind,
) -> Result<()> {
    let manifest = load_default_skin_into_renderer(renderer)?;
    let decoded = decode_beatoraja_skin(skin_path, kind)?;
    install_decoded_skin(renderer, decoded, manifest)
}

/// デフォルトスキンの manifest と PNG テクスチャを renderer に取り込む。
/// 起動時に 1 回だけ呼ばれることを想定 (同じテクスチャを複数回 upsert しても害は無いが無駄)。
pub fn load_default_skin_into_renderer(renderer: &mut Renderer) -> Result<SkinManifest> {
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
    Ok(manifest)
}

/// beatoraja JSON skin の document/フォント/PNG ソースを並列にデコードする。
/// Renderer には触らないので Send-safe で、別スレッドからも呼べる。
pub fn decode_beatoraja_skin(skin_path: &Path, kind: SkinKind) -> Result<DecodedSkin> {
    decode_beatoraja_skin_with_options(skin_path, kind, &BTreeMap::new(), &BTreeMap::new())
}

/// `decode_beatoraja_skin` のカスタマイズオプション / ファイル選択付き版。
///
/// `options` はオプション名 -> 選択肢名の対応。JSON スキンは選択肢の `op`
/// コード列へ、Lua スキンはそのまま渡して展開する。
///
/// `files` は filepath 定義名 -> 選択ファイルのスキンルート相対パスの対応。
/// Lua スキンは `skin_config.get_path` の解決へ、JSON スキンは `source` /
/// `font` のワイルドカード解決へ反映する。
pub fn decode_beatoraja_skin_with_options(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<DecodedSkin> {
    let mut document = load_skin_document(skin_path, kind, options, files)?;
    // フォント ID は scene 横断的に Renderer のグローバルマップに登録されるので、
    // play / select / result で同じ "0" 等が衝突する。namespace を付与して隔離する。
    // text 定義の font 参照側も同じ namespace を付ける。
    let font_namespace = kind.font_namespace();
    for text in &mut document.text {
        if !text.font.is_empty() {
            text.font = format!("{}:{}", font_namespace, text.font);
        }
    }
    let skin_root = skin_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let required_sources: HashSet<String> =
        required_skin_source_ids(&document).into_iter().map(str::to_string).collect();
    let warn_missing_required = kind.warn_missing_required_sources();

    // フォントを並列にデコードする。
    let font_tasks: Vec<_> = document
        .font
        .iter()
        .filter_map(|font| {
            if font.id.is_empty() || font.path.is_empty() {
                return None;
            }
            let font_path = resolve_json_skin_asset_path(&skin_root, &font.path, &document, files)?;
            if !is_supported_font_path(&font_path) {
                tracing::debug!(
                    font_id = %font.id,
                    path = %font_path.display(),
                    "skipping unsupported beatoraja skin font"
                );
                return None;
            }
            let stored_id = format!("{}:{}", font_namespace, font.id);
            Some((stored_id, font_path))
        })
        .collect();

    let fonts: Vec<DecodedFont> = font_tasks
        .into_par_iter()
        .filter_map(|(stored_id, font_path)| match decode_font(&font_path) {
            Ok(data) => Some(DecodedFont { stored_id, path: font_path, data }),
            Err(error) => {
                tracing::warn!(
                    font_id = %stored_id,
                    path = %font_path.display(),
                    %error,
                    "failed to load beatoraja skin font"
                );
                None
            }
        })
        .collect();

    // ソースは ID 順を保つため、まず resolved path リストを順次組み立て、
    // PNG デコード本体だけを並列実行する。
    let source_tasks: Vec<(usize, String, PathBuf)> = document
        .source
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            let source_path =
                resolve_json_skin_source_path(&skin_root, &source.path, &document, files)?;
            if !source_path.to_string_lossy().to_ascii_lowercase().ends_with(".png") {
                tracing::debug!(
                    source_id = %source.id,
                    path = %source_path.display(),
                    "skipping unsupported beatoraja skin source"
                );
                return None;
            }
            Some((index, source.id.clone(), source_path))
        })
        .collect();

    let mut decoded_pairs: Vec<(usize, String, PathBuf, RgbaImageAsset)> = source_tasks
        .into_par_iter()
        .filter_map(|(index, source_id, source_path)| match load_png_rgba(&source_path) {
            Ok(asset) => Some((index, source_id, source_path, asset)),
            Err(error) => {
                if warn_missing_required && required_sources.contains(&source_id) {
                    tracing::warn!(
                        source_id = %source_id,
                        path = %source_path.display(),
                        %error,
                        "failed to load beatoraja skin source"
                    );
                } else {
                    tracing::debug!(
                        source_id = %source_id,
                        path = %source_path.display(),
                        %error,
                        "skipping unused missing beatoraja skin source"
                    );
                }
                None
            }
        })
        .collect();
    decoded_pairs.sort_by_key(|(index, _, _, _)| *index);

    let mut next_texture_id = kind.first_texture_id();
    let sources: Vec<DecodedSource> = decoded_pairs
        .into_iter()
        .map(|(_, source_id, path, asset)| {
            let texture = SkinTextureId(next_texture_id);
            next_texture_id += 1;
            DecodedSource { source_id, path, texture, asset }
        })
        .collect();

    Ok(DecodedSkin { kind, document, fonts, sources })
}

fn load_skin_document(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<SkinDocument> {
    if is_lua_skin_path(skin_path) {
        // Lua スキンはオプション選択 (名前 -> 選択肢名) とファイル選択
        // (filepath 定義名 -> 相対パス) をそのまま渡す。
        let loaded = bmz_skin::load_lua_skin(skin_path, decode_skin_kind(kind), options, files)
            .with_context(|| format!("failed to load lua skin: {}", skin_path.display()))?;
        for warning in loaded.warnings {
            tracing::warn!(
                path = %skin_path.display(),
                kind = ?kind,
                warning = %warning.message,
                "lua skin load warning"
            );
        }
        Ok(loaded.document)
    } else {
        let document =
            bmz_skin::load_beatoraja_json_skin_with_defaults(skin_path).with_context(|| {
                format!("failed to load beatoraja json skin: {}", skin_path.display())
            })?;
        if options.is_empty() {
            return Ok(document);
        }
        // JSON スキンは property 定義から選択肢の op コード列を組み立て、
        // それを有効オプションとして再デコードする。
        let enabled = enabled_options_from_selections(&document, options);
        bmz_skin::load_beatoraja_json_skin(skin_path, &enabled).with_context(|| {
            format!("failed to load beatoraja json skin with options: {}", skin_path.display())
        })
    }
}

/// property 定義とユーザ選択 (オプション名 -> 選択肢名) から、JSON スキンの
/// 有効オプション (op コード列) を組み立てる。
///
/// 選択が無い property は `def` (空なら先頭 item) の op を使う。
fn enabled_options_from_selections(
    document: &SkinDocument,
    selections: &BTreeMap<String, String>,
) -> Vec<i32> {
    document
        .property
        .iter()
        .filter_map(|property| {
            let chosen = selections
                .get(&property.name)
                .and_then(|name| property.item.iter().find(|item| &item.name == name));
            let selected = chosen.or_else(|| {
                if property.def.is_empty() {
                    property.item.first()
                } else {
                    property.item.iter().find(|item| item.name == property.def)
                }
            });
            selected.map(|item| item.op)
        })
        .collect()
}

fn decode_skin_kind(kind: SkinKind) -> DecodeSkinKind {
    match kind {
        SkinKind::Play => DecodeSkinKind::Play,
        SkinKind::Select => DecodeSkinKind::Select,
        SkinKind::Result => DecodeSkinKind::Result,
    }
}

pub fn is_decodable_skin_path(path: &Path) -> bool {
    is_json_skin_path(path) || is_lua_skin_path(path)
}

pub fn is_json_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

pub fn is_lua_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("luaskin") || ext.eq_ignore_ascii_case("lua"))
}

fn decode_font(path: &Path) -> Result<DecodedFontData> {
    if is_bitmap_font_path(path) {
        Ok(DecodedFontData::Bitmap(load_bitmap_font(path)?))
    } else {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read font: {}", path.display()))?;
        Ok(DecodedFontData::Vector(bytes))
    }
}

/// Phase A でデコードした成果物を Renderer に取り込み、scene context を更新する。
/// `default_manifest` は `load_default_skin_into_renderer` で取得した値を渡す。
/// 一括 install するので、PNG/フォント数が多いと 1 フレーム分のコストになる。
/// 起動直後や同期パスではこちらを使うが、ランタイム中はフレーム分散する方が望ましい。
pub fn install_decoded_skin(
    renderer: &mut Renderer,
    decoded: DecodedSkin,
    default_manifest: SkinManifest,
) -> Result<()> {
    let DecodedSkin { kind, document, fonts, sources } = decoded;

    for font in fonts {
        install_decoded_font(renderer, font);
    }

    let document_textures: Vec<SkinDocumentTexture> =
        sources.into_iter().filter_map(|source| install_decoded_source(renderer, source)).collect();

    set_decoded_skin_context(renderer, kind, default_manifest, document, document_textures);
    Ok(())
}

/// 1 個のフォントを renderer に登録する。フレーム分散インストールから呼ばれる。
pub fn install_decoded_font(renderer: &mut Renderer, font: DecodedFont) {
    let DecodedFont { stored_id, path, data } = font;
    let result: Result<()> = match data {
        DecodedFontData::Vector(bytes) => renderer.install_font_bytes(stored_id.clone(), bytes),
        DecodedFontData::Bitmap(bitmap) => {
            renderer.install_bitmap_font(stored_id.clone(), bitmap);
            Ok(())
        }
    };
    match result {
        Ok(()) => tracing::info!(
            font_id = %stored_id,
            path = %path.display(),
            "loaded beatoraja skin font"
        ),
        Err(error) => tracing::warn!(
            font_id = %stored_id,
            path = %path.display(),
            %error,
            "failed to install beatoraja skin font"
        ),
    }
}

/// 1 個の PNG ソースを renderer にアップロードし、対応する SkinDocumentTexture を返す。
/// アップロードに失敗した場合は None。
pub fn install_decoded_source(
    renderer: &mut Renderer,
    source: DecodedSource,
) -> Option<SkinDocumentTexture> {
    let DecodedSource { source_id, path, texture, asset } = source;
    let source_size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
    if let Err(error) = renderer.upsert_image_asset(TextureId(texture.0), &asset) {
        tracing::warn!(
            source_id = %source_id,
            texture_id = texture.0,
            path = %path.display(),
            %error,
            "failed to upload beatoraja skin source"
        );
        return None;
    }
    tracing::info!(
        source_id = %source_id,
        texture_id = texture.0,
        path = %path.display(),
        "loaded beatoraja skin source"
    );
    Some(SkinDocumentTexture { source_id, texture, source_size })
}

/// 取り込み済みのフォント/PNG から SkinContext を組み立てて scene context にセットする。
pub fn set_decoded_skin_context(
    renderer: &mut Renderer,
    kind: SkinKind,
    default_manifest: SkinManifest,
    document: SkinDocument,
    document_textures: Vec<SkinDocumentTexture>,
) {
    let context =
        SkinContext::from_manifest_and_document(default_manifest, document, document_textures);
    match kind {
        SkinKind::Play => renderer.set_play_skin_context(context),
        SkinKind::Select => renderer.set_select_skin_context(context),
        SkinKind::Result => renderer.set_result_skin_context(context),
    }
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
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    resolve_json_skin_asset_path(skin_root, source_path, document, files)
}

fn resolve_json_skin_asset_path(
    skin_root: &Path,
    asset_path: &str,
    document: &SkinDocument,
    files: &BTreeMap<String, String>,
) -> Option<PathBuf> {
    let normalized = asset_path.replace('\\', "/");
    if !normalized.contains('*') {
        return Some(skin_root.join(normalized));
    }

    let filepath =
        document.filepath.iter().find(|filepath| filepath.path.replace('\\', "/") == normalized);

    // 1. パスが filepath 定義と完全一致するときは、選択ファイルをそのまま使う。
    if let Some(filepath) = filepath
        && let Some(selected) = files.get(&filepath.name).filter(|selected| !selected.is_empty())
        && let Some(path) = resolve_selected_skin_file(skin_root, selected)
    {
        return Some(path);
    }

    // 2. 完全一致しなくても、filepath 定義の `*` が asset_path の `*` と同じ
    //    位置に来るなら、選択値からワイルドカード部分を抽出して埋め込む
    //    (例: 定義 `custom/laser/*` で選択 `custom/laser/veryshort` のとき、
    //         ソース `custom/laser/*/main.png` を `custom/laser/veryshort/main.png` へ)。
    if let Some(substituted) = substitute_filepath_choice(&normalized, &document.filepath, files) {
        let candidate = skin_root.join(&substituted);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let preferred =
        filepath.and_then(|filepath| (!filepath.def.is_empty()).then_some(filepath.def.as_str()));
    resolve_wildcard_path(skin_root, &normalized, preferred)
}

/// filepath 定義のワイルドカードと一致するユーザ選択値を `asset_path` の
/// ワイルドカード位置に埋め込んだ相対パスを返す。
///
/// 一致条件: `asset_path` と filepath 定義の `path` が `*` 直前の prefix を
/// 共有していること。選択値からも同じ prefix（および suffix）を剥がして
/// ワイルドカード相当の文字列を取り出し、`asset_path` の `*` を置換する。
fn substitute_filepath_choice(
    asset_path: &str,
    filepaths: &[SkinFilepathDef],
    files: &BTreeMap<String, String>,
) -> Option<String> {
    let (asset_before, asset_after) = asset_path.split_once('*')?;
    for filepath in filepaths {
        let def_path = filepath.path.replace('\\', "/");
        let Some((def_prefix, def_suffix)) = def_path.split_once('*') else {
            continue;
        };
        if def_prefix != asset_before {
            continue;
        }
        let Some(selected) = files.get(&filepath.name).filter(|selected| !selected.is_empty())
        else {
            continue;
        };
        let selected = selected.replace('\\', "/");
        let Some(stripped) = selected.strip_prefix(def_prefix) else {
            continue;
        };
        let wildcard_value = stripped.strip_suffix(def_suffix).unwrap_or(stripped);
        return Some(format!("{asset_before}{wildcard_value}{asset_after}"));
    }
    None
}

/// ユーザ選択のスキンルート相対パスを解決する。
/// 絶対パスやスキンルート外への脱出を含む選択は無効として `None` を返す。
fn resolve_selected_skin_file(skin_root: &Path, selected: &str) -> Option<PathBuf> {
    use std::path::Component;

    let relative = Path::new(selected);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        })
    {
        return None;
    }
    let candidate = skin_root.join(relative);
    candidate.is_file().then_some(candidate)
}

fn resolve_wildcard_path(
    skin_root: &Path,
    pattern: &str,
    preferred: Option<&str>,
) -> Option<PathBuf> {
    let pattern = strip_beatoraja_asset_filter(pattern);
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

fn strip_beatoraja_asset_filter(pattern: &str) -> &str {
    pattern.split_once('|').map_or(pattern, |(path, _)| path)
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

    fn filepath_def(name: &str, path: &str, def: &str) -> SkinFilepathDef {
        SkinFilepathDef {
            category: String::new(),
            name: name.to_string(),
            path: path.to_string(),
            def: def.to_string(),
        }
    }

    #[test]
    fn substitute_filepath_choice_replaces_wildcard_in_asset_path() {
        let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
        let mut files = BTreeMap::new();
        files.insert("レーザー".to_string(), "custom/laser/veryshort".to_string());

        let result = substitute_filepath_choice("custom/laser/*/main.png", &filepaths, &files);
        assert_eq!(result.as_deref(), Some("custom/laser/veryshort/main.png"));
    }

    #[test]
    fn substitute_filepath_choice_strips_def_suffix_from_selection() {
        let filepaths = vec![filepath_def("icon", "icon-*.png", "")];
        let mut files = BTreeMap::new();
        files.insert("icon".to_string(), "icon-blue.png".to_string());

        let result = substitute_filepath_choice("icon-*.png", &filepaths, &files);
        assert_eq!(result.as_deref(), Some("icon-blue.png"));
    }

    #[test]
    fn substitute_filepath_choice_returns_none_when_prefix_mismatch() {
        let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
        let mut files = BTreeMap::new();
        files.insert("レーザー".to_string(), "custom/laser/veryshort".to_string());

        // asset の prefix が定義と一致しない
        let result = substitute_filepath_choice("other/path/*.png", &filepaths, &files);
        assert_eq!(result, None);
    }

    #[test]
    fn substitute_filepath_choice_returns_none_when_no_selection() {
        let filepaths = vec![filepath_def("レーザー", "custom/laser/*", "default")];
        let files: BTreeMap<String, String> = BTreeMap::new();

        let result = substitute_filepath_choice("custom/laser/*/main.png", &filepaths, &files);
        assert_eq!(result, None);
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
    fn beatoraja_default_select_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/select.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_select_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_play7_1p_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/skins/ECFN/play/play7-1p.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn beatoraja_default_result_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/beatoraja/skin/default/result.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_result_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_result_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/skins/ECFN/RESULT/result-converted.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_result_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_select_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/skins/ECFN/select/select-converted.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_select_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_lua_skins_can_be_decoded_when_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/skins/ECFN");
        let cases = [
            (root.join("select/select.luaskin"), SkinKind::Select),
            (root.join("play/play7.luaskin"), SkinKind::Play),
            (root.join("RESULT/result.luaskin"), SkinKind::Result),
        ];
        for (skin_path, kind) in cases {
            if !skin_path.is_file() {
                continue;
            }
            let decoded = decode_beatoraja_skin(&skin_path, kind).unwrap();
            assert!(!decoded.document.destination.is_empty());
        }
    }

    #[test]
    fn starseeker_play_lua_skin_can_be_decoded_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert!(!decoded.document.destination.is_empty());
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
    fn apply_skin_from_config_lua_path_loads_beatoraja_skin_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/skins/ECFN/play/play7.luaskin");
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

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &BTreeMap::new())
                .unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
    }

    #[test]
    fn wildcard_skin_source_prefers_user_file_selection() {
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
        // ユーザ選択は `def` (blue) より優先される。
        let files = BTreeMap::from([("Parts".to_string(), "parts/default.png".to_string())]);

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
    }

    #[test]
    fn wildcard_skin_source_falls_back_when_user_selection_missing() {
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
        // 存在しないファイルを選択 → `def` (blue) へフォールバック。
        let files = BTreeMap::from([("Parts".to_string(), "parts/missing.png".to_string())]);

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &files).unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("blue.png"));
    }

    #[test]
    fn wildcard_skin_source_ignores_beatoraja_filter_suffix() {
        let root = unique_test_dir("bmz-json-source-filter");
        std::fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
        std::fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
        std::fs::write(root.join("parts/lanecover_lift/TYPE-M.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "filepath": [
                    {
                        "name": "レーンカバー",
                        "path": "parts/lanecover_lift/*.png|lanecover|",
                        "def": "default"
                    }
                ]
            }
            "#,
        )
        .unwrap();

        let resolved = resolve_json_skin_source_path(
            &root,
            "parts/lanecover_lift/*.png|lanecover|",
            &document,
            &BTreeMap::new(),
        )
        .unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("default.png"));
    }

    #[test]
    fn wildcard_skin_source_falls_back_to_first_match() {
        let root = unique_test_dir("bmz-json-source");
        std::fs::create_dir_all(root.join("parts")).unwrap();
        std::fs::write(root.join("parts/b.png"), []).unwrap();
        std::fs::write(root.join("parts/a.png"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let resolved =
            resolve_json_skin_source_path(&root, "parts/*.png", &document, &BTreeMap::new())
                .unwrap();

        assert_eq!(resolved.file_name().and_then(|name| name.to_str()), Some("a.png"));
    }

    #[test]
    fn wildcard_skin_font_resolves_nested_file() {
        let root = unique_test_dir("bmz-json-font");
        std::fs::create_dir_all(root.join("frame/SP/Default")).unwrap();
        std::fs::write(root.join("frame/SP/Default/song.fnt"), []).unwrap();
        let document: SkinDocument = serde_json::from_str("{}").unwrap();

        let resolved =
            resolve_json_skin_asset_path(&root, "frame/SP/*/song.fnt", &document, &BTreeMap::new())
                .unwrap();

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
