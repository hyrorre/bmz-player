//! 本体設定 / スキン設定 / デバッグ表示のための egui レイヤ。
//!
//! `egui::Context` と winit 連携状態 (`egui_winit::State`) を所有し、毎フレーム
//! UI を構築して描画プリミティブ (`EguiFrame`) を生成する。bmz-render はその
//! プリミティブをゲーム / スキン描画の上にペイントするだけにする。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bmz_render::skin::{SkinDocument, SkinFilepathDef, SkinOffsetDef, SkinPropertyDef};
use bmz_render::skin_offset::SKIN_OFFSET_BAR_LINE;
use bmz_render::ui::EguiFrame;
use egui::{NumExt, ViewportId};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::config::app_config::{
    AppConfig, AudioBackend, AudioBufferSizeMode, RendererBackend, WindowMode,
};
use crate::config::profile_config::{
    ProfileConfig, SkinConfig, SkinHistoryEntryConfig, SkinOffsetConfig,
};
use crate::screens::course_session::CourseResultSummary;
use crate::screens::select_model::SelectCourseRow;
use crate::songs_cmd::{add_song_root_entry, remove_song_root_entry};

/// スキンが宣言する設定可能項目の定義 (1 シーン分)。
///
/// renderer が保持する `SkinDocument` から複製して egui パネルへ渡す。
#[derive(Clone, Default)]
pub struct SceneSkinDefs {
    pub property: Vec<SkinPropertyDef>,
    pub filepath: Vec<SkinFilepathDef>,
    pub offset: Vec<SkinOffsetDef>,
}

impl SceneSkinDefs {
    /// renderer の `SkinDocument` から設定可能項目の定義を複製する。
    pub fn from_document(document: Option<&SkinDocument>) -> Self {
        match document {
            Some(doc) => Self {
                property: doc.property.clone(),
                filepath: doc.filepath.clone(),
                offset: doc.offset.clone(),
            },
            None => Self::default(),
        }
    }

    /// beatoraja はすべてのプレイ用スキンに共通 offset を追加するため、
    /// BMZ のスキン設定 UI でも play skin だけ同じ項目を常時出す。
    pub fn from_play_document(document: Option<&SkinDocument>) -> Self {
        let mut defs = Self::from_document(document);
        defs.append_missing_beatoraja_play_offsets();
        defs
    }

    fn is_empty(&self) -> bool {
        self.property.is_empty() && self.filepath.is_empty() && self.offset.is_empty()
    }

    fn append_missing_beatoraja_play_offsets(&mut self) {
        for offset in beatoraja_play_common_offsets() {
            if let Some(existing) = self.offset.iter_mut().find(|existing| existing.id == offset.id)
            {
                if offset.id == SKIN_OFFSET_BAR_LINE {
                    existing.h = true;
                    existing.a = true;
                }
            } else {
                self.offset.push(offset);
            }
        }
    }
}

fn beatoraja_play_common_offsets() -> [SkinOffsetDef; 5] {
    [
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "All offset(%)".to_string(),
            id: 10,
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: false,
        },
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "Notes offset".to_string(),
            id: 30,
            x: false,
            y: false,
            w: false,
            h: true,
            r: false,
            a: false,
        },
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "Judge offset".to_string(),
            id: 32,
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: true,
        },
        SkinOffsetDef {
            category: "beatoraja".to_string(),
            name: "Judge Detail offset".to_string(),
            id: 33,
            x: true,
            y: true,
            w: true,
            h: true,
            r: false,
            a: true,
        },
        SkinOffsetDef {
            category: "bmz".to_string(),
            name: "Bar Line offset".to_string(),
            id: SKIN_OFFSET_BAR_LINE,
            x: false,
            y: false,
            w: false,
            h: true,
            r: false,
            a: true,
        },
    ]
}

/// 選曲 / プレイ / リザルト各スキンの設定可能項目。
#[derive(Default)]
pub struct SkinConfigMeta {
    pub select: SceneSkinDefs,
    pub decide: SceneSkinDefs,
    pub play5: SceneSkinDefs,
    pub play7: SceneSkinDefs,
    pub play10: SceneSkinDefs,
    pub play14: SceneSkinDefs,
    pub result: SceneSkinDefs,
}

#[derive(Debug, Clone, Default)]
pub struct SkinCatalog {
    pub select: Vec<SkinCandidate>,
    pub decide: Vec<SkinCandidate>,
    pub play5: Vec<SkinCandidate>,
    pub play7: Vec<SkinCandidate>,
    pub play10: Vec<SkinCandidate>,
    pub play14: Vec<SkinCandidate>,
    pub result: Vec<SkinCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinCandidate {
    pub name: String,
    pub path: String,
}

/// デバッグ表示パネルへ毎フレーム渡すアプリ側の情報。
pub struct DebugInfo {
    /// 現在のシーン種別 ("Select" / "Play" / "Result")。
    pub scene: &'static str,
    /// 描画サーフェスの幅 (px)。
    pub width: u32,
    /// 描画サーフェスの高さ (px)。
    pub height: u32,
}

/// `EguiLayer::run` の 1 フレーム出力。
pub struct EguiOutput {
    /// renderer へ渡す描画データ。
    pub frame: EguiFrame,
    /// 本体設定 (`AppConfig`) の保存が要求されたか。
    pub save_app_config: bool,
    /// プロファイル設定 (`ProfileConfig`) の保存が要求されたか。
    pub save_profile_config: bool,
    /// profile.toml からスキン設定を再読込して未保存変更を戻す要求。
    pub reset_skin_config: bool,
    /// スキン設定値が変更されたか。app 側でデバウンスして再読込へつなぐ。
    pub skin_config_changed: bool,
    /// デバッグ表示パネルの現在の開閉状態。
    /// profile config の `ui.show_fps` へ同期し、終了時に永続化される。
    pub debug_panel_visible: bool,
    /// 有効な曲ルートをライブラリ DB へ再スキャンする要求。
    pub trigger_song_rescan: bool,
}

/// egui の状態管理とフレーム構築を担うレイヤ。
pub struct EguiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    /// メニュー全体の表示状態。F5 でトグルする。
    visible: bool,
    /// デバッグ表示パネルの開閉状態。
    show_debug: bool,
    /// 本体設定パネルの開閉状態。
    show_settings: bool,
    /// スキン設定パネルの開閉状態。
    show_skin: bool,
    /// 本体設定パネル: 曲フォルダ追加用の入力欄。
    settings_new_root_path: String,
    /// 本体設定パネル: 曲フォルダ追加の直近エラー。
    settings_add_root_error: String,
}

impl EguiLayer {
    /// `show_debug` はデバッグ表示パネルの初期開閉状態 (profile config の
    /// `ui.show_fps` を引き継ぐ)。
    pub fn new(window: &Window, show_debug: bool) -> Self {
        let ctx = egui::Context::default();
        install_japanese_font(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            ctx,
            state,
            visible: false,
            show_debug,
            show_settings: false,
            show_skin: false,
            settings_new_root_path: String::new(),
            settings_add_root_error: String::new(),
        }
    }

    /// メニュー表示状態を反転する (F5)。
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        tracing::info!(visible = self.visible, "egui menu toggled");
    }

    /// winit イベントを egui へ供給する。
    ///
    /// 戻り値が true のとき、その入力は egui が消費したのでゲーム側へ伝播させない。
    /// メニュー非表示中は egui に状態は渡すが消費とは扱わず、ゲーム操作を妨げない。
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> bool {
        let response = self.state.on_window_event(window, event);
        self.visible && response.consumed
    }

    /// 1 フレーム分の UI を構築し、描画データと要求されたアクションを返す。
    pub fn run(
        &mut self,
        window: &Window,
        info: &DebugInfo,
        app_config: &mut AppConfig,
        profile_config: &mut ProfileConfig,
        skin_meta: &SkinConfigMeta,
        skin_catalog: &SkinCatalog,
        course_result: Option<&CourseResultSummary>,
        course_preview: Option<&SelectCourseRow>,
    ) -> EguiOutput {
        let raw_input = self.state.take_egui_input(window);
        let ctx = self.ctx.clone();
        let show_debug = &mut self.show_debug;
        let show_settings = &mut self.show_settings;
        let show_skin = &mut self.show_skin;
        let mut save_app_config = false;
        let mut save_profile_config = false;
        let mut reset_skin_config = false;
        let mut skin_config_changed = false;
        let mut trigger_song_rescan = false;
        let visible_flag = &mut self.visible;
        let full_output = ctx.run_ui(raw_input, |ui| {
            // Course result overlay is shown regardless of menu visibility — it's
            // a gameplay-level summary, not a settings panel.
            if let Some(summary) = course_result {
                build_course_result_panel(ui.ctx(), summary);
            }
            // Course preview is shown only while hovering a course row on the
            // select screen — the caller decides when to pass Some().
            if let Some(preview) = course_preview {
                build_course_preview_panel(ui.ctx(), preview);
            }
            if *visible_flag {
                let ctx = ui.ctx();
                build_menu(ctx, visible_flag, show_debug, show_settings, show_skin);
                build_debug_panel(ctx, show_debug, info);
                let settings_actions = build_settings_panel(
                    ctx,
                    show_settings,
                    app_config,
                    &mut self.settings_new_root_path,
                    &mut self.settings_add_root_error,
                );
                save_app_config |= settings_actions.save;
                trigger_song_rescan |= settings_actions.rescan;
                let skin_actions = build_skin_panel(
                    ctx,
                    show_skin,
                    &mut profile_config.skin,
                    skin_meta,
                    skin_catalog,
                );
                save_profile_config |= skin_actions.save;
                reset_skin_config |= skin_actions.reset;
                skin_config_changed |= skin_actions.changed;
            }
        });
        self.state.handle_platform_output(window, full_output.platform_output);
        let primitives = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        EguiOutput {
            frame: EguiFrame {
                primitives,
                textures_delta: full_output.textures_delta,
                pixels_per_point: full_output.pixels_per_point,
            },
            save_app_config,
            save_profile_config,
            reset_skin_config,
            skin_config_changed,
            debug_panel_visible: *show_debug,
            trigger_song_rescan,
        }
    }
}

/// egui のデフォルトフォントは日本語グリフを含まないため、OS の CJK 対応
/// フォントを各フォントファミリの末尾フォールバックとして登録する。
fn install_japanese_font(ctx: &egui::Context) {
    let Some(bytes) = bmz_render::renderer::load_japanese_font_bytes() else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    let font_data = egui::FontData::from_owned(bytes).tweak(egui::FontTweak {
        scale: 1.0,
        y_offset_factor: 0.26,
        y_offset: 0.0,
        ..Default::default()
    });
    fonts.font_data.insert("bmz_jp".to_owned(), std::sync::Arc::new(font_data));
    // Latin は egui 既定フォントのまま、欠落グリフ (日本語) だけここへフォールバックさせる。
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(chain) = fonts.families.get_mut(&family) {
            chain.push("bmz_jp".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// 各サブパネルの開閉を切り替えるメインメニューハブ。
fn build_menu(
    ctx: &egui::Context,
    visible: &mut bool,
    show_debug: &mut bool,
    show_settings: &mut bool,
    show_skin: &mut bool,
) {
    egui::Window::new("BMZ メニュー")
        .open(visible)
        .constrain_to(ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN))
        .default_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            ui.label("F5 でこのメニューを開閉します。");
            ui.separator();
            ui.checkbox(show_debug, "デバッグ表示");
            ui.checkbox(show_settings, "本体設定");
            ui.checkbox(show_skin, "スキン設定");
        });
}

/// Window 内コンテンツを全体スクロール可能にする。
fn scrollable_window_content<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    // レイアウト確定前に inner が膨らむのを防ぐため、
    // 利用可能矩形から ScrollArea 高さを明示的に制限する。
    let available = ui.available_rect_before_wrap();
    let max_height = available.height().max(64.0);
    egui::ScrollArea::vertical()
        .max_height(max_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(available.width());
            add_contents(ui)
        })
        .inner
}

/// パネル Window の default / max サイズと初期位置をビューポート内に収める。
const PANEL_VIEWPORT_MARGIN: f32 = 16.0;

/// Window の outer サイズ = inner + chrome。egui `Window` の resize margin 計算に合わせる。
fn panel_window_chrome(ctx: &egui::Context) -> egui::Vec2 {
    let style = ctx.global_style();
    let frame = egui::Frame::window(&style);
    let title_bar_inner_height = ctx
        .fonts_mut(|fonts| fonts.row_height(&style.text_styles[&egui::TextStyle::Heading]))
        .at_least(style.spacing.interact_size.y)
        + frame.inner_margin.sum().y;
    let title_content_spacing = frame.stroke.width;
    let frame_margin = frame.total_margin().sum();
    egui::vec2(frame_margin.x, frame_margin.y + title_bar_inner_height + title_content_spacing)
}

fn clamp_panel_layout(
    constrain: egui::Rect,
    chrome: egui::Vec2,
    preferred_width: f32,
    preferred_height: f32,
    preferred_pos: egui::Pos2,
) -> (egui::Vec2, egui::Vec2, egui::Pos2) {
    let max_inner = egui::vec2(
        (constrain.width() - chrome.x).max(200.0),
        (constrain.height() - chrome.y).max(80.0),
    );
    let default_inner =
        egui::vec2(preferred_width.min(max_inner.x), preferred_height.min(max_inner.y));
    let outer = default_inner + chrome;
    let max_x = (constrain.max.x - outer.x).max(constrain.min.x);
    let max_y = (constrain.max.y - outer.y).max(constrain.min.y);
    let default_pos = egui::pos2(
        preferred_pos.x.clamp(constrain.min.x, max_x),
        preferred_pos.y.clamp(constrain.min.y, max_y),
    );
    (default_inner, max_inner, default_pos)
}

/// 既存 Window の outer rect が constrain からはみ出していれば位置を補正する。
fn panel_window_pos(
    ctx: &egui::Context,
    title: &'static str,
    constrain: egui::Rect,
    default_pos: egui::Pos2,
) -> egui::Pos2 {
    let id = egui::Id::new(title);
    let Some(rect) = ctx.memory(|memory| memory.area_rect(id)) else {
        return default_pos;
    };
    constrain_window_rect_to_area(rect, constrain).min
}

/// egui `Context::constrain_window_rect_to_area` と同等 (crate 外からは非公開のため)。
fn constrain_window_rect_to_area(window: egui::Rect, area: egui::Rect) -> egui::Rect {
    let mut pos = window.min;
    let margin_x = (window.width() - area.width()).at_least(0.0);
    let margin_y = (window.height() - area.height()).at_least(0.0);
    pos.x = pos.x.at_most(area.right() + margin_x - window.width());
    pos.x = pos.x.at_least(area.left() - margin_x);
    pos.y = pos.y.at_most(area.bottom() + margin_y - window.height());
    pos.y = pos.y.at_least(area.top() - margin_y);
    egui::Rect::from_min_size(pos, window.size())
}

fn sized_panel_window<'open>(
    title: &'static str,
    ctx: &egui::Context,
    open: &'open mut bool,
    preferred_width: f32,
    preferred_height: f32,
    default_pos: egui::Pos2,
) -> egui::Window<'open> {
    let constrain = ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN);
    let chrome = panel_window_chrome(ctx);
    let (default_inner, max_inner, clamped_default_pos) =
        clamp_panel_layout(constrain, chrome, preferred_width, preferred_height, default_pos);
    let pos = panel_window_pos(ctx, title, constrain, clamped_default_pos);
    egui::Window::new(title)
        .open(open)
        .resizable(true)
        .constrain_to(constrain)
        .current_pos(pos)
        .default_size(default_inner)
        .max_size(max_inner)
        .min_size([280.0, 80.0])
}

/// コース全体リザルトを画面上にオーバーレイ表示する。
///
/// `finished_course` が `Some` のあいだ表示され続け、リザルト画面を抜けると
/// `None` になって自動的に消える。最小実装として egui::Window を 1 枚出すだけ。
fn build_course_result_panel(ctx: &egui::Context, summary: &CourseResultSummary) {
    let content_rect = ctx.content_rect();
    // Panel widened from 360px to 440px so the 6-column per-chart grid
    // (#/title/EX/combo/clear/miss) fits without horizontal scroll.
    let panel_width = 440.0_f32;
    let pos = egui::pos2(content_rect.right() - panel_width - 16.0, 16.0);

    egui::Window::new("コースリザルト")
        .id(egui::Id::new("course_result_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .title_bar(true)
        .current_pos(pos)
        .default_width(panel_width)
        .show(ctx, |ui| {
            ui.heading(&summary.title);

            ui.horizontal(|ui| {
                let kind_label = match summary.kind {
                    bmz_core::course::CourseKind::Dan => "段位",
                    bmz_core::course::CourseKind::Course => "コース",
                };
                ui.label(kind_label);
                ui.separator();
                if summary.course_failed {
                    ui.colored_label(egui::Color32::LIGHT_RED, "FAILED");
                } else if summary.course_clear {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "CLEAR");
                } else {
                    ui.colored_label(egui::Color32::LIGHT_YELLOW, "NO TROPHY");
                }
                ui.separator();
                ui.label(format!("{}/{}", summary.played_entries, summary.total_entries));
            });

            ui.separator();

            // Totals.
            let score_rate = if summary.max_ex_score > 0 {
                summary.total_ex_score as f32 / summary.max_ex_score as f32 * 100.0
            } else {
                0.0
            };
            egui::Grid::new("course_result_totals").num_columns(2).show(ui, |ui| {
                ui.label("EX SCORE");
                ui.label(format!(
                    "{} / {} ({:.2}%)",
                    summary.total_ex_score, summary.max_ex_score, score_rate
                ));
                ui.end_row();
                ui.label("NOTES");
                ui.label(format!("{}", summary.total_notes));
                ui.end_row();
                ui.label("PG / GR");
                ui.label(format!(
                    "{} / {}",
                    summary.judge_counts.pgreat, summary.judge_counts.great
                ));
                ui.end_row();
                ui.label("GD / BD / PR");
                ui.label(format!(
                    "{} / {} / {}",
                    summary.judge_counts.good, summary.judge_counts.bad, summary.judge_counts.poor,
                ));
                ui.end_row();
            });

            if !summary.trophy_results.is_empty() {
                ui.separator();
                ui.label("トロフィー");
                // `trophy_results` is built only from `definition.trophies`
                // in `ActiveCourseSession::into_result`, so it cannot show
                // a name that the course author did not declare.
                ui.horizontal_wrapped(|ui| {
                    for trophy in &summary.trophy_results {
                        let color = if trophy.achieved {
                            egui::Color32::from_rgb(255, 215, 0) // gold
                        } else {
                            egui::Color32::DARK_GRAY
                        };
                        ui.colored_label(color, &trophy.name);
                    }
                });
            }

            // BEST section: shows the highest persisted attempt for this
            // course.  Includes the current attempt if it improved the
            // record (the lookup runs after insert_course_score).
            if let Some(best) = &summary.best_score {
                ui.separator();
                ui.label("ベスト");
                let best_rate = if best.max_ex_score > 0 {
                    best.ex_score as f32 / best.max_ex_score as f32 * 100.0
                } else {
                    0.0
                };
                let is_new_record = best.ex_score == summary.total_ex_score
                    && best.max_ex_score == summary.max_ex_score
                    && !summary.course_failed;
                egui::Grid::new("course_result_best").num_columns(2).show(ui, |ui| {
                    ui.label("EX SCORE");
                    let ex_text =
                        format!("{} / {} ({:.2}%)", best.ex_score, best.max_ex_score, best_rate);
                    if is_new_record {
                        ui.colored_label(egui::Color32::from_rgb(255, 215, 0), ex_text);
                    } else {
                        ui.label(ex_text);
                    }
                    ui.end_row();
                    ui.label("CLEAR");
                    ui.label(&best.clear_type);
                    ui.end_row();
                    ui.label("MAX COMBO");
                    ui.label(format!("{}", best.max_combo));
                    ui.end_row();
                });
                if is_new_record {
                    ui.colored_label(egui::Color32::from_rgb(255, 215, 0), "★ NEW RECORD");
                }
            }

            if !summary.entry_summaries.is_empty() {
                ui.separator();
                ui.label("各曲");
                egui::Grid::new("course_result_entries").num_columns(6).striped(true).show(
                    ui,
                    |ui| {
                        // Header row.
                        ui.label("#");
                        ui.label("曲名");
                        ui.label("EX");
                        ui.label("COMBO");
                        ui.label("CLEAR");
                        ui.label("MISS");
                        ui.end_row();
                        for (i, entry) in summary.entry_summaries.iter().enumerate() {
                            ui.label(format!("{}", i + 1));
                            let title =
                                if entry.title.is_empty() { "(no title)" } else { &entry.title };
                            ui.label(title);
                            ui.label(format!("{}", entry.ex_score));
                            ui.label(format!("{}", entry.max_combo));
                            // Color the clear cell so failed entries stand out.
                            let clear_text = entry.clear_type.as_str();
                            let clear_color = match entry.clear_type {
                                bmz_core::clear::ClearType::Failed => egui::Color32::LIGHT_RED,
                                bmz_core::clear::ClearType::FullCombo
                                | bmz_core::clear::ClearType::Perfect
                                | bmz_core::clear::ClearType::Max => egui::Color32::LIGHT_GREEN,
                                _ => ui.visuals().text_color(),
                            };
                            ui.colored_label(clear_color, clear_text);
                            let miss = entry.judge_counts.bad
                                + entry.judge_counts.poor
                                + entry.judge_counts.empty_poor;
                            ui.label(format!("{}", miss));
                            ui.end_row();
                        }
                    },
                );
            }
        });
}

/// 選曲画面でコース行にカーソルがある間、コース内の各曲のメタ情報を表示する
/// プレビューパネル。
fn build_course_preview_panel(ctx: &egui::Context, preview: &SelectCourseRow) {
    let content_rect = ctx.content_rect();
    let pos = egui::pos2(16.0, content_rect.bottom() - 320.0);

    egui::Window::new("コース内訳")
        .id(egui::Id::new("course_preview_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .title_bar(true)
        .current_pos(pos)
        .default_width(380.0)
        .max_height(300.0)
        .show(ctx, |ui| {
            ui.heading(&preview.title);
            ui.horizontal(|ui| {
                ui.label(&preview.category_label);
                ui.separator();
                ui.label(format!("{}/{} resolved", preview.resolved_count, preview.entry_count));
                ui.separator();
                ui.label(format!("notes {}", preview.total_notes));
            });
            if !preview.trophy_names.is_empty() {
                ui.label(format!("trophies: {}", preview.trophy_names.join(" / ")));
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("course_preview_entries").num_columns(4).striped(true).show(
                    ui,
                    |ui| {
                        ui.label("#");
                        ui.label("曲名");
                        ui.label("☆");
                        ui.label("notes");
                        ui.end_row();
                        for (i, entry) in preview.entry_previews.iter().enumerate() {
                            ui.label(format!("{}", i + 1));
                            let title =
                                if entry.title.is_empty() { "(no title)" } else { &entry.title };
                            if entry.resolved {
                                ui.label(title);
                            } else {
                                ui.colored_label(
                                    egui::Color32::GRAY,
                                    format!("{} (missing)", title),
                                );
                            }
                            ui.label(&entry.play_level);
                            ui.label(format!("{}", entry.total_notes));
                            ui.end_row();
                        }
                    },
                );
            });
        });
}

/// FPS / フレーム時間 / シーン / 解像度を表示するデバッグパネル。
fn build_debug_panel(ctx: &egui::Context, open: &mut bool, info: &DebugInfo) {
    sized_panel_window("デバッグ表示", ctx, open, 320.0, 200.0, egui::pos2(16.0, 140.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
                let dt = ctx.input(|i| i.stable_dt);
                let fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
                egui::Grid::new("debug_grid").num_columns(2).show(ui, |ui| {
                    ui.label("FPS");
                    ui.label(format!("{fps:.1}"));
                    ui.end_row();
                    ui.label("フレーム時間");
                    ui.label(format!("{:.2} ms", dt * 1000.0));
                    ui.end_row();
                    ui.label("シーン");
                    ui.label(info.scene);
                    ui.end_row();
                    ui.label("解像度");
                    ui.label(format!("{} x {}", info.width, info.height));
                    ui.end_row();
                });
            });
        },
    );
}

/// 本体設定パネルからのアクション要求。
struct SettingsPanelActions {
    save: bool,
    rescan: bool,
}

/// `AppConfig` を編集する本体設定パネル。
fn build_settings_panel(
    ctx: &egui::Context,
    open: &mut bool,
    config: &mut AppConfig,
    new_root_path: &mut String,
    add_root_error: &mut String,
) -> SettingsPanelActions {
    let mut save_clicked = false;
    let mut rescan_clicked = false;
    sized_panel_window("本体設定", ctx, open, 440.0, 520.0, egui::pos2(16.0, 320.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
                egui::CollapsingHeader::new("曲フォルダ (BMS)")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove_index = None;
                        for (index, root) in config.songs.roots.iter_mut().enumerate() {
                            ui.push_id(index, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(&root.path);
                                    if ui.button("削除").clicked() {
                                        remove_index = Some(index);
                                    }
                                });
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut root.enabled, "有効");
                                    ui.checkbox(&mut root.recursive, "再帰スキャン");
                                });
                                ui.separator();
                            });
                        }
                        if let Some(index) = remove_index {
                            remove_song_root_entry(&mut config.songs.roots, index);
                        }
                        if config.songs.roots.is_empty() {
                            ui.label("登録された曲フォルダはありません。");
                        }
                        ui.horizontal(|ui| {
                            ui.label("パス");
                            ui.add(
                                egui::TextEdit::singleline(new_root_path)
                                    .desired_width(240.0)
                                    .hint_text("/path/to/bms"),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui.button("フォルダを選択…").clicked()
                                && let Some(folder) = rfd::FileDialog::new().pick_folder()
                            {
                                *new_root_path = folder.to_string_lossy().into_owned();
                                add_root_error.clear();
                            }
                            if ui.button("追加").clicked() {
                                let path = new_root_path.trim();
                                if path.is_empty() {
                                    *add_root_error =
                                        "パスを入力するかフォルダを選択してください。".to_string();
                                } else {
                                    match add_song_root_entry(
                                        &mut config.songs.roots,
                                        path,
                                        true,
                                        true,
                                    ) {
                                        Ok(()) => {
                                            new_root_path.clear();
                                            add_root_error.clear();
                                        }
                                        Err(error) => *add_root_error = error.to_string(),
                                    }
                                }
                            }
                        });
                        if !add_root_error.is_empty() {
                            ui.colored_label(egui::Color32::RED, add_root_error.as_str());
                        }
                        if ui.button("ライブラリを再スキャン").clicked() {
                            rescan_clicked = true;
                        }
                        ui.label("再スキャンは有効なルートを対象に library.db を更新します。");
                    });

                egui::CollapsingHeader::new("スキャン").show(ui, |ui| {
                    ui.checkbox(&mut config.scan.follow_symlinks, "シンボリックリンクを辿る");
                    ui.checkbox(&mut config.scan.skip_hidden, "隠しファイル / フォルダをスキップ");
                    ui.checkbox(
                        &mut config.scan.auto_rescan_on_startup,
                        "起動時に自動再スキャン",
                    );
                    ui.checkbox(
                        &mut config.scan.rescan_missing_files,
                        "存在しないファイルを DB から除去 (未実装)",
                    );
                });

                egui::CollapsingHeader::new("音声").show(ui, |ui| {
                    egui::ComboBox::from_label("バックエンド")
                        .selected_text(audio_backend_label(&config.audio.backend))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Auto,
                                "自動選択",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::CoreAudio,
                                "Core Audio",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Wasapi,
                                "WASAPI",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Asio,
                                "ASIO",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Alsa,
                                "ALSA",
                            );
                            ui.selectable_value(
                                &mut config.audio.backend,
                                AudioBackend::Pulse,
                                "PulseAudio",
                            );
                        });
                    ui.add(
                        egui::Slider::new(&mut config.audio.sample_rate, 44_100..=96_000)
                            .text("サンプルレート"),
                    );
                    egui::ComboBox::from_label("バッファサイズモード")
                        .selected_text(audio_buffer_size_mode_label(&config.audio.buffer_size_mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.audio.buffer_size_mode,
                                AudioBufferSizeMode::Auto,
                                "自動",
                            );
                            ui.selectable_value(
                                &mut config.audio.buffer_size_mode,
                                AudioBufferSizeMode::Fixed,
                                "固定",
                            );
                        });
                    ui.add(
                        egui::Slider::new(&mut config.audio.buffer_size, 64..=4096)
                            .text("バッファサイズ (フレーム)"),
                    );
                    ui.checkbox(&mut config.audio.exclusive_mode, "排他モード");
                    ui.add_enabled_ui(false, |ui| {
                        ui.label(format!(
                            "出力デバイス (未実装): {}",
                            if config.audio.output_device.is_empty() {
                                "(デフォルト)"
                            } else {
                                config.audio.output_device.as_str()
                            }
                        ));
                        ui.label(format!(
                            "ASIO ドライバ (未実装): {}",
                            if config.audio.asio_driver.is_empty() {
                                "(未指定)"
                            } else {
                                config.audio.asio_driver.as_str()
                            }
                        ));
                    });
                    ui.label("音声設定は次回起動時に反映されます。");
                });

                egui::CollapsingHeader::new("映像").show(ui, |ui| {
                    egui::ComboBox::from_label("ウィンドウモード")
                        .selected_text(window_mode_label(&config.video.mode))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.video.mode,
                                WindowMode::Windowed,
                                "ウィンドウ",
                            );
                            ui.selectable_value(
                                &mut config.video.mode,
                                WindowMode::BorderlessFullscreen,
                                "ボーダレスフルスクリーン",
                            );
                            ui.selectable_value(
                                &mut config.video.mode,
                                WindowMode::ExclusiveFullscreen,
                                "排他フルスクリーン",
                            );
                        });
                    ui.add(
                        egui::Slider::new(&mut config.video.width, 640..=3840).text("幅 (px)"),
                    );
                    ui.add(
                        egui::Slider::new(&mut config.video.height, 480..=2160).text("高さ (px)"),
                    );
                    ui.checkbox(&mut config.video.vsync, "垂直同期 (VSync)");
                    ui.add(
                        egui::Slider::new(&mut config.video.target_fps, 30..=480).text("目標 FPS"),
                    );
                    ui.add(
                        egui::Slider::new(&mut config.video.frame_limit_in_background, 1..=120)
                            .text("バックグラウンド FPS 上限"),
                    );
                    egui::ComboBox::from_label("レンダリングバックエンド")
                        .selected_text(renderer_backend_label(&config.video.renderer))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Auto,
                                "自動選択",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Vulkan,
                                "Vulkan",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Metal,
                                "Metal",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Dx12,
                                "DirectX 12",
                            );
                            ui.selectable_value(
                                &mut config.video.renderer,
                                RendererBackend::Gl,
                                "OpenGL",
                            );
                        });
                    ui.label(
                        "VSync / ウィンドウモードは即時反映。幅 / 高さ / 目標 FPS / レンダリングバックエンドは次回起動時に反映されます。",
                    );
                });

                ui.separator();
                if ui.button("保存").clicked() {
                    save_clicked = true;
                }
            });
        });
    SettingsPanelActions { save: save_clicked, rescan: rescan_clicked }
}

fn audio_backend_label(backend: &AudioBackend) -> &'static str {
    match backend {
        AudioBackend::Auto => "自動選択",
        AudioBackend::Wasapi => "WASAPI",
        AudioBackend::Asio => "ASIO",
        AudioBackend::CoreAudio => "Core Audio",
        AudioBackend::Alsa => "ALSA",
        AudioBackend::Pulse => "PulseAudio",
    }
}

fn audio_buffer_size_mode_label(mode: &AudioBufferSizeMode) -> &'static str {
    match mode {
        AudioBufferSizeMode::Auto => "自動",
        AudioBufferSizeMode::Fixed => "固定",
    }
}

fn window_mode_label(mode: &WindowMode) -> &'static str {
    match mode {
        WindowMode::Windowed => "ウィンドウ",
        WindowMode::BorderlessFullscreen => "ボーダレスフルスクリーン",
        WindowMode::ExclusiveFullscreen => "排他フルスクリーン",
    }
}

fn renderer_backend_label(backend: &RendererBackend) -> &'static str {
    match backend {
        RendererBackend::Auto => "自動選択",
        RendererBackend::Vulkan => "Vulkan",
        RendererBackend::Metal => "Metal",
        RendererBackend::Dx12 => "DirectX 12",
        RendererBackend::Gl => "OpenGL",
    }
}

/// スキン設定パネルからのアクション要求。
struct SkinPanelActions {
    /// 「保存」ボタンが押された (profile.toml へ書き出し)。
    save: bool,
    /// 「リセット」ボタンが押された (profile.toml の値へ戻す)。
    reset: bool,
    /// パネル内のスキン設定が変更された。
    changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkinSlot {
    Select,
    Decide,
    Play5,
    Play7,
    Play10,
    Play14,
    Result,
}

fn skin_path_combo(
    ui: &mut egui::Ui,
    skin: &mut SkinConfig,
    slot: SkinSlot,
    label: &str,
    candidates: &[SkinCandidate],
) -> bool {
    ui.label(label);
    let current = skin_slot_path(skin, slot).to_string();
    let mut selected = current.clone();
    let selected_text = skin_candidate_label(candidates, &current);
    egui::ComboBox::from_id_salt(("skin_path_combo", label))
        .selected_text(selected_text)
        .width(320.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut selected, String::new(), "(デフォルト)");
            for candidate in candidates {
                ui.selectable_value(
                    &mut selected,
                    candidate.path.clone(),
                    skin_candidate_display(candidate),
                );
            }
        });
    let combo_changed = selected != current;
    if combo_changed {
        save_skin_slot_history(skin, slot);
        *skin_slot_path_mut(skin, slot) = selected;
        restore_skin_slot_history(skin, slot);
    }
    let mut edited_path = skin_slot_path(skin, slot).to_string();
    let text_changed = ui.text_edit_singleline(&mut edited_path).changed();
    if text_changed {
        save_skin_slot_history(skin, slot);
        *skin_slot_path_mut(skin, slot) = edited_path;
        restore_skin_slot_history(skin, slot);
    }
    combo_changed || text_changed
}

fn skin_candidate_label(candidates: &[SkinCandidate], current: &str) -> String {
    if current.is_empty() {
        return "(デフォルト)".to_string();
    }
    candidates
        .iter()
        .find(|candidate| candidate.path == current)
        .map(skin_candidate_display)
        .unwrap_or_else(|| current.to_string())
}

fn skin_candidate_display(candidate: &SkinCandidate) -> String {
    if candidate.name.is_empty() {
        candidate.path.clone()
    } else {
        format!("{} ({})", candidate.name, candidate.path)
    }
}

fn skin_slot_path(skin: &SkinConfig, slot: SkinSlot) -> &str {
    match slot {
        SkinSlot::Select => &skin.select,
        SkinSlot::Decide => &skin.decide,
        SkinSlot::Play5 => &skin.play5,
        SkinSlot::Play7 => &skin.play7,
        SkinSlot::Play10 => &skin.play10,
        SkinSlot::Play14 => &skin.play14,
        SkinSlot::Result => &skin.result,
    }
}

fn skin_slot_path_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut String {
    match slot {
        SkinSlot::Select => &mut skin.select,
        SkinSlot::Decide => &mut skin.decide,
        SkinSlot::Play5 => &mut skin.play5,
        SkinSlot::Play7 => &mut skin.play7,
        SkinSlot::Play10 => &mut skin.play10,
        SkinSlot::Play14 => &mut skin.play14,
        SkinSlot::Result => &mut skin.result,
    }
}

fn skin_slot_options_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_options,
        SkinSlot::Decide => &mut skin.decide_options,
        SkinSlot::Play5 => &mut skin.play5_options,
        SkinSlot::Play7 => &mut skin.play7_options,
        SkinSlot::Play10 => &mut skin.play10_options,
        SkinSlot::Play14 => &mut skin.play14_options,
        SkinSlot::Result => &mut skin.result_options,
    }
}

fn skin_slot_files_mut(skin: &mut SkinConfig, slot: SkinSlot) -> &mut BTreeMap<String, String> {
    match slot {
        SkinSlot::Select => &mut skin.select_files,
        SkinSlot::Decide => &mut skin.decide_files,
        SkinSlot::Play5 => &mut skin.play5_files,
        SkinSlot::Play7 => &mut skin.play7_files,
        SkinSlot::Play10 => &mut skin.play10_files,
        SkinSlot::Play14 => &mut skin.play14_files,
        SkinSlot::Result => &mut skin.result_files,
    }
}

fn save_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot) {
    let path = skin_slot_path(skin, slot).trim().to_string();
    if path.is_empty() {
        return;
    }
    let options = skin_slot_options_mut(skin, slot).clone();
    let files = skin_slot_files_mut(skin, slot).clone();
    skin.history
        .insert(path, SkinHistoryEntryConfig { options, files, offsets: skin.offsets.clone() });
}

fn restore_skin_slot_history(skin: &mut SkinConfig, slot: SkinSlot) {
    let path = skin_slot_path(skin, slot).trim().to_string();
    let Some(entry) = skin.history.get(&path).cloned() else {
        skin_slot_options_mut(skin, slot).clear();
        skin_slot_files_mut(skin, slot).clear();
        skin.offsets.clear();
        return;
    };
    *skin_slot_options_mut(skin, slot) = entry.options;
    *skin_slot_files_mut(skin, slot) = entry.files;
    skin.offsets = entry.offsets;
}

/// プロファイルのスキン設定 (`SkinConfig`) を編集するパネル。
fn build_skin_panel(
    ctx: &egui::Context,
    open: &mut bool,
    skin: &mut SkinConfig,
    skin_meta: &SkinConfigMeta,
    skin_catalog: &SkinCatalog,
) -> SkinPanelActions {
    let mut save_clicked = false;
    let mut reset_clicked = false;
    let mut changed = false;
    sized_panel_window("スキン設定", ctx, open, 440.0, 560.0, egui::pos2(16.0, 480.0)).show(
        ctx,
        |ui| {
            scrollable_window_content(ui, |ui| {
            ui.label("各画面のスキン。空欄なら内蔵描画 / デフォルトスキンを使用します。");
            egui::Grid::new("skin_grid").num_columns(2).show(ui, |ui| {
                changed |=
                    skin_path_combo(ui, skin, SkinSlot::Select, "選曲", &skin_catalog.select);
                ui.end_row();
                changed |=
                    skin_path_combo(ui, skin, SkinSlot::Decide, "決定", &skin_catalog.decide);
                ui.end_row();
                changed |=
                    skin_path_combo(ui, skin, SkinSlot::Play5, "プレイ (5K)", &skin_catalog.play5);
                ui.end_row();
                changed |=
                    skin_path_combo(ui, skin, SkinSlot::Play7, "プレイ (7K)", &skin_catalog.play7);
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play10,
                    "プレイ (10K)",
                    &skin_catalog.play10,
                );
                ui.end_row();
                changed |= skin_path_combo(
                    ui,
                    skin,
                    SkinSlot::Play14,
                    "プレイ (14K)",
                    &skin_catalog.play14,
                );
                ui.end_row();
                changed |=
                    skin_path_combo(ui, skin, SkinSlot::Result, "リザルト", &skin_catalog.result);
                ui.end_row();
            });
            ui.separator();
            ui.label("読み込み済みスキンが宣言する設定可能項目:");
            let select_root = skin_root_path(&skin.select);
            let decide_root = skin_root_path(&skin.decide);
            let play5_root = skin_root_path(&skin.play5);
            let play7_root = skin_root_path(&skin.play7);
            let play10_root = skin_root_path(&skin.play10);
            let play14_root = skin_root_path(&skin.play14);
            let result_root = skin_root_path(&skin.result);
            changed |= build_scene_skin_defs(
                ui,
                "選曲スキン",
                &skin_meta.select,
                select_root.as_deref(),
                &mut skin.select_options,
                &mut skin.select_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "決定スキン",
                &skin_meta.decide,
                decide_root.as_deref(),
                &mut skin.decide_options,
                &mut skin.decide_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (5K)",
                &skin_meta.play5,
                play5_root.as_deref(),
                &mut skin.play5_options,
                &mut skin.play5_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (7K)",
                &skin_meta.play7,
                play7_root.as_deref(),
                &mut skin.play7_options,
                &mut skin.play7_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (10K)",
                &skin_meta.play10,
                play10_root.as_deref(),
                &mut skin.play10_options,
                &mut skin.play10_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "プレイスキン (14K)",
                &skin_meta.play14,
                play14_root.as_deref(),
                &mut skin.play14_options,
                &mut skin.play14_files,
                &mut skin.offsets,
            );
            changed |= build_scene_skin_defs(
                ui,
                "リザルトスキン",
                &skin_meta.result,
                result_root.as_deref(),
                &mut skin.result_options,
                &mut skin.result_files,
                &mut skin.offsets,
            );
            ui.separator();
            ui.label(
                "「保存」で profile.toml へ書き出し。「リセット」で保存済みの設定へ戻します。オプションの「デフォルトに戻す」は保存までディスクへ書きません。",
            );
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save_clicked = true;
                }
                if ui.button("リセット").clicked() {
                    reset_clicked = true;
                }
            });
            });
        },
    );
    SkinPanelActions { save: save_clicked, reset: reset_clicked, changed }
}

/// 1 シーン分のスキン設定可能項目を折りたたみ表示・編集する。
///
/// - property: ComboBox で選択肢を選び `options` へ書き込む。
/// - filepath: `path` グロブにマッチするファイルを ComboBox で選び `files` へ書き込む。
/// - offset: 宣言された要素ごとに x/y/w/h/r/a を編集し `offsets` (id 単位) へ反映。
fn build_scene_skin_defs(
    ui: &mut egui::Ui,
    label: &str,
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(label).show(ui, |ui| {
        if defs.is_empty() {
            ui.label("設定可能項目はありません (スキン未読込、または定義なし)。");
            return;
        }
        changed |= fill_missing_skin_defaults(defs, skin_root, options, files);
        if !defs.property.is_empty() {
            ui.strong("オプション");
            // property / filepath は同名 (例: "シャッター") を持ちうるので、egui の
            // ComboBox ID 衝突を防ぐためにカテゴリで名前空間を切る。
            ui.push_id("property", |ui| {
                for prop in &defs.property {
                    let mut selected =
                        options.get(&prop.name).cloned().unwrap_or_else(|| property_default(prop));
                    let before = selected.clone();
                    egui::ComboBox::from_label(&prop.name).selected_text(&selected).show_ui(
                        ui,
                        |ui| {
                            for item in &prop.item {
                                ui.selectable_value(&mut selected, item.name.clone(), &item.name);
                            }
                        },
                    );
                    if selected != before {
                        options.insert(prop.name.clone(), selected);
                        changed = true;
                    }
                }
            });
        }
        if !defs.filepath.is_empty() {
            ui.strong("ファイル選択");
            ui.push_id("filepath", |ui| {
                for filepath in &defs.filepath {
                    let mut selected = files.get(&filepath.name).cloned().unwrap_or_default();
                    let before = selected.clone();
                    let display =
                        if selected.is_empty() { "(未選択)" } else { selected.as_str() };
                    egui::ComboBox::from_label(&filepath.name).selected_text(display).show_ui(
                        ui,
                        |ui| {
                            // 候補列挙は ComboBox を開いたときだけ行う (毎フレームの fs 走査を回避)。
                            let candidates = match skin_root {
                                Some(root) => glob_candidates(root, &filepath.path),
                                None => Vec::new(),
                            };
                            if candidates.is_empty() {
                                ui.label("候補なし");
                            }
                            for candidate in candidates {
                                ui.selectable_value(&mut selected, candidate.clone(), &candidate);
                            }
                        },
                    );
                    if selected != before {
                        files.insert(filepath.name.clone(), selected);
                        changed = true;
                    }
                }
            });
        }
        if !defs.offset.is_empty() {
            ui.strong("オフセット可能要素");
            for offset_def in &defs.offset {
                ui.push_id(offset_def.id, |ui| {
                    ui.label(format!(
                        "{} [{}] — id {}",
                        offset_def.name, offset_def.category, offset_def.id
                    ));
                    let existing = offsets.iter().find(|o| o.id == offset_def.id).copied();
                    let mut value = existing
                        .unwrap_or(SkinOffsetConfig { id: offset_def.id, ..Default::default() });
                    let before = value;
                    ui.horizontal(|ui| {
                        changed |= add_offset_drag_values(ui, offset_def, &mut value);
                    });
                    if value != before {
                        match offsets.iter_mut().find(|o| o.id == offset_def.id) {
                            Some(entry) => *entry = value,
                            None => offsets.push(value),
                        }
                        changed = true;
                    }
                });
            }
        }
        if !defs.is_empty() && ui.button("デフォルトに戻す").clicked() {
            changed |= reset_scene_skin_to_defaults(defs, skin_root, options, files, offsets);
        }
    });
    changed
}

/// 1 シーン分の options / files / 当該 offset id をスキン定義の factory default へ戻す。
fn reset_scene_skin_to_defaults(
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
    offsets: &mut Vec<SkinOffsetConfig>,
) -> bool {
    if defs.is_empty() {
        return false;
    }
    options.clear();
    files.clear();
    let scene_offset_ids: std::collections::HashSet<i32> =
        defs.offset.iter().map(|offset| offset.id).collect();
    offsets.retain(|offset| !scene_offset_ids.contains(&offset.id));
    fill_missing_skin_defaults(defs, skin_root, options, files)
}

fn fill_missing_skin_defaults(
    defs: &SceneSkinDefs,
    skin_root: Option<&Path>,
    options: &mut BTreeMap<String, String>,
    files: &mut BTreeMap<String, String>,
) -> bool {
    let mut changed = false;
    for prop in &defs.property {
        if !options.contains_key(&prop.name) {
            options.insert(prop.name.clone(), property_default(prop));
            changed = true;
        }
    }
    let Some(skin_root) = skin_root else {
        return changed;
    };
    for filepath in &defs.filepath {
        if files.contains_key(&filepath.name) {
            continue;
        }
        let candidates = glob_candidates(skin_root, &filepath.path);
        if let Some(default) = filepath_default(filepath, &candidates) {
            files.insert(filepath.name.clone(), default);
            changed = true;
        }
    }
    changed
}

fn add_offset_drag_values(
    ui: &mut egui::Ui,
    def: &SkinOffsetDef,
    value: &mut SkinOffsetConfig,
) -> bool {
    let mut changed = false;
    let mut any = false;
    if def.x {
        changed |= ui.add(egui::DragValue::new(&mut value.x).prefix("x:")).changed();
        any = true;
    }
    if def.y {
        changed |= ui.add(egui::DragValue::new(&mut value.y).prefix("y:")).changed();
        any = true;
    }
    if def.w {
        changed |= ui.add(egui::DragValue::new(&mut value.w).prefix("w:")).changed();
        any = true;
    }
    if def.h {
        changed |= ui.add(egui::DragValue::new(&mut value.h).prefix("h:")).changed();
        any = true;
    }
    if def.r {
        changed |= ui.add(egui::DragValue::new(&mut value.r).prefix("r:")).changed();
        any = true;
    }
    if def.a {
        changed |= ui.add(egui::DragValue::new(&mut value.a).prefix("a:")).changed();
        any = true;
    }
    if !any {
        ui.label("調整可能な値はありません");
    }
    changed
}

/// スキンパス文字列からスキンルートディレクトリ (親ディレクトリ) を得る。
fn skin_root_path(skin_path: &str) -> Option<PathBuf> {
    let trimmed = skin_path.trim();
    if trimmed.is_empty() {
        return None;
    }
    Path::new(trimmed).parent().map(Path::to_path_buf)
}

/// `pattern` (スキンルート相対、末尾要素にワイルドカード `*` を 1 個まで) に
/// マッチするファイルの相対パス一覧を返す。
///
/// beatoraja の `path|filter|` 形式の `|...|` 接尾辞 (lanecover などの
/// アセット用途タグ) は対象ファイル名には含まれないので、列挙前に取り除く。
fn glob_candidates(root: &Path, pattern: &str) -> Vec<String> {
    let pattern = pattern.replace('\\', "/");
    let pattern = pattern.split_once('|').map_or(pattern.as_str(), |(path, _)| path).to_string();
    let (dir_part, name_part) = match pattern.rfind('/') {
        Some(index) => (&pattern[..=index], &pattern[index + 1..]),
        None => ("", pattern.as_str()),
    };
    let Some((prefix, suffix)) = name_part.split_once('*') else {
        // ワイルドカード無し: パターンそのものを唯一の候補とする。
        return vec![pattern.clone()];
    };
    let dir = root.join(dir_part);
    let mut candidates = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
            {
                candidates.push(format!("{dir_part}{name}"));
            }
        }
    }
    candidates.sort();
    candidates
}

/// property の既定選択肢名。beatoraja と同じく `def` が item name と一致する
/// ときだけ採用し、未指定/不一致なら先頭 item を使う。
fn property_default(prop: &SkinPropertyDef) -> String {
    prop.item
        .iter()
        .find(|item| !prop.def.is_empty() && item.name == prop.def)
        .or_else(|| prop.item.first())
        .map(|item| item.name.clone())
        .unwrap_or_default()
}

fn filepath_default(filepath: &SkinFilepathDef, candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    if !filepath.def.is_empty()
        && let Some(candidate) =
            candidates.iter().find(|candidate| filename_matches_def(candidate, &filepath.def))
    {
        return Some(candidate.clone());
    }
    candidates.first().cloned()
}

fn filename_matches_def(candidate: &str, def: &str) -> bool {
    let file_name = Path::new(candidate).file_name().and_then(|name| name.to_str()).unwrap_or("");
    if file_name.eq_ignore_ascii_case(def) {
        return true;
    }
    Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case(def))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{name}-{nanos}-{counter}"))
    }

    #[test]
    fn clamp_panel_layout_fits_high_dpi_1920x1080_logical_viewport() {
        // 1920x1080 物理ウィンドウ @ 2x → egui 論理 960x540 相当。
        let constrain = egui::Rect::from_min_size(egui::pos2(16.0, 16.0), egui::vec2(928.0, 508.0));
        // egui 0.34 既定 style 付近の chrome 高さ (frame + title bar)。
        let chrome = egui::vec2(12.0, 58.0);
        let (default_inner, max_inner, pos) =
            clamp_panel_layout(constrain, chrome, 440.0, 560.0, egui::pos2(16.0, 480.0));

        let outer = default_inner + chrome;
        assert!(outer.x <= constrain.width() + 0.01);
        assert!(outer.y <= constrain.height() + 0.01);
        assert!(pos.x + outer.x <= constrain.max.x + 0.01);
        assert!(pos.y + outer.y <= constrain.max.y + 0.01);
        assert_eq!(pos, egui::pos2(16.0, 16.0));
        assert!(default_inner.y < 560.0);
        assert_eq!(max_inner, egui::vec2(916.0, 450.0));
    }

    #[test]
    fn clamp_panel_layout_keeps_preferred_size_on_large_viewport() {
        let constrain =
            egui::Rect::from_min_size(egui::pos2(16.0, 16.0), egui::vec2(1888.0, 1048.0));
        let chrome = egui::vec2(12.0, 58.0);
        let (default_inner, max_inner, pos) =
            clamp_panel_layout(constrain, chrome, 440.0, 560.0, egui::pos2(16.0, 480.0));

        assert_eq!(default_inner, egui::vec2(440.0, 560.0));
        assert_eq!(max_inner, egui::vec2(1876.0, 990.0));
        // outer 高さ 618 のため y=480 では下端がはみ出す → 446 へクランプ。
        assert_eq!(pos, egui::pos2(16.0, 446.0));
    }

    #[test]
    fn glob_candidates_lists_files_matching_simple_pattern() {
        let root = unique_test_dir("bmz-ui-glob");
        fs::create_dir_all(root.join("parts")).unwrap();
        fs::write(root.join("parts/a.png"), []).unwrap();
        fs::write(root.join("parts/b.png"), []).unwrap();
        fs::write(root.join("parts/c.txt"), []).unwrap();

        let candidates = glob_candidates(&root, "parts/*.png");

        assert_eq!(candidates, vec!["parts/a.png".to_string(), "parts/b.png".to_string()]);
    }

    #[test]
    fn glob_candidates_strips_beatoraja_filter_suffix() {
        let root = unique_test_dir("bmz-ui-glob");
        fs::create_dir_all(root.join("parts/lanecover_lift")).unwrap();
        fs::write(root.join("parts/lanecover_lift/default.png"), []).unwrap();
        fs::write(root.join("parts/lanecover_lift/TYPE-M.png"), []).unwrap();

        let candidates = glob_candidates(&root, "parts/lanecover_lift/*.png|lanecover|");

        assert_eq!(
            candidates,
            vec![
                "parts/lanecover_lift/TYPE-M.png".to_string(),
                "parts/lanecover_lift/default.png".to_string(),
            ]
        );
    }

    #[test]
    fn property_default_uses_matching_def_name_or_first_item() {
        let prop = SkinPropertyDef {
            category: String::new(),
            name: "Notes".to_string(),
            item: vec![
                bmz_render::skin::SkinPropertyItemDef { name: "Light".to_string(), op: 1 },
                bmz_render::skin::SkinPropertyItemDef { name: "Dark".to_string(), op: 2 },
            ],
            def: "Dark".to_string(),
        };
        assert_eq!(property_default(&prop), "Dark");

        let prop = SkinPropertyDef { def: "Missing".to_string(), ..prop };
        assert_eq!(property_default(&prop), "Light");
    }

    #[test]
    fn filepath_default_matches_def_with_or_without_extension_case_insensitive() {
        let filepath = SkinFilepathDef {
            category: String::new(),
            name: "Notes".to_string(),
            path: "notes/*.png".to_string(),
            def: "default".to_string(),
        };
        let candidates = vec!["notes/aaa.png".to_string(), "notes/Default.PNG".to_string()];

        assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("notes/Default.PNG"));

        let filepath = SkinFilepathDef { def: "missing".to_string(), ..filepath };
        assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("notes/aaa.png"));
    }

    #[test]
    fn fill_missing_skin_defaults_keeps_saved_values_and_fills_new_items() {
        let root = unique_test_dir("bmz-ui-defaults");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/aaa.png"), []).unwrap();
        fs::write(root.join("notes/default.png"), []).unwrap();
        let defs = SceneSkinDefs {
            property: vec![
                SkinPropertyDef {
                    category: String::new(),
                    name: "Lane".to_string(),
                    item: vec![
                        bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 0 },
                        bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 1 },
                    ],
                    def: "On".to_string(),
                },
                SkinPropertyDef {
                    category: String::new(),
                    name: "Saved".to_string(),
                    item: vec![
                        bmz_render::skin::SkinPropertyItemDef { name: "A".to_string(), op: 0 },
                        bmz_render::skin::SkinPropertyItemDef { name: "B".to_string(), op: 1 },
                    ],
                    def: "A".to_string(),
                },
            ],
            filepath: vec![SkinFilepathDef {
                category: String::new(),
                name: "Notes".to_string(),
                path: "notes/*.png".to_string(),
                def: "default".to_string(),
            }],
            offset: Vec::new(),
        };
        let mut options = BTreeMap::from([("Saved".to_string(), "B".to_string())]);
        let mut files = BTreeMap::new();

        assert!(fill_missing_skin_defaults(&defs, Some(&root), &mut options, &mut files));

        assert_eq!(options.get("Lane").map(String::as_str), Some("On"));
        assert_eq!(options.get("Saved").map(String::as_str), Some("B"));
        assert_eq!(files.get("Notes").map(String::as_str), Some("notes/default.png"));
    }

    #[test]
    fn play_skin_defs_include_beatoraja_common_offsets() {
        let defs = SceneSkinDefs::from_play_document(None);

        let offsets: Vec<_> =
            defs.offset.iter().map(|offset| (offset.id, offset.name.as_str())).collect();
        assert!(offsets.contains(&(10, "All offset(%)")));
        assert!(offsets.contains(&(30, "Notes offset")));
        assert!(offsets.contains(&(32, "Judge offset")));
        assert!(offsets.contains(&(33, "Judge Detail offset")));
        assert!(offsets.contains(&(SKIN_OFFSET_BAR_LINE, "Bar Line offset")));
    }

    #[test]
    fn play_skin_defs_do_not_duplicate_existing_common_offset_ids() {
        let mut defs = SceneSkinDefs::default();
        defs.offset.push(SkinOffsetDef {
            category: "custom".to_string(),
            name: "Custom all".to_string(),
            id: 10,
            x: true,
            y: true,
            w: false,
            h: false,
            r: false,
            a: false,
        });

        defs.append_missing_beatoraja_play_offsets();

        assert_eq!(defs.offset.iter().filter(|offset| offset.id == 10).count(), 1);
        assert_eq!(defs.offset.len(), 5);
    }

    #[test]
    fn play_skin_defs_enable_bar_line_alpha_when_skin_def_disables_it() {
        let mut defs = SceneSkinDefs::default();
        defs.offset.push(SkinOffsetDef {
            category: "custom".to_string(),
            name: "Custom bar".to_string(),
            id: SKIN_OFFSET_BAR_LINE,
            x: false,
            y: false,
            w: false,
            h: true,
            r: false,
            a: false,
        });

        defs.append_missing_beatoraja_play_offsets();

        let bar_line = defs
            .offset
            .iter()
            .find(|offset| offset.id == SKIN_OFFSET_BAR_LINE)
            .expect("bar line offset def");
        assert!(bar_line.a);
    }

    #[test]
    fn reset_scene_skin_to_defaults_clears_saved_values_and_restores_factory_defaults() {
        let root = unique_test_dir("bmz-ui-reset-scene");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/aaa.png"), []).unwrap();
        fs::write(root.join("notes/default.png"), []).unwrap();
        let defs = SceneSkinDefs {
            property: vec![SkinPropertyDef {
                category: String::new(),
                name: "Lane".to_string(),
                item: vec![
                    bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 0 },
                    bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 1 },
                ],
                def: "On".to_string(),
            }],
            filepath: vec![SkinFilepathDef {
                category: String::new(),
                name: "Notes".to_string(),
                path: "notes/*.png".to_string(),
                def: "default".to_string(),
            }],
            offset: vec![SkinOffsetDef {
                category: "test".to_string(),
                name: "Judge".to_string(),
                id: 32,
                x: true,
                y: true,
                w: false,
                h: false,
                r: false,
                a: false,
            }],
        };
        let mut options = BTreeMap::from([("Lane".to_string(), "Off".to_string())]);
        let mut files = BTreeMap::from([("Notes".to_string(), "notes/aaa.png".to_string())]);
        let mut offsets = vec![SkinOffsetConfig { id: 32, x: 99, ..Default::default() }];

        assert!(reset_scene_skin_to_defaults(
            &defs,
            Some(&root),
            &mut options,
            &mut files,
            &mut offsets
        ));

        assert_eq!(options.get("Lane").map(String::as_str), Some("On"));
        assert_eq!(files.get("Notes").map(String::as_str), Some("notes/default.png"));
        assert!(offsets.is_empty());
    }

    #[test]
    fn skin_slot_history_restores_options_files_and_offsets_by_path() {
        let mut skin = SkinConfig {
            play7: "data/skins/ECFN/play/play7.luaskin".to_string(),
            offsets: vec![SkinOffsetConfig { id: 32, x: 12, ..Default::default() }],
            ..SkinConfig::default()
        };
        skin.play7_options.insert("Judge".to_string(), "On".to_string());
        skin.play7_files.insert("Notes".to_string(), "notes/default.png".to_string());

        save_skin_slot_history(&mut skin, SkinSlot::Play7);
        skin.play7 = "data/skins/Starseeker/play/play7.luaskin".to_string();
        skin.play7_options.insert("Judge".to_string(), "Off".to_string());
        skin.play7_files.insert("Notes".to_string(), "notes/other.png".to_string());
        skin.offsets = vec![SkinOffsetConfig { id: 32, x: -4, ..Default::default() }];
        save_skin_slot_history(&mut skin, SkinSlot::Play7);

        skin.play7 = "data/skins/ECFN/play/play7.luaskin".to_string();
        restore_skin_slot_history(&mut skin, SkinSlot::Play7);

        assert_eq!(skin.play7_options.get("Judge").map(String::as_str), Some("On"));
        assert_eq!(skin.play7_files.get("Notes").map(String::as_str), Some("notes/default.png"));
        assert_eq!(skin.offsets, vec![SkinOffsetConfig { id: 32, x: 12, ..Default::default() }]);
    }
}
