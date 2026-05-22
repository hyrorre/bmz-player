//! 本体設定 / スキン設定 / デバッグ表示のための egui レイヤ。
//!
//! `egui::Context` と winit 連携状態 (`egui_winit::State`) を所有し、毎フレーム
//! UI を構築して描画プリミティブ (`EguiFrame`) を生成する。bmz-render はその
//! プリミティブをゲーム / スキン描画の上にペイントするだけにする。

use bmz_render::ui::EguiFrame;
use egui::ViewportId;
use winit::event::WindowEvent;
use winit::window::Window;

/// egui の状態管理とフレーム構築を担うレイヤ。
pub struct EguiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    /// メニュー表示中かどうか。F5 でトグルする。
    visible: bool,
}

impl EguiLayer {
    pub fn new(window: &Window) -> Self {
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
        Self { ctx, state, visible: false }
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

    /// 1 フレーム分の UI を構築し、描画プリミティブを返す。
    pub fn run(&mut self, window: &Window) -> EguiFrame {
        let raw_input = self.state.take_egui_input(window);
        let visible = self.visible;
        let full_output = self.ctx.run_ui(raw_input, |ui| {
            if visible {
                build_ui(ui.ctx());
            }
        });
        self.state.handle_platform_output(window, full_output.platform_output);
        let primitives = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        EguiFrame {
            primitives,
            textures_delta: full_output.textures_delta,
            pixels_per_point: full_output.pixels_per_point,
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

/// v1: egui 統合の動作確認用ウィンドウ。
///
/// 本体設定 / スキン設定 / デバッグ表示の各パネルは後続タスクでここへ追加する。
fn build_ui(ctx: &egui::Context) {
    egui::Window::new("BMZ Debug").default_pos(egui::pos2(16.0, 16.0)).show(ctx, |ui| {
        ui.label("egui 統合の動作確認ウィンドウです。");
        ui.label("F5 で開閉します。");
        ui.separator();
        if ui.button("クリック").clicked() {
            tracing::info!("egui debug window button clicked");
        }
    });
}
