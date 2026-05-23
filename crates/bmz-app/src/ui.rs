//! 本体設定 / スキン設定 / デバッグ表示のための egui レイヤ。
//!
//! `egui::Context` と winit 連携状態 (`egui_winit::State`) を所有し、毎フレーム
//! UI を構築して描画プリミティブ (`EguiFrame`) を生成する。bmz-render はその
//! プリミティブをゲーム / スキン描画の上にペイントするだけにする。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bmz_render::skin::{SkinDocument, SkinFilepathDef, SkinOffsetDef, SkinPropertyDef};
use bmz_render::ui::EguiFrame;
use egui::ViewportId;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::config::app_config::{AppConfig, WindowMode};
use crate::config::profile_config::{ProfileConfig, SkinConfig, SkinOffsetConfig};

/// スキンが宣言する設定可能項目の定義 (1 シーン分)。
///
/// renderer が保持する `SkinDocument` から複製して egui パネルへ渡す。
#[derive(Default)]
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

    fn is_empty(&self) -> bool {
        self.property.is_empty() && self.filepath.is_empty() && self.offset.is_empty()
    }
}

/// 選曲 / プレイ / リザルト各スキンの設定可能項目。
#[derive(Default)]
pub struct SkinConfigMeta {
    pub select: SceneSkinDefs,
    pub decide: SceneSkinDefs,
    pub play: SceneSkinDefs,
    pub result: SceneSkinDefs,
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
    /// スキンの再読込 (現在の config パスを renderer へ再適用) が要求されたか。
    pub reload_skins: bool,
    /// デバッグ表示パネルの現在の開閉状態。
    /// profile config の `ui.show_fps` へ同期し、終了時に永続化される。
    pub debug_panel_visible: bool,
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
        Self { ctx, state, visible: false, show_debug, show_settings: false, show_skin: false }
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
    ) -> EguiOutput {
        let raw_input = self.state.take_egui_input(window);
        let ctx = self.ctx.clone();
        let visible = self.visible;
        let show_debug = &mut self.show_debug;
        let show_settings = &mut self.show_settings;
        let show_skin = &mut self.show_skin;
        let mut save_app_config = false;
        let mut save_profile_config = false;
        let mut reload_skins = false;
        let full_output = ctx.run_ui(raw_input, |ui| {
            if visible {
                let ctx = ui.ctx();
                build_menu(ctx, show_debug, show_settings, show_skin);
                build_debug_panel(ctx, show_debug, info);
                if build_settings_panel(ctx, show_settings, app_config) {
                    save_app_config = true;
                }
                let skin_actions =
                    build_skin_panel(ctx, show_skin, &mut profile_config.skin, skin_meta);
                save_profile_config |= skin_actions.save;
                reload_skins |= skin_actions.reload;
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
            reload_skins,
            debug_panel_visible: *show_debug,
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
    fonts
        .font_data
        .insert("bmz_jp".to_owned(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
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
    show_debug: &mut bool,
    show_settings: &mut bool,
    show_skin: &mut bool,
) {
    egui::Window::new("BMZ メニュー").default_pos(egui::pos2(16.0, 16.0)).show(ctx, |ui| {
        ui.label("F5 でこのメニューを開閉します。");
        ui.separator();
        ui.checkbox(show_debug, "デバッグ表示");
        ui.checkbox(show_settings, "本体設定");
        ui.checkbox(show_skin, "スキン設定");
    });
}

/// FPS / フレーム時間 / シーン / 解像度を表示するデバッグパネル。
fn build_debug_panel(ctx: &egui::Context, open: &mut bool, info: &DebugInfo) {
    egui::Window::new("デバッグ表示").open(open).default_pos(egui::pos2(16.0, 140.0)).show(
        ctx,
        |ui| {
            // egui が算出した平滑化フレーム時間から FPS を求める。
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
        },
    );
}

/// `AppConfig` の映像設定を編集する本体設定パネル。
///
/// 戻り値 `true` は「保存」ボタンが押されたことを表す。設定値は
/// `config` を直接編集し、保存はアプリ側 (`run_egui_frame`) が行う。
fn build_settings_panel(ctx: &egui::Context, open: &mut bool, config: &mut AppConfig) -> bool {
    let mut save_clicked = false;
    egui::Window::new("本体設定").open(open).default_pos(egui::pos2(16.0, 320.0)).show(ctx, |ui| {
        ui.heading("映像");
        egui::ComboBox::from_label("ウィンドウモード")
            .selected_text(window_mode_label(&config.video.mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut config.video.mode, WindowMode::Windowed, "ウィンドウ");
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
        ui.checkbox(&mut config.video.vsync, "垂直同期 (VSync)");
        ui.add(egui::Slider::new(&mut config.video.target_fps, 30..=480).text("目標 FPS"));
        ui.separator();
        ui.label("VSync / ウィンドウモードは即時反映。目標 FPS は次回起動時に反映されます。");
        if ui.button("保存").clicked() {
            save_clicked = true;
        }
    });
    save_clicked
}

fn window_mode_label(mode: &WindowMode) -> &'static str {
    match mode {
        WindowMode::Windowed => "ウィンドウ",
        WindowMode::BorderlessFullscreen => "ボーダレスフルスクリーン",
        WindowMode::ExclusiveFullscreen => "排他フルスクリーン",
    }
}

/// スキン設定パネルからのアクション要求。
struct SkinPanelActions {
    /// 「保存」ボタンが押された (profile.toml へ書き出し)。
    save: bool,
    /// 「スキン再読込」ボタンが押された (現在のパスを renderer へ再適用)。
    reload: bool,
}

/// プロファイルのスキン設定 (`SkinConfig`) を編集するパネル。
fn build_skin_panel(
    ctx: &egui::Context,
    open: &mut bool,
    skin: &mut SkinConfig,
    skin_meta: &SkinConfigMeta,
) -> SkinPanelActions {
    let mut save_clicked = false;
    let mut reload_clicked = false;
    egui::Window::new("スキン設定").open(open).default_pos(egui::pos2(16.0, 480.0)).show(
        ctx,
        |ui| {
            ui.label("各画面のスキンパス。空欄なら内蔵描画 / デフォルトスキンを使用します。");
            egui::Grid::new("skin_grid").num_columns(2).show(ui, |ui| {
                ui.label("選曲");
                ui.text_edit_singleline(&mut skin.select);
                ui.end_row();
                ui.label("決定");
                ui.text_edit_singleline(&mut skin.decide);
                ui.end_row();
                ui.label("プレイ");
                ui.text_edit_singleline(&mut skin.play);
                ui.end_row();
                ui.label("リザルト");
                ui.text_edit_singleline(&mut skin.result);
                ui.end_row();
            });
            ui.separator();
            ui.label("スキンオフセット (id ごとの位置 / サイズ / 回転 / 不透明度の補正)");
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                let mut remove_index = None;
                for (index, offset) in skin.offsets.iter_mut().enumerate() {
                    ui.push_id(index, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("ID");
                            ui.add(egui::DragValue::new(&mut offset.id));
                            if ui.button("削除").clicked() {
                                remove_index = Some(index);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut offset.x).prefix("x:"));
                            ui.add(egui::DragValue::new(&mut offset.y).prefix("y:"));
                            ui.add(egui::DragValue::new(&mut offset.w).prefix("w:"));
                            ui.add(egui::DragValue::new(&mut offset.h).prefix("h:"));
                            ui.add(egui::DragValue::new(&mut offset.r).prefix("r:"));
                            ui.add(egui::DragValue::new(&mut offset.a).prefix("a:"));
                        });
                        ui.separator();
                    });
                }
                if let Some(index) = remove_index {
                    skin.offsets.remove(index);
                }
            });
            if ui.button("オフセット追加").clicked() {
                skin.offsets.push(SkinOffsetConfig::default());
            }
            ui.separator();
            ui.label("読み込み済みスキンが宣言する設定可能項目:");
            let select_root = skin_root_path(&skin.select);
            let decide_root = skin_root_path(&skin.decide);
            let play_root = skin_root_path(&skin.play);
            let result_root = skin_root_path(&skin.result);
            // オプション数が多いとウィンドウが画面をはみ出すため、この区画は
            // スクロール可能にする。
            egui::ScrollArea::vertical().id_salt("skin_defs_scroll").max_height(280.0).show(
                ui,
                |ui| {
                    build_scene_skin_defs(
                        ui,
                        "選曲スキン",
                        &skin_meta.select,
                        select_root.as_deref(),
                        &mut skin.select_options,
                        &mut skin.select_files,
                        &mut skin.offsets,
                    );
                    build_scene_skin_defs(
                        ui,
                        "決定スキン",
                        &skin_meta.decide,
                        decide_root.as_deref(),
                        &mut skin.decide_options,
                        &mut skin.decide_files,
                        &mut skin.offsets,
                    );
                    build_scene_skin_defs(
                        ui,
                        "プレイスキン",
                        &skin_meta.play,
                        play_root.as_deref(),
                        &mut skin.play_options,
                        &mut skin.play_files,
                        &mut skin.offsets,
                    );
                    build_scene_skin_defs(
                        ui,
                        "リザルトスキン",
                        &skin_meta.result,
                        result_root.as_deref(),
                        &mut skin.result_options,
                        &mut skin.result_files,
                        &mut skin.offsets,
                    );
                },
            );
            ui.separator();
            ui.label(
                "「保存」で profile.toml へ書き出し、「スキン再読込」で現在のパスを即適用します。",
            );
            ui.horizontal(|ui| {
                if ui.button("保存").clicked() {
                    save_clicked = true;
                }
                if ui.button("スキン再読込").clicked() {
                    reload_clicked = true;
                }
            });
        },
    );
    SkinPanelActions { save: save_clicked, reload: reload_clicked }
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
) {
    egui::CollapsingHeader::new(label).show(ui, |ui| {
        if defs.is_empty() {
            ui.label("設定可能項目はありません (スキン未読込、または定義なし)。");
            return;
        }
        if !defs.property.is_empty() {
            ui.strong("オプション");
            // property / filepath は同名 (例: "シャッター") を持ちうるので、egui の
            // ComboBox ID 衝突を防ぐためにカテゴリで名前空間を切る。
            ui.push_id("property", |ui| {
                for prop in &defs.property {
                    let default = property_default(prop);
                    let mut selected = options.get(&prop.name).cloned().unwrap_or(default);
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
                    }
                }
            });
        }
        if !defs.filepath.is_empty() {
            ui.strong("ファイル選択");
            ui.push_id("filepath", |ui| {
                for filepath in &defs.filepath {
                    let mut selected =
                        files.get(&filepath.name).cloned().unwrap_or_else(|| filepath.def.clone());
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
                        ui.add(egui::DragValue::new(&mut value.x).prefix("x:"));
                        ui.add(egui::DragValue::new(&mut value.y).prefix("y:"));
                        ui.add(egui::DragValue::new(&mut value.w).prefix("w:"));
                        ui.add(egui::DragValue::new(&mut value.h).prefix("h:"));
                        ui.add(egui::DragValue::new(&mut value.r).prefix("r:"));
                        ui.add(egui::DragValue::new(&mut value.a).prefix("a:"));
                    });
                    if value != before {
                        match offsets.iter_mut().find(|o| o.id == offset_def.id) {
                            Some(entry) => *entry = value,
                            None => offsets.push(value),
                        }
                    }
                });
            }
        }
    });
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

/// property の既定選択肢名。`def` を優先し、空なら先頭 item を使う。
fn property_default(prop: &SkinPropertyDef) -> String {
    if prop.def.is_empty() {
        prop.item.first().map(|item| item.name.clone()).unwrap_or_default()
    } else {
        prop.def.clone()
    }
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
}
