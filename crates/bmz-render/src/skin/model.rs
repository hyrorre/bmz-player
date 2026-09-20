use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct SkinObject {
    pub id: SkinObjectId,
    pub source: SkinSource,
    pub placements: Vec<SkinPlacement>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinDefinition {
    pub objects: Vec<SkinObject>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinDocumentTexture {
    pub source_id: String,
    pub texture: SkinTextureId,
    pub source_size: SkinImageSize,
}

/// Position-independent material for a note image. Ordinary taps use animation
/// time zero, so a lane can reuse this within a frame for every visible tap.
#[derive(Clone, Copy)]
pub(in crate::skin) struct NoteSprite {
    pub texture: SkinTextureId,
    pub uv: TextureRegion,
    pub source_size: SkinImageSize,
}

impl NoteSprite {
    pub(in crate::skin) fn render_item(self, rect: Rect) -> SkinRenderItem {
        SkinRenderItem::Image {
            texture: self.texture,
            rect,
            uv: self.uv,
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: SkinImageScale::Stretch,
            border: None,
            source_size: Some(self.source_size),
            linear_filter: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinBgaFrame {
    pub texture: SkinTextureId,
    pub source_size: SkinImageSize,
    pub tint_r: f32,
    pub tint_g: f32,
    pub tint_b: f32,
    pub tint_a: f32,
    /// 動画 BGA フレームかどうか。Layer/Layer2 でも動画ならクロマキーを適用しない
    /// (beatoraja の `ffmpeg.frag` 相当)。
    pub is_video: bool,
}

impl SkinBgaFrame {
    pub fn opaque(texture: SkinTextureId, source_size: SkinImageSize) -> Self {
        Self {
            texture,
            source_size,
            tint_r: 1.0,
            tint_g: 1.0,
            tint_b: 1.0,
            tint_a: 1.0,
            is_video: false,
        }
    }
}

static DEFAULT_RESULT_IR_SNAPSHOT: crate::scene::ResultIrSnapshot =
    crate::scene::ResultIrSnapshot::EMPTY;

#[derive(Debug, Clone, PartialEq)]
pub struct SkinTextState<'a> {
    pub title: &'a str,
    /// 現在プロフィール名 (STRING_PLAYER=2)。
    pub player_name: &'a str,
    /// IR ライバル名 (STRING_RIVAL=1)。未取得なら空。
    pub rival: &'a str,
    pub subtitle: &'a str,
    pub artist: &'a str,
    pub subartist: &'a str,
    pub genre: &'a str,
    pub difficulty_name: &'a str,
    pub play_level: &'a str,
    pub grade_diff: &'a str,
    pub target: &'a str,
    /// ref 1用の解決済みターゲット名。Some("")も表示名として優先する。
    pub resolved_target_name: Option<&'a str>,
    pub select_arrange: &'a str,
    pub select_arrange_2p: &'a str,
    pub select_gauge: &'a str,
    pub select_gauge_auto_shift: &'a str,
    pub select_bottom_shiftable_gauge: &'a str,
    pub select_double_option: &'a str,
    pub select_hs_fix: &'a str,
    pub select_assist: &'a str,
    pub select_mode: &'a str,
    pub select_sort: &'a str,
    pub select_ln_mode: &'a str,
    pub select_bga: &'a str,
    pub select_chart_replication: &'a str,
    pub select_judge_timing_auto_adjust: &'a str,
    pub current_folder: &'a str,
    pub bar_text: &'a str,
    pub table_level: &'a str,
    pub table_text_primary: &'a str,
    pub table_text_secondary: &'a str,
    pub table_text_fallback: &'a str,
    pub course_stage: Option<CourseStageMarker>,
    pub course_titles: [&'a str; 10],
    /// beatoraja `SkinProperty.STRING_SEARCHWORD` (`ref=30`) input overlay.
    /// Current song search query as typed by the user.
    pub search_word: &'a str,
    /// Multiplier applied to the rendered alpha of the `ref=30` text element.
    /// `1.0` keeps the skin-defined alpha unchanged; values < 1.0 are used for
    /// placeholder / inactive states (beatoraja `messageFontColor=GRAY` 相当).
    pub search_word_alpha: f32,
    /// Optional caret position for `search_word`, expressed as a UTF-8 byte index.
    pub search_caret_byte_index: Option<usize>,
    pub ir_ranking: &'a crate::scene::ResultIrSnapshot,
}

impl<'a> Default for SkinTextState<'a> {
    fn default() -> Self {
        Self {
            title: "",
            player_name: "",
            subtitle: "",
            artist: "",
            subartist: "",
            genre: "",
            difficulty_name: "",
            play_level: "",
            grade_diff: "",
            target: "",
            resolved_target_name: None,
            select_arrange: "",
            select_arrange_2p: "",
            select_gauge: "",
            select_gauge_auto_shift: "",
            select_bottom_shiftable_gauge: "",
            select_double_option: "",
            select_hs_fix: "",
            select_assist: "",
            select_mode: "",
            select_sort: "",
            select_ln_mode: "",
            select_bga: "",
            select_chart_replication: "",
            select_judge_timing_auto_adjust: "",
            current_folder: "",
            bar_text: "",
            table_level: "",
            table_text_primary: "",
            table_text_secondary: "",
            table_text_fallback: "",
            course_stage: None,
            course_titles: [""; 10],
            search_word: "",
            rival: "",
            search_word_alpha: 1.0,
            search_caret_byte_index: None,
            ir_ranking: &DEFAULT_RESULT_IR_SNAPSHOT,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SkinManifest {
    #[serde(default)]
    pub textures: Vec<SkinTextureManifest>,
    #[serde(default)]
    pub play: SkinPlayManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkinTextureManifest {
    pub id: u32,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SkinPlayManifest {
    pub note: Option<SkinImageManifest>,
    pub ln_start: Option<SkinImageManifest>,
    pub ln_end: Option<SkinImageManifest>,
    pub receptor: Option<SkinImageManifest>,
    pub judge_line: Option<SkinImageManifest>,
    pub gauge_frame: Option<SkinImageManifest>,
    pub gauge_fill: Option<SkinImageManifest>,
    pub combo_panel: Option<SkinImageManifest>,
    pub combo_panel_inactive: Option<SkinImageManifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SkinImageManifest {
    pub texture: u32,
    pub key_even_texture: Option<u32>,
    pub scratch_texture: Option<u32>,
    pub source_size: Option<SkinImageSize>,
    #[serde(default)]
    pub uv: TextureRegion,
    #[serde(default)]
    pub scale: SkinImageScale,
    pub border: Option<SkinImageBorder>,
}

impl SkinImageManifest {
    pub fn texture_for_lane(self, lane: Lane) -> u32 {
        match lane {
            Lane::Scratch | Lane::Scratch2 => self.scratch_texture.unwrap_or(self.texture),
            Lane::Key2 | Lane::Key4 | Lane::Key6 | Lane::Key9 | Lane::Key11 | Lane::Key13 => {
                self.key_even_texture.unwrap_or(self.texture)
            }
            Lane::Key1
            | Lane::Key3
            | Lane::Key5
            | Lane::Key7
            | Lane::Key8
            | Lane::Key10
            | Lane::Key12
            | Lane::Key14 => self.texture,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SkinImageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinImageScale {
    #[default]
    Stretch,
    NineSlice,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SkinImageBorder {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    #[serde(default)]
    pub unit: SkinImageBorderUnit,
}

impl SkinImageBorder {
    pub(super) fn normalized(self, source_size: Option<SkinImageSize>) -> Option<Self> {
        match self.unit {
            SkinImageBorderUnit::Normalized => Some(self),
            SkinImageBorderUnit::Pixels => {
                let size = source_size?;
                if size.width <= 0.0 || size.height <= 0.0 {
                    return None;
                }
                Some(Self {
                    left: self.left / size.width,
                    right: self.right / size.width,
                    top: self.top / size.height,
                    bottom: self.bottom / size.height,
                    unit: SkinImageBorderUnit::Normalized,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkinImageBorderUnit {
    #[default]
    Normalized,
    Pixels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSkinTexture {
    pub id: TextureId,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinRenderContext<'a> {
    pub phase: SkinPhase,
    pub elapsed_ms: i32,
    pub text: &'a [(TextSlot, String)],
    pub numbers: &'a [(NumberSlot, i64)],
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkinSource {
    Image { texture: SkinTextureId, uv: TextureRegion },
    Text { slot: TextSlot, style: TextStyle },
    Number { slot: NumberSlot, style: TextStyle, digits: u8 },
    Rect { color: Color },
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct TextureRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for TextureRegion {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSlot {
    Title,
    Artist,
    Judge,
    ClearType,
    ReplayState,
    Custom(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberSlot {
    Score,
    ExScore,
    Combo,
    MaxCombo,
    Gauge,
    Hispeed,
    JudgeCount,
    Custom(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkinPlacement {
    pub phase: SkinPhase,
    pub time_ms: i32,
    pub rect: Rect,
    pub alpha: f32,
    pub blend: BlendMode,
    pub animation: Animation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinPhase {
    Select,
    Play,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Add,
    /// LR2 / beatoraja `blend=4`。描画済みの色へソース色を乗算する。
    Multiply,
    /// 透明な render target へ通常 alpha 合成済みの offscreen texture 用。
    /// RGB は premultiplied 済みなので、再合成時に source alpha を掛け直さない。
    Premultiplied,
    /// BGA Layer/Layer2 の黒クロマキー描画。
    /// beatoraja の `layer.frag` 相当: RGB(0,0,0) ピクセルを α=0 として描画する。
    LayerMask,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub keyframes: Vec<Keyframe>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub time_ms: i32,
    pub rect: Rect,
    pub alpha: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkinRenderItem {
    Image {
        texture: SkinTextureId,
        rect: Rect,
        uv: TextureRegion,
        tint: Color,
        blend: BlendMode,
        scale: SkinImageScale,
        border: Option<SkinImageBorder>,
        source_size: Option<SkinImageSize>,
        linear_filter: bool,
    },
    RotatedImage {
        texture: SkinTextureId,
        rect: Rect,
        uv: TextureRegion,
        tint: Color,
        blend: BlendMode,
        source_size: Option<SkinImageSize>,
        linear_filter: bool,
        angle_deg: f32,
        center: Point,
        post_scale: Point,
    },
    Text {
        origin: Point,
        text: String,
        style: TextStyle,
        caret: Option<TextCaret>,
        blend: BlendMode,
        post_scale: Point,
    },
    Rect {
        rect: Rect,
        color: Color,
        blend: BlendMode,
    },
    RectBatch {
        rects: Arc<[RectCommand]>,
        cache: Option<RectBatchCache>,
    },
}
