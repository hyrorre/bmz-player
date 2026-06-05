use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bmz_core::lane::KeyMode;
use bmz_render::assets::{RgbaImageAsset, load_png_rgba};
use bmz_render::bitmap_font::{BitmapFont, load_bitmap_font};
use bmz_render::plan::TextureId;
use bmz_render::renderer::{GpuUploader, PreparedTexture, Renderer};
use bmz_render::skin::{
    DestinationListEntry, SkinContext, SkinDocument, SkinDocumentTexture, SkinFilepathDef,
    SkinImageSize, SkinManifest, SkinTextureId,
};
use bmz_skin::SkinKind as DecodeSkinKind;
use rayon::prelude::*;

use crate::config::profile_config::SkinConfig;

/// `SkinConfig` から key_mode に対応するプレイスキン path / options / files を借用する。
pub struct PlaySkinSelection<'a> {
    pub key_mode: KeyMode,
    pub path: &'a str,
    pub options: &'a BTreeMap<String, String>,
    pub files: &'a BTreeMap<String, String>,
}

/// `SkinConfig` から key_mode に応じたプレイスキン設定の参照を取り出す。
pub fn play_skin_selection_for(skin: &SkinConfig, key_mode: KeyMode) -> PlaySkinSelection<'_> {
    match key_mode {
        KeyMode::K5 => PlaySkinSelection {
            key_mode,
            path: skin.play5.as_str(),
            options: &skin.play5_options,
            files: &skin.play5_files,
        },
        KeyMode::K7 => PlaySkinSelection {
            key_mode,
            path: skin.play7.as_str(),
            options: &skin.play7_options,
            files: &skin.play7_files,
        },
        KeyMode::K10 => PlaySkinSelection {
            key_mode,
            path: skin.play10.as_str(),
            options: &skin.play10_options,
            files: &skin.play10_files,
        },
        KeyMode::K14 => PlaySkinSelection {
            key_mode,
            path: skin.play14.as_str(),
            options: &skin.play14_options,
            files: &skin.play14_files,
        },
        KeyMode::K9 => PlaySkinSelection {
            key_mode,
            path: skin.play9.as_str(),
            options: &skin.play9_options,
            files: &skin.play9_files,
        },
        // Qwilight 系未実装の間は 7K プレイスキンへフォールバック。
        KeyMode::K4 | KeyMode::K6 | KeyMode::K8 => PlaySkinSelection {
            key_mode,
            path: skin.play7.as_str(),
            options: &skin.play7_options,
            files: &skin.play7_files,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinKind {
    Play,
    Select,
    Decide,
    Result,
}

impl SkinKind {
    fn first_texture_id(self) -> u32 {
        match self {
            SkinKind::Play => 10_000,
            SkinKind::Select => 20_000,
            SkinKind::Decide => 25_000,
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
            SkinKind::Decide => "decide",
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

enum SourceDecodeTask {
    File { index: usize, source_id: String, path: PathBuf },
    Builtin { index: usize, source_id: String, path: PathBuf, asset: RgbaImageAsset },
}

/// GPU アップロード済みの 1 ソース。upload worker が `DecodedSource` から生成する。
pub struct PreparedSource {
    pub source_id: String,
    pub texture: SkinTextureId,
    pub prepared: PreparedTexture,
    pub size: SkinImageSize,
}

/// decode + GPU アップロードまで終わった 1 スキンぶん。upload worker → main で渡す。
/// `PreparedTexture` (= wgpu::Texture/View) は `Send` なのでスレッド間で受け渡せる。
pub struct UploadedSkin {
    pub kind: SkinKind,
    pub document: SkinDocument,
    pub fonts: Vec<DecodedFont>,
    pub prepared: Vec<PreparedSource>,
}

/// `DecodedSkin` の全ソースを GPU へアップロードして `UploadedSkin` を返す。
/// upload worker スレッドから呼ぶ (`uploader` は `Renderer::gpu_uploader` の clone)。
pub fn upload_decoded_skin(uploader: &GpuUploader, decoded: DecodedSkin) -> UploadedSkin {
    let DecodedSkin { kind, document, fonts, sources } = decoded;
    let prepared = sources
        .into_iter()
        .filter_map(|source| {
            let DecodedSource { source_id, path, texture, asset } = source;
            if let Err(error) = asset.validate() {
                tracing::warn!(
                    source_id = %source_id,
                    texture_id = texture.0,
                    path = %path.display(),
                    %error,
                    "skipping invalid beatoraja skin source"
                );
                return None;
            }
            let size = SkinImageSize { width: asset.width as f32, height: asset.height as f32 };
            let prepared = uploader.upload(asset.width, asset.height, &asset.pixels);
            Some(PreparedSource { source_id, texture, prepared, size })
        })
        .collect();
    UploadedSkin { kind, document, fonts, prepared }
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
    renderer.set_play_skin_context(SkinContext::from_manifest(manifest), false);

    Ok(())
}

pub fn default_skin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default")
}

pub fn apply_default_skin(renderer: &mut Renderer) -> Result<()> {
    apply_skin_from_dir(renderer, &default_skin_root())
}

/// `profile.toml` の `[skin] play` 設定からスキンをロードする。
/// 空文字列 → デフォルトスキン、`.json`/`.luaskin`/`.lua`/`.lr2skin` 拡張子 → beatoraja スキン、
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

pub fn apply_beatoraja_decide_json_skin(renderer: &mut Renderer, skin_path: &Path) -> Result<()> {
    apply_beatoraja_json_skin_for_kind(renderer, skin_path, SkinKind::Decide)
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
    let source_tasks: Vec<SourceDecodeTask> = document
        .source
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            if let Some(asset) = lr2_builtin_source_asset(&source.path) {
                return Some(SourceDecodeTask::Builtin {
                    index,
                    source_id: source.id.clone(),
                    path: PathBuf::from(&source.path),
                    asset,
                });
            }
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
            Some(SourceDecodeTask::File { index, source_id: source.id.clone(), path: source_path })
        })
        .collect();

    let mut decoded_pairs: Vec<(usize, String, PathBuf, RgbaImageAsset)> = source_tasks
        .into_par_iter()
        .filter_map(|task| match task {
            SourceDecodeTask::Builtin { index, source_id, path, asset } => {
                Some((index, source_id, path, asset))
            }
            SourceDecodeTask::File { index, source_id, path: source_path } => {
                match load_png_rgba(&source_path) {
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
                }
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

fn lr2_builtin_source_asset(path: &str) -> Option<RgbaImageAsset> {
    let pixel = match path {
        // BMZ does not yet emulate every LR2 transition timer exactly.  A real
        // black reference source can leave WMII's fullscreen fade overlays
        // covering play, so keep it transparent until that animation path is
        // implemented more faithfully.
        "bmz://lr2/black" => [0, 0, 0, 0],
        "bmz://lr2/white" => [255, 255, 255, 255],
        // BACKBMP itself is drawn by the play snapshot path.  Keep a transparent
        // source so LR2 CSV objects using IMAGE_BACKBMP can be decoded without
        // failing texture resolution when the chart has no backbmp.
        "bmz://lr2/backbmp" => [0, 0, 0, 0],
        _ => return None,
    };
    Some(RgbaImageAsset { width: 1, height: 1, pixels: pixel.to_vec() })
}

fn load_skin_document(
    skin_path: &Path,
    kind: SkinKind,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> Result<SkinDocument> {
    let mut document = if is_lua_skin_path(skin_path) {
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
        loaded.document
    } else if is_lr2_skin_path(skin_path) {
        let loaded = bmz_skin::load_lr2_csv_skin(skin_path, decode_skin_kind(kind), options, files)
            .with_context(|| format!("failed to load lr2 csv skin: {}", skin_path.display()))?;
        for warning in loaded.warnings {
            tracing::warn!(
                path = %skin_path.display(),
                kind = ?kind,
                warning = %warning.message,
                "lr2 csv skin load warning"
            );
        }
        loaded.document
    } else {
        let document =
            bmz_skin::load_beatoraja_json_skin_with_defaults(skin_path).with_context(|| {
                format!("failed to load beatoraja json skin: {}", skin_path.display())
            })?;
        if options.is_empty() {
            document
        } else {
            // JSON スキンは property 定義から選択肢の op コード列を組み立て、
            // それを有効オプションとして再デコードする。
            let enabled = enabled_options_from_selections(&document, options);
            bmz_skin::load_beatoraja_json_skin(skin_path, &enabled).with_context(|| {
                format!("failed to load beatoraja json skin with options: {}", skin_path.display())
            })?
        }
    };
    // レンダー時の `enabled_options()` がユーザ選択を反映するように、
    // 選択値から算出した op コード列を document に格納する。
    // (選択が空でもデフォルト計算結果と同じになるため、常に設定して問題ない)
    document.user_selected_options = Some(enabled_options_from_selections(&document, options));
    Ok(document)
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
        SkinKind::Decide => DecodeSkinKind::Decide,
        SkinKind::Result => DecodeSkinKind::Result,
    }
}

pub fn is_decodable_skin_path(path: &Path) -> bool {
    is_json_skin_path(path) || is_lua_skin_path(path) || is_lr2_skin_path(path)
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

pub fn is_lr2_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2skin"))
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

    set_decoded_skin_context(renderer, kind, default_manifest, document, document_textures, false);
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
///
/// プレイ中に `play7_files` などだけ変えた場合、`preserve_play_dynamic_timers` を true にすると
/// グルーヴ枠など `timer_observe_boolean` 由来のアニメ経過を維持できる。
pub fn set_decoded_skin_context(
    renderer: &mut Renderer,
    kind: SkinKind,
    default_manifest: SkinManifest,
    document: SkinDocument,
    document_textures: Vec<SkinDocumentTexture>,
    preserve_play_dynamic_timers: bool,
) {
    let context =
        SkinContext::from_manifest_and_document(default_manifest, document, document_textures);
    match kind {
        SkinKind::Play => {
            renderer.set_play_skin_context(context, preserve_play_dynamic_timers);
        }
        SkinKind::Select => renderer.set_select_skin_context(context),
        SkinKind::Decide => renderer.set_decide_skin_context(context),
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
    for gauge in &document.gauges {
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
    fn enabled_options_includes_unselected_property_default_for_real_skin() {
        // 実際の Starseeker play7.luaskin で「スコアグラフ=On」のみ選択した時、
        // 未選択の「プレーサイド」のデフォルト (1P=920) と「スコアグラフ=On」(901)
        // の両方が enabled_options に入ることを確認する。
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            eprintln!("skipping: skin not present at {}", skin_path.display());
            return;
        }
        let mut selections = BTreeMap::new();
        selections.insert("スコアグラフ".to_string(), "On".to_string());

        let document =
            load_skin_document(&skin_path, SkinKind::Play, &selections, &BTreeMap::new())
                .expect("load skin document");
        let ops = enabled_options_from_selections(&document, &selections);
        assert!(ops.contains(&901), "expected 901 in ops, got {ops:?}");
        assert!(ops.contains(&920), "expected 920 (1P default) in ops, got {ops:?}");
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
        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7-1p.json");
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
            .join("../../data/skins/ECFN/RESULT/result-converted.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_result_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn ecfn_select_json_skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/ECFN/select/select-converted.json");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_select_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn m_select_lua_select_skin_renders_items_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/m_select/music_select.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Select,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        let document_textures =
            decoded.sources.iter().map(|source| bmz_render::skin::SkinDocumentTexture {
                source_id: source.source_id.clone(),
                texture: source.texture,
                source_size: bmz_render::skin::SkinImageSize {
                    width: source.asset.width as f32,
                    height: source.asset.height as f32,
                },
            });
        let context = bmz_render::skin::SkinContext::from_manifest_and_document(
            bmz_render::skin::default_skin_manifest(),
            decoded.document,
            document_textures,
        );
        assert!(context.document().is_some_and(|document| document.skin_type == 5));
        let snapshot = bmz_render::scene::SelectSnapshot {
            rows: vec![bmz_render::scene::SelectRowSnapshot {
                title: "Song".to_string(),
                ..Default::default()
            }],
            chart_count: 1,
            ..Default::default()
        };
        let items = context.select_document_items_with_dynamic_timers(&snapshot, None);
        assert!(!items.is_empty(), "m_select select skin should produce render items");
        assert!(
            items
                .iter()
                .any(|item| matches!(item, bmz_render::skin::SkinRenderItem::Text { text, .. } if text == "Song")),
            "m_select select skin should render the song title text"
        );
    }

    #[test]
    fn rm_skin_play_lua_skins_can_be_decoded_when_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/Rm-skin");
        let cases = [
            (root.join("play5main.luaskin"), SkinKind::Play),
            (root.join("play7main.luaskin"), SkinKind::Play),
            (root.join("play9main.luaskin"), SkinKind::Play),
        ];
        for (skin_path, kind) in cases {
            if !skin_path.is_file() {
                continue;
            }
            let decoded = decode_beatoraja_skin(&skin_path, kind).unwrap();
            assert!(!decoded.document.destination.is_empty(), "{}", skin_path.display());
        }
    }

    #[test]
    fn ecfn_lua_skins_can_be_decoded_when_available() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN");
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
    fn ecfn_play7_judge_combo_x_matches_beatoraja_layout_when_available() {
        use std::collections::HashMap;

        use bmz_render::skin::{SkinDocumentTexture, SkinImageSize, SkinRenderItem, SkinTextureId};

        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let mock_texture = SkinDocumentTexture {
            source_id: "mock".to_string(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 1920.0, height: 1080.0 },
        };
        let sources: HashMap<String, SkinDocumentTexture> = decoded
            .document
            .source
            .iter()
            .map(|source| (source.id.clone(), mock_texture.clone()))
            .chain(
                decoded
                    .document
                    .value
                    .iter()
                    .map(|value| (value.src.clone(), mock_texture.clone())),
            )
            .chain(
                decoded
                    .document
                    .image
                    .iter()
                    .map(|image| (image.src.clone(), mock_texture.clone())),
            )
            .collect();
        let items =
            decoded.document.judge_render_items("PGREAT", 42, 100, &sources).expect("judge items");
        let digit_xs: Vec<f32> = items
            .iter()
            .skip(1)
            .filter_map(|item| match item {
                SkinRenderItem::Image { rect, .. } => Some(rect.x),
                _ => None,
            })
            .collect();
        assert_eq!(digit_xs.len(), 2);
        let expected_first = 334.0 / 1920.0;
        let expected_second = 392.0 / 1920.0;
        assert!(
            (digit_xs[0] - expected_first).abs() < 0.001,
            "first digit x={} expected {expected_first}",
            digit_xs[0]
        );
        assert!(
            (digit_xs[1] - expected_second).abs() < 0.001,
            "second digit x={} expected {expected_second}",
            digit_xs[1]
        );
    }

    #[test]
    fn ecfn_play7_pre_notes_judge_line_renders_in_front_when_available() {
        use std::collections::HashMap;

        use bmz_render::skin::{
            SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinRenderItem, SkinTextState,
        };

        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let image_15 = decoded
            .document
            .image
            .iter()
            .find(|image| image.id == "15")
            .expect("ECFN id=15 image should decode");
        assert_eq!((image_15.src.as_str(), image_15.x, image_15.y), ("0", 16, 0));
        let image_15_map = decoded.document.image_map();
        let mapped_15 = image_15_map.get("15").expect("ECFN id=15 image should map");
        assert_eq!((mapped_15.src.as_str(), mapped_15.x, mapped_15.y), ("0", 16, 0));
        let system_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "0")
            .map(|source| source.texture)
            .expect("ECFN source 0 should decode");
        let system_size = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "0")
            .map(|source| SkinImageSize {
                width: source.asset.width as f32,
                height: source.asset.height as f32,
            })
            .expect("ECFN source 0 should decode");
        let sources: HashMap<String, SkinDocumentTexture> = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.asset.width as f32,
                            height: source.asset.height as f32,
                        },
                    },
                )
            })
            .collect();

        let (behind, front, _) = decoded.document.static_render_items_split(
            &sources,
            SkinDrawState::default(),
            SkinTextState::default(),
        );

        assert!(
            behind.iter().all(|item| !matches!(
                item,
                SkinRenderItem::Image {
                    texture,
                    rect,
                    ..
                } if *texture == system_texture
                    && (rect.y - 715.0 / 1080.0).abs() < 0.001
                    && (rect.height - 8.0 / 1080.0).abs() < 0.001
            )),
            "ECFN judge line should not remain behind notes"
        );
        assert!(
            front.iter().any(|item| matches!(
                item,
                SkinRenderItem::Image {
                    texture,
                    rect,
                    uv,
                    ..
                } if *texture == system_texture
                    && (rect.y - 715.0 / 1080.0).abs() < 0.001
                    && (rect.height - 8.0 / 1080.0).abs() < 0.001
                    && (uv.x - 16.0 / system_size.width).abs() < 0.001
                    && uv.y.abs() < 0.001
            )),
            "expected ECFN id=15 judge line in front items; got {front:?}"
        );
    }

    #[test]
    fn ecfn_play14_judge1_combo_is_right_of_judge_when_available() {
        use std::collections::HashMap;

        use bmz_core::lane::Lane;
        use bmz_render::skin::{
            MAX_JUDGE_REGIONS, SkinDocumentTexture, SkinDrawState, SkinImageSize, SkinRenderItem,
            SkinTextureId,
        };

        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play14.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let Ok(decoded) = decode_beatoraja_skin(&skin_path, SkinKind::Play) else {
            // play14main.lua は skin_config 未注入だと geometry が未初期化で落ちる。
            return;
        };
        let judge0 =
            decoded.document.judge.iter().find(|judge| judge.id == "judge").expect("judge");
        let judge1 =
            decoded.document.judge.iter().find(|judge| judge.id == "judge1").expect("judge1");
        assert_eq!(judge0.index, 0);
        assert_eq!(judge1.index, 1);

        let mock_texture = SkinDocumentTexture {
            source_id: "mock".to_string(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 1920.0, height: 1080.0 },
        };
        let sources: HashMap<String, SkinDocumentTexture> = decoded
            .document
            .source
            .iter()
            .map(|source| (source.id.clone(), mock_texture.clone()))
            .chain(
                decoded
                    .document
                    .value
                    .iter()
                    .map(|value| (value.src.clone(), mock_texture.clone())),
            )
            .chain(
                decoded
                    .document
                    .image
                    .iter()
                    .map(|image| (image.src.clone(), mock_texture.clone())),
            )
            .collect();

        let mut judge_ms = [None; MAX_JUDGE_REGIONS];
        let mut judge_index = [None; MAX_JUDGE_REGIONS];
        judge_ms[0] = Some(100);
        judge_ms[1] = Some(100);
        judge_index[0] = Some(0);
        judge_index[1] = Some(0);
        let state = SkinDrawState { judge_ms, judge_index, combo: 42, ..SkinDrawState::default() };

        let left_items = decoded
            .document
            .judge_render_items_for_def(judge0, 0, 42, 100, &sources, state)
            .expect("left judge");
        let right_items = decoded
            .document
            .judge_render_items_for_def(judge1, 0, 42, 100, &sources, state)
            .expect("right judge");
        let left_digit = left_items
            .iter()
            .skip(1)
            .find_map(|item| match item {
                SkinRenderItem::Image { rect, .. } => Some(rect.x),
                _ => None,
            })
            .expect("left combo digit");
        let right_digit = right_items
            .iter()
            .skip(1)
            .find_map(|item| match item {
                SkinRenderItem::Image { rect, .. } => Some(rect.x),
                _ => None,
            })
            .expect("right combo digit");
        assert!(
            right_digit > left_digit + 0.3,
            "judge1 digit x={right_digit} should be right of judge x={left_digit}"
        );

        let region = bmz_render::skin::lane_judge_region(
            Lane::Key8.index(),
            bmz_core::lane::LANE_COUNT,
            decoded.document.judge_region_count(),
        );
        assert_eq!(region, 1);
    }

    #[test]
    fn starseeker_play_lua_skin_can_be_decoded_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();

        assert!(!decoded.document.destination.is_empty());
    }

    #[test]
    fn starseeker_frame_filepath_selection_merges_frame_destinations_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/Starseeker/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }

        let mut files = BTreeMap::new();
        files.insert("フレーム".to_string(), "custom/frame/AC_SP/starseeker".to_string());

        let decoded = decode_beatoraja_skin_with_options(
            &skin_path,
            SkinKind::Play,
            &BTreeMap::new(),
            &files,
        )
        .expect("decode starseeker frame skin");

        assert!(
            decoded.document.source.iter().any(|source| source.id == "main_frame"),
            "expected main_frame source from starseeker frameL.lua"
        );
        assert!(
            decoded
                .document
                .all_destinations(&[])
                .iter()
                .any(|destination| destination.id == "base_L" || destination.id == "base_R"),
            "expected frame panel destinations from starseeker frameL.lua"
        );
    }

    #[test]
    fn play_skin_selection_for_returns_per_mode_fields() {
        let mut skin = SkinConfig {
            play5: "skin5.json".to_string(),
            play7: "skin7.json".to_string(),
            play9: "skin9.json".to_string(),
            play10: "skin10.json".to_string(),
            play14: "skin14.json".to_string(),
            ..SkinConfig::default()
        };
        skin.play5_options.insert("a".to_string(), "x".to_string());
        skin.play7_options.insert("b".to_string(), "y".to_string());
        skin.play9_options.insert("e".to_string(), "p".to_string());
        skin.play10_files.insert("c".to_string(), "z.png".to_string());
        skin.play14_files.insert("d".to_string(), "w.png".to_string());

        let s5 = play_skin_selection_for(&skin, KeyMode::K5);
        assert_eq!(s5.path, "skin5.json");
        assert!(s5.options.contains_key("a"));

        let s7 = play_skin_selection_for(&skin, KeyMode::K7);
        assert_eq!(s7.path, "skin7.json");
        assert!(s7.options.contains_key("b"));

        let s9 = play_skin_selection_for(&skin, KeyMode::K9);
        assert_eq!(s9.path, "skin9.json");
        assert!(s9.options.contains_key("e"));

        let s10 = play_skin_selection_for(&skin, KeyMode::K10);
        assert_eq!(s10.path, "skin10.json");
        assert!(s10.files.contains_key("c"));

        let s14 = play_skin_selection_for(&skin, KeyMode::K14);
        assert_eq!(s14.path, "skin14.json");
        assert!(s14.files.contains_key("d"));
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
        let skin_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/ECFN/play/play7.luaskin");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_skin_from_config(&mut renderer, skin_path.to_str().unwrap()).unwrap();
    }

    #[test]
    fn wmii_fhd_lr2skin_decodes_play_document_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        assert_eq!(decoded.document.name, "WMII FHD play AC");
        assert!(decoded.document.w >= 1920);
        assert!(decoded.document.source.len() >= 10);
        assert!(decoded.document.image.len() >= 100);
        assert!(
            decoded.document.source.iter().any(|source| source.id == "110")
                && decoded.document.source.iter().any(|source| source.id == "111"),
            "expected LR2 black/white reference sources"
        );
        let note = decoded.document.note.as_ref().expect("lr2 play skin should define notes");
        assert!(!note.group.is_empty());
        assert!(decoded.document.gauge.is_some());
        assert!(decoded.document.bga.is_some());
        assert!(
            decoded.sources.len() >= 10,
            "expected WMII sources to decode, got {}; source paths: {:?}; decoded: {:?}",
            decoded.sources.len(),
            decoded.document.source.iter().map(|source| source.path.as_str()).collect::<Vec<_>>(),
            decoded.sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>()
        );
        let black = decoded.sources.iter().find(|source| source.source_id == "110").unwrap();
        let white = decoded.sources.iter().find(|source| source.source_id == "111").unwrap();
        assert_eq!(black.asset.pixels, vec![0, 0, 0, 0]);
        assert_eq!(white.asset.pixels, vec![255, 255, 255, 255]);
    }

    #[test]
    fn wmii_fhd_lr2skin_can_be_applied_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }
        let mut renderer = Renderer::default();

        apply_beatoraja_json_skin(&mut renderer, &skin_path).unwrap();
    }

    #[test]
    fn wmii_fhd_lr2skin_produces_static_play_items_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.asset.width as f32,
                            height: source.asset.height as f32,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            ready_timer_ms: Some(2_000),
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            state,
            bmz_render::skin::SkinTextState::default(),
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_ac_bga_frame_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let frame_image = decoded
            .document
            .image
            .iter()
            .find(|image| image.src == "2" && image.x == 1016 && image.y == 1276 && image.w == 389)
            .expect("WMII AC frame image should decode");
        let frame_destination = decoded
            .document
            .destination
            .iter()
            .filter_map(|entry| match entry {
                bmz_render::skin::DestinationListEntry::Single(destination) => Some(destination),
                bmz_render::skin::DestinationListEntry::Conditional { destinations, .. } => {
                    destinations.iter().find(|destination| destination.id == frame_image.id)
                }
            })
            .find(|destination| destination.id == frame_image.id && destination.op == [41, 30])
            .expect("WMII AC frame destination should decode");
        assert!(
            frame_destination.dst.len() >= 2,
            "expected WMII AC frame destination keyframes, got {:?}",
            frame_destination.dst
        );
        let frame_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "2")
            .expect("WMII AC frame source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.asset.width as f32,
                            height: source.asset.height as f32,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            ready_timer_ms: Some(2_000),
            has_bga: true,
            bga_enabled: true,
            autoplay: false,
            skin_loaded: true,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            state,
            bmz_render::skin::SkinTextState::default(),
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                    if *texture == frame_texture
                        && (rect.x - 845.0 / 1920.0).abs() < 0.001
                        && (rect.width - 389.0 / 1920.0).abs() < 0.001
                        && tint.a > 0.5
            )),
            "expected WMII AC BGA frame item from source 2; got {items:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_uses_full_note_lane_region_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let area = decoded
            .document
            .note_lane_area(
                bmz_core::lane::Lane::Scratch,
                bmz_core::lane::KeyMode::K7,
                &decoded.document.enabled_options(),
            )
            .expect("WMII scratch lane area should decode");

        assert!((area.x - 75.0 / 1920.0).abs() < 0.001);
        assert!(
            area.height > 0.65,
            "expected LR2 note.dst to define the full scroll lane height, got {area:?}"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_maps_note_sources_by_lr2_lane_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let note = decoded.document.note.as_ref().expect("WMII notes should decode");
        let images = decoded.document.image_map();
        let scratch =
            images.get(note.note[7].as_str()).expect("WMII scratch note image should resolve");
        let key1 = images.get(note.note[0].as_str()).expect("WMII key1 note image should resolve");
        let key2 = images.get(note.note[1].as_str()).expect("WMII key2 note image should resolve");

        assert_eq!((scratch.x, scratch.w), (94, 90));
        assert_eq!((key1.x, key1.w), (187, 52));
        assert_eq!((key2.x, key2.w), (241, 40));
    }

    #[test]
    fn wmii_fhd_lr2skin_inserts_notes_marker_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        assert!(
            decoded
                .document
                .all_destinations(&decoded.document.enabled_options())
                .iter()
                .any(|destination| destination.id == "notes"),
            "LR2 play skins should insert the notes marker at the first DST_NOTE command"
        );
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_groove_gauge_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let gauge_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "19")
            .expect("WMII gauge source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.asset.width as f32,
                            height: source.asset.height as f32,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for gauge_type in [
            bmz_core::clear::GaugeType::AssistEasy,
            bmz_core::clear::GaugeType::Normal,
            bmz_core::clear::GaugeType::Hard,
        ] {
            let state = bmz_render::skin::SkinDrawState {
                elapsed_ms: 2_000,
                play_timer_ms: Some(2_000),
                gauge: 80.0,
                gauge_max: 100.0,
                gauge_border: 80.0,
                gauge_type: gauge_type as i32,
                ..Default::default()
            };

            let items = decoded.document.static_render_items(
                &sources,
                state,
                bmz_render::skin::SkinTextState::default(),
            );
            assert!(
                items.iter().any(|item| matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                        if *texture == gauge_texture
                            && (rect.x - 54.0 / 1920.0).abs() < 0.001
                            && rect.width > 0.004
                            && tint.a > 0.5
                )),
                "expected WMII groove gauge item from source 19 for {gauge_type:?}; got {items:?}"
            );
        }
    }

    #[test]
    fn wmii_fhd_lr2skin_renders_judge_and_combo_when_available() {
        let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !skin_path.is_file() {
            return;
        }

        let decoded = decode_beatoraja_skin(&skin_path, SkinKind::Play).unwrap();
        let judge_texture = decoded
            .sources
            .iter()
            .find(|source| source.source_id == "13")
            .expect("WMII judge source should load")
            .texture;
        let sources = decoded
            .sources
            .iter()
            .map(|source| {
                (
                    source.source_id.clone(),
                    SkinDocumentTexture {
                        source_id: source.source_id.clone(),
                        texture: source.texture,
                        source_size: SkinImageSize {
                            width: source.asset.width as f32,
                            height: source.asset.height as f32,
                        },
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut judge_ms = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
        judge_ms[0] = Some(100);
        let mut judge_index = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
        judge_index[0] = Some(0);
        let mut judge_combo = [0; bmz_render::skin::MAX_JUDGE_REGIONS];
        judge_combo[0] = 123;
        let state = bmz_render::skin::SkinDrawState {
            elapsed_ms: 2_000,
            play_timer_ms: Some(2_000),
            judge_ms,
            judge_index,
            judge_combo,
            ..Default::default()
        };

        let items = decoded.document.static_render_items(
            &sources,
            state,
            bmz_render::skin::SkinTextState::default(),
        );
        let judge_items = items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                        if *texture == judge_texture
                            && rect.height > 0.01
                            && tint.a > 0.5
                )
            })
            .count();

        assert!(
            judge_items >= 2,
            "expected WMII judge text and combo digits from source 13; got {items:?}"
        );
        assert!(
            items.iter().any(|item| matches!(
                item,
                bmz_render::skin::SkinRenderItem::Image { texture, rect, uv, tint, .. }
                    if *texture == judge_texture
                        && rect.height > 0.05
                        && uv.y < 0.001
                        && tint.a > 0.5
            )),
            "expected PGREAT judge image to use the top WMII judge source row; got {items:?}"
        );

        for (judge_index, label) in ["PGREAT", "GREAT", "GOOD", "BAD", "POOR"].iter().enumerate() {
            let mut judge_ms = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
            judge_ms[0] = Some(100);
            let mut judge_indices = [None; bmz_render::skin::MAX_JUDGE_REGIONS];
            judge_indices[0] = Some(judge_index);
            let mut judge_combo = [0; bmz_render::skin::MAX_JUDGE_REGIONS];
            judge_combo[0] = 123;
            let state = bmz_render::skin::SkinDrawState {
                elapsed_ms: 2_000,
                play_timer_ms: Some(2_000),
                judge_ms,
                judge_index: judge_indices,
                judge_combo,
                ..Default::default()
            };
            let items = decoded.document.static_render_items(
                &sources,
                state,
                bmz_render::skin::SkinTextState::default(),
            );
            assert!(
                items.iter().any(|item| matches!(
                    item,
                    bmz_render::skin::SkinRenderItem::Image { texture, rect, tint, .. }
                        if *texture == judge_texture
                            && rect.height > 0.05
                            && tint.a > 0.5
                )),
                "expected WMII {label} judge image to render; got {items:?}"
            );
        }
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
