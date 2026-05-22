//! 本体設定 / スキン設定 / デバッグ表示のための egui レイヤ。
//!
//! `egui::Context` と winit 連携状態 (`egui_winit::State`) を所有し、毎フレーム
//! UI を構築して描画プリミティブ (`EguiFrame`) を生成する。bmz-render はその
//! プリミティブをゲーム / スキン描画の上にペイントするだけにする。

use bmz_render::ui::EguiFrame;
use egui::ViewportId;
use winit::event::WindowEvent;
use winit::window::Window;

use crate::config::app_config::{AppConfig, WindowMode};
use crate::config::profile_config::{ProfileConfig, SkinConfig};

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
    ) -> EguiOutput {
        let raw_input = self.state.take_egui_input(window);
        let ctx = self.ctx.clone();
        let visible = self.visible;
        let show_debug = &mut self.show_debug;
        let show_settings = &mut self.show_settings;
        let show_skin = &mut self.show_skin;
        let mut save_app_config = false;
        let mut save_profile_config = false;
        let full_output = ctx.run_ui(raw_input, |ui| {
            if visible {
                let ctx = ui.ctx();
                build_menu(ctx, show_debug, show_settings, show_skin);
                build_debug_panel(ctx, show_debug, info);
                if build_settings_panel(ctx, show_settings, app_config) {
                    save_app_config = true;
                }
                if build_skin_panel(ctx, show_skin, &mut profile_config.skin) {
                    save_profile_config = true;
                }
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

/// プロファイルのスキン設定 (`SkinConfig`) を編集するパネル。
///
/// 戻り値 `true` は「保存」ボタンが押されたことを表す。
fn build_skin_panel(ctx: &egui::Context, open: &mut bool, skin: &mut SkinConfig) -> bool {
    let mut save_clicked = false;
    egui::Window::new("スキン設定").open(open).default_pos(egui::pos2(16.0, 480.0)).show(
        ctx,
        |ui| {
            ui.label("各画面のスキンパス。空欄なら内蔵描画 / デフォルトスキンを使用します。");
            egui::Grid::new("skin_grid").num_columns(2).show(ui, |ui| {
                ui.label("選曲");
                ui.text_edit_singleline(&mut skin.select);
                ui.end_row();
                ui.label("プレイ");
                ui.text_edit_singleline(&mut skin.play);
                ui.end_row();
                ui.label("リザルト");
                ui.text_edit_singleline(&mut skin.result);
                ui.end_row();
            });
            ui.separator();
            ui.label(format!("オフセット定義: {} 件", skin.offsets.len()));
            ui.label("変更は次回起動時に反映されます。");
            if ui.button("保存").clicked() {
                save_clicked = true;
            }
        },
    );
    save_clicked
}
