use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_audio::sample::DecodedSample;
use bmz_core::lane::KeyMode;
use bmz_render::assets::{RgbaImageAsset, load_static_rgba_image};
use bmz_render::bitmap_font::{BitmapFont, load_bitmap_font};
use bmz_render::plan::TextureId;
use bmz_render::renderer::{GpuUploader, PreparedTexture, Renderer};
use bmz_render::skin::{
    DestinationListEntry, SkinContext, SkinDocument, SkinDocumentTexture, SkinDrawState,
    SkinFilepathDef, SkinImageSize, SkinLuaDrawRuntime, SkinManifest, SkinTextureId,
    default_skin_manifest_for_root, lua_main_state_event_index, lua_main_state_float,
    lua_main_state_number, lua_main_state_option, lua_main_state_timer,
};
use bmz_skin::{
    LuaLoadRuntimeState, LuaMainState, LuaSkinRuntime, SkinKind as DecodeSkinKind,
    SkinLoadDependencies, SkinLoadedFileDependency, SkinPathContext,
};
use rayon::prelude::*;

use crate::config::profile_config::{SkinConfig, SkinOffsetConfig};
use crate::paths::{AppPaths, resolve_app_paths};
use crate::select_options::SessionMode;

/// `SkinConfig` から key_mode に対応するプレイスキン path / options / files / offsets を借用する。
pub struct PlaySkinSelection<'a> {
    pub key_mode: KeyMode,
    pub path: &'a str,
    pub options: &'a BTreeMap<String, String>,
    pub files: &'a BTreeMap<String, String>,
    pub offsets: &'a [SkinOffsetConfig],
}

/// `SkinConfig` から key_mode に応じたプレイスキン設定の参照を取り出す。
pub fn play_skin_selection_for(skin: &SkinConfig, key_mode: KeyMode) -> PlaySkinSelection<'_> {
    match key_mode {
        KeyMode::K5 => PlaySkinSelection {
            key_mode,
            path: skin.play5.as_str(),
            options: &skin.play5_options,
            files: &skin.play5_files,
            offsets: &skin.play5_offsets,
        },
        KeyMode::K4 => PlaySkinSelection {
            key_mode,
            path: skin.play4.as_str(),
            options: &skin.play4_options,
            files: &skin.play4_files,
            offsets: &skin.play4_offsets,
        },
        KeyMode::K6 => PlaySkinSelection {
            key_mode,
            path: skin.play6.as_str(),
            options: &skin.play6_options,
            files: &skin.play6_files,
            offsets: &skin.play6_offsets,
        },
        KeyMode::K7 => PlaySkinSelection {
            key_mode,
            path: skin.play7.as_str(),
            options: &skin.play7_options,
            files: &skin.play7_files,
            offsets: &skin.play7_offsets,
        },
        KeyMode::K8 => PlaySkinSelection {
            key_mode,
            path: skin.play8.as_str(),
            options: &skin.play8_options,
            files: &skin.play8_files,
            offsets: &skin.play8_offsets,
        },
        KeyMode::K10 => PlaySkinSelection {
            key_mode,
            path: skin.play10.as_str(),
            options: &skin.play10_options,
            files: &skin.play10_files,
            offsets: &skin.play10_offsets,
        },
        KeyMode::K14 => PlaySkinSelection {
            key_mode,
            path: skin.play14.as_str(),
            options: &skin.play14_options,
            files: &skin.play14_files,
            offsets: &skin.play14_offsets,
        },
        KeyMode::K9 => PlaySkinSelection {
            key_mode,
            path: skin.play9.as_str(),
            options: &skin.play9_options,
            files: &skin.play9_files,
            offsets: &skin.play9_offsets,
        },
    }
}

/// Battle SessionMode では source 5K/7K のみ専用スロットを使う。
/// 専用スロットが未設定なら同じ source key mode の通常スロットへ戻し、
/// それ以外のキーモードも通常スロットを使う。
pub fn play_skin_selection_for_session(
    skin: &SkinConfig,
    key_mode: KeyMode,
    session_mode: SessionMode,
) -> PlaySkinSelection<'_> {
    if !session_mode.uses_battle_skin() {
        return play_skin_selection_for(skin, key_mode);
    }
    match key_mode {
        KeyMode::K5 => {
            if skin.battle5.trim().is_empty() {
                play_skin_selection_for(skin, KeyMode::K5)
            } else {
                PlaySkinSelection {
                    key_mode: KeyMode::K5,
                    path: skin.battle5.as_str(),
                    options: &skin.battle5_options,
                    files: &skin.battle5_files,
                    offsets: &skin.battle5_offsets,
                }
            }
        }
        KeyMode::K7 => {
            if skin.battle7.trim().is_empty() {
                play_skin_selection_for(skin, KeyMode::K7)
            } else {
                PlaySkinSelection {
                    key_mode: KeyMode::K7,
                    path: skin.battle7.as_str(),
                    options: &skin.battle7_options,
                    files: &skin.battle7_files,
                    offsets: &skin.battle7_offsets,
                }
            }
        }
        _ => play_skin_selection_for(skin, key_mode),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

pub fn default_skin_document_path_from_paths(app_paths: &AppPaths, kind: SkinKind) -> PathBuf {
    let file_name = match kind {
        SkinKind::Play => "play7.json",
        SkinKind::Select => "select.json",
        SkinKind::Decide => "decide.json",
        SkinKind::Result => "result.json",
    };
    default_skin_root_from_paths(app_paths).join(file_name)
}

pub fn default_play_skin_document_path_from_paths(
    app_paths: &AppPaths,
    key_mode: KeyMode,
) -> PathBuf {
    let file_name = match key_mode {
        KeyMode::K4 => "play4.json",
        KeyMode::K5 => "play5.json",
        KeyMode::K6 => "play6.json",
        KeyMode::K7 => "play7.json",
        KeyMode::K8 => "play8.json",
        KeyMode::K9 => "play9.json",
        KeyMode::K10 => "play10.json",
        KeyMode::K14 => "play14.json",
    };
    default_skin_root_from_paths(app_paths).join(file_name)
}

/// バックグラウンドスレッドでデコード可能な 1 スキンぶんの中間データ。
/// Renderer に触らず Send-safe な値だけを保持する。
pub struct DecodedSkin {
    pub load_dependencies: SkinLoadDependencies,
    pub kind: SkinKind,
    pub document: SkinDocument,
    pub lua_runtime: Option<LuaSkinRuntime>,
    pub fonts: Vec<DecodedFont>,
    pub sources: Vec<DecodedSource>,
    pub audio_assets: Vec<DecodedSkinAudio>,
    pub stats: SkinDecodeStats,
}

pub struct DecodedSkinAudio {
    pub path: String,
    pub sample: DecodedSample,
}

#[derive(Debug, Clone, Default)]
pub struct SkinDecodeStats {
    pub document_us: u64,
    pub document_cache_hits: usize,
    pub document_cache_misses: usize,
    pub document_cache_uncacheable: usize,
    pub document_cache_disabled: usize,
    pub font_count: usize,
    pub font_decode_us: u64,
    pub font_payload_skipped: usize,
    pub font_cache_hits: usize,
    pub font_cache_misses: usize,
    pub font_cache_uncacheable: usize,
    pub font_cache_disabled: usize,
    pub source_task_count: usize,
    pub source_decode_us: u64,
    pub builtin_source_count: usize,
    pub image_source_count: usize,
    pub video_source_count: usize,
    pub source_cache_hits: usize,
    pub source_cache_misses: usize,
    pub source_cache_uncacheable: usize,
    pub source_cache_disabled: usize,
    pub video_source_cache_hits: usize,
    pub video_source_cache_misses: usize,
    pub video_source_cache_uncacheable: usize,
    pub video_source_cache_disabled: usize,
    pub source_texture_cache_hits: usize,
    pub video_source_texture_cache_hits: usize,
    pub source_texture_cache_hit_bytes: usize,
    pub video_source_texture_cache_hit_bytes: usize,
    pub decoded_source_count: usize,
    pub decoded_source_bytes: usize,
}

pub struct DecodedFont {
    pub stored_id: String,
    pub path: PathBuf,
    pub data: Option<DecodedFontData>,
    pub cache_key: Option<SkinFontCacheKey>,
}

#[derive(Clone)]
pub enum DecodedFontData {
    Vector(Vec<u8>),
    Bitmap(BitmapFont),
}

pub struct DecodedSource {
    pub source_id: String,
    pub path: PathBuf,
    pub texture: SkinTextureId,
    pub asset: Option<RgbaImageAsset>,
    pub size: SkinImageSize,
    pub cache_key: Option<SkinSourceAssetCacheKey>,
    pub is_video: bool,
    pub texture_lease: Option<Arc<()>>,
}

pub type SharedSkinSourceAssetCache = Arc<Mutex<SkinSourceAssetCache>>;
pub type SharedSkinDocumentCache = Arc<Mutex<SkinDocumentCache>>;
pub type SharedSkinFontCache = Arc<Mutex<SkinFontCache>>;
pub type SharedSkinGpuTextureCache = Arc<Mutex<SkinGpuTextureCache>>;

mod cache;
mod decode;
mod install;
mod path;

pub use cache::{
    CachedSkinGpuTexture, SkinDocumentCache, SkinFontCache, SkinFontCacheKey, SkinGpuTextureCache,
    SkinSourceAssetCache, SkinSourceAssetCacheKey,
};
pub(crate) use decode::enabled_options_from_selections;
pub use decode::{
    BeatorajaSkinDecodeRequest, apply_beatoraja_decide_json_skin, apply_beatoraja_json_skin,
    apply_beatoraja_result_json_skin, apply_beatoraja_select_json_skin, apply_default_skin,
    apply_default_skin_from_paths, apply_skin_from_config, decode_beatoraja_skin,
    decode_beatoraja_skin_request, decode_beatoraja_skin_with_options,
    decode_beatoraja_skin_with_options_and_runtime_state,
    decode_beatoraja_skin_with_options_and_runtime_state_and_caches,
    decode_beatoraja_skin_with_options_and_runtime_state_and_source_cache, default_skin_root,
    default_skin_root_from_paths, is_decodable_skin_path, is_json_skin_path, is_lr2_skin_path,
    is_lua_skin_path, load_default_skin_into_renderer, load_default_skin_into_renderer_from_paths,
};
pub use install::{
    PreparedSource, SkinUploadStats, UploadedSkin, install_decoded_font, install_decoded_skin,
    install_decoded_source, set_decoded_skin_context, upload_decoded_skin,
    upload_decoded_skin_with_texture_cache,
};
pub(crate) use path::RANDOM_FILE_SELECTION;

use cache::*;
use decode::*;
use install::*;
use path::*;

#[cfg(test)]
#[path = "skin_loader/tests.rs"]
mod tests;
