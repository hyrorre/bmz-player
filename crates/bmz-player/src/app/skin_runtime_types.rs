use super::*;

pub(super) struct ActiveSkinVideoSource {
    pub(super) texture: SkinTextureId,
    pub(super) path: PathBuf,
    pub(super) decoder: Option<VideoBgaDecoder>,
    pub(super) last_pts: Option<i64>,
    pub(super) loop_start_us: i64,
    /// スキン config の option による静的な有効判定。
    pub(super) active: bool,
    /// このソースを参照する各 destination の op 条件。実行時 state に対して
    /// 評価し、未表示ソースの遅延起動とフレーム転送の可否を決める。
    /// 空なら参照されておらず常時可視扱い。
    pub(super) gating_op_sets: Vec<Vec<i32>>,
    /// `gating_op_sets` 評価に必要な document の有効 option 一覧。
    pub(super) enabled_options: Vec<i32>,
    /// リザルト draw state 構築に使う document の ranktime。
    pub(super) result_ranktime_ms: i32,
    pub(super) failed: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PendingSkinRenderProbe {
    pub(super) kind: SkinKind,
    pub(super) generation: u64,
    pub(super) applied_at: Instant,
}

pub(super) type PlaySkinSignature = (
    KeyMode,
    SessionMode,
    String,
    BTreeMap<String, String>,
    BTreeMap<String, String>,
    bmz_skin::LuaLoadRuntimeState,
    u64,
);
pub(super) type ResultSkinSignature = (
    ResultSkinSlot,
    String,
    BTreeMap<String, String>,
    BTreeMap<String, String>,
    bmz_skin::LuaLoadRuntimeState,
);

pub(super) fn play_skin_signature(
    key_mode: KeyMode,
    session_mode: SessionMode,
    path: &str,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
    runtime_state: &bmz_skin::LuaLoadRuntimeState,
    play_preload_generation: u64,
) -> PlaySkinSignature {
    (
        key_mode,
        session_mode,
        path.to_string(),
        options.clone(),
        files.clone(),
        runtime_state.clone(),
        play_preload_generation,
    )
}

pub(super) fn skin_offset_values_from_config(offsets: &[SkinOffsetConfig]) -> SkinOffsetValues {
    let mut values = SkinOffsetValues::default();
    for offset in offsets {
        values.set(
            offset.id,
            SkinOffsetValue {
                x: offset.x,
                y: offset.y,
                w: offset.w,
                h: offset.h,
                r: offset.r,
                a: offset.a,
            },
        );
    }
    values
}

pub(super) fn apply_skin_offsets_to_lua_runtime_state(
    runtime_state: &mut bmz_skin::LuaLoadRuntimeState,
    offsets: &[SkinOffsetConfig],
) {
    for offset in offsets {
        let value = bmz_skin::LuaSkinOffsetValue {
            x: offset.x,
            y: offset.y,
            w: offset.w,
            h: offset.h,
            r: offset.r,
            a: offset.a,
        };
        if let Some(name) = offset.name.as_deref().filter(|name| !name.is_empty()) {
            runtime_state.offset_values.entry(name.to_string()).or_insert(value);
        }
        runtime_state.offset_id_values.insert(offset.id, value);
    }
}

pub(super) fn lua_runtime_state_with_skin_offsets(
    mut runtime_state: bmz_skin::LuaLoadRuntimeState,
    offsets: &[SkinOffsetConfig],
) -> bmz_skin::LuaLoadRuntimeState {
    apply_skin_offsets_to_lua_runtime_state(&mut runtime_state, offsets);
    runtime_state
}

pub(super) fn lua_runtime_state_with_mode(
    mut runtime_state: bmz_skin::LuaLoadRuntimeState,
    runtime_mode: bmz_skin::LuaSkinRuntimeMode,
) -> bmz_skin::LuaLoadRuntimeState {
    runtime_state.runtime_mode = runtime_mode;
    runtime_state
}
