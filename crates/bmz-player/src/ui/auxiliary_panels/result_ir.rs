use super::*;
use crate::screens::result_ir::{IrSubmitState, RankingLoadState, ResultIrState, ResultRankingTab};

/// リザルト画面の IR 送信状況とランキングを表示するオーバーレイ。
pub(super) fn build_result_ir_panel(
    ctx: &egui::Context,
    state: &mut ResultIrState,
    text: Localizer,
) {
    let constrain = ctx.content_rect().shrink(PANEL_VIEWPORT_MARGIN);
    let panel_width = 360.0;
    let pos = egui::pos2(constrain.right() - panel_width, constrain.top());
    let max_height = (constrain.height() - panel_window_chrome(ctx).y).max(32.0);

    egui::Window::new(tr!(text, "result-ir-title"))
        .id(egui::Id::new("result_ir_overlay"))
        .resizable(false)
        .collapsible(true)
        .movable(true)
        .constrain_to(constrain)
        .default_pos(pos)
        .default_width(panel_width)
        .max_height(max_height)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().min_scrolled_height(0.0).show(ui, |ui| {
                build_result_ir_content(ui, state, text);
            });
        });
}

fn build_result_ir_content(ui: &mut egui::Ui, state: &mut ResultIrState, text: Localizer) {
    for submission in &state.provider_submissions {
        ui.horizontal(|ui| {
            ui.strong(&submission.display_name);
            if submission.primary {
                ui.small(tr!(text, "result-ir-primary"));
            }
        });
        match &submission.state {
            IrSubmitState::Sending => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!(text, "result-ir-submitting"));
                });
            }
            IrSubmitState::Done { submitted, failed, message } => {
                if *failed > 0 {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        tr!(
                            text,
                            "result-ir-submit-failed",
                            "failed" => *failed,
                            "submitted" => *submitted
                        ),
                    );
                    if let Some(message) = message {
                        ui.small(message.clone());
                    }
                } else if *submitted > 0 {
                    ui.colored_label(
                        egui::Color32::LIGHT_GREEN,
                        tr!(
                            text,
                            "result-ir-submitted",
                            "submitted" => *submitted
                        ),
                    );
                } else {
                    ui.label(tr!(text, "result-ir-nothing-to-submit"));
                }
            }
        }
        ui.add_space(4.0);
    }

    ui.separator();
    let mut selected_tab = None;
    ui.horizontal(|ui| {
        let global = state.active_tab == ResultRankingTab::Global;
        let rivals = state.active_tab == ResultRankingTab::SelfAndRivals;
        if ui.selectable_label(global, tr!(text, "result-ir-tab-global")).clicked() && !global {
            selected_tab = Some(ResultRankingTab::Global);
        }
        if state.supports_tab(ResultRankingTab::SelfAndRivals)
            && ui.selectable_label(rivals, tr!(text, "result-ir-tab-rivals")).clicked()
            && !rivals
        {
            selected_tab = Some(ResultRankingTab::SelfAndRivals);
        }
    });
    if let Some(tab) = selected_tab {
        state.select_tab(tab);
    }
    // タブ未選択のまま NotRequested の場合 (prefetch OFF) も取得を開始する。
    if matches!(state.active_state(), RankingLoadState::NotRequested) {
        state.select_tab(state.active_tab);
    }

    match state.active_state() {
        RankingLoadState::NotRequested | RankingLoadState::Loading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr!(text, "result-ir-loading"));
            });
        }
        RankingLoadState::Failed(error) => {
            ui.colored_label(egui::Color32::LIGHT_RED, tr!(text, "result-ir-load-failed"));
            ui.small(error.clone());
        }
        RankingLoadState::Loaded(ranking) => {
            if ranking.entries.is_empty() {
                ui.label(tr!(text, "result-ir-empty"));
            } else {
                egui::Grid::new("result_ir_ranking_grid").num_columns(5).striped(true).show(
                    ui,
                    |ui| {
                        ui.strong("#");
                        ui.strong(tr!(text, "result-ir-player"));
                        ui.strong(tr!(text, "course-column-ex"));
                        ui.strong(tr!(text, "result-ir-clear"));
                        ui.strong(tr!(text, "course-column-bp"));
                        ui.end_row();
                        for entry in &ranking.entries {
                            ui.monospace(entry.rank.to_string());
                            ui.label(&entry.player_name);
                            ui.monospace(entry.ex_score.to_string());
                            ui.label(&entry.clear);
                            ui.monospace(entry.bp.to_string());
                            ui.end_row();
                        }
                    },
                );
                if let Some(rank) = ranking.self_rank {
                    ui.separator();
                    ui.label(tr!(text, "result-ir-self-rank", "rank" => rank));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{IrConfig, IrProviderConfig};
    use crate::ir::types::IrRankingScope;
    use crate::ln_policy::LnScorePolicy;
    use crate::screens::result_ir::{
        ResultIrRanking, ResultIrRankingEntry, cached_result_ir_state,
    };
    use crate::select_options::DoubleOptionScoreBucket;

    #[test]
    fn long_result_ir_ranking_scrolls_to_last_entry() {
        let mut provider = IrProviderConfig::bmz_ir();
        provider.provider_key = provider.provider.clone();
        provider.enabled = true;
        let config = IrConfig {
            primary_provider: provider.provider_key.clone(),
            providers: vec![provider],
            ..IrConfig::default()
        };
        let mut state = cached_result_ir_state(
            std::path::PathBuf::new(),
            &config,
            "chart".to_string(),
            LnScorePolicy::AutoLn,
            DoubleOptionScoreBucket::Off,
            bmz_gameplay::rule::RuleMode::Beatoraja,
        )
        .unwrap();
        state.global = RankingLoadState::Loaded(ResultIrRanking {
            scope: IrRankingScope::Global,
            entries: (1..=100)
                .map(|rank| ResultIrRankingEntry {
                    rank,
                    player_name: format!("Player {rank}"),
                    ex_score: 1000 - rank,
                    clear: "Normal".to_string(),
                    bp: 0,
                    max_combo: 0,
                })
                .collect(),
            clear_rate: None,
            clear_counts: None,
            self_rank: Some(100),
            previous_rank: None,
            total: Some(100),
        });

        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 300.0));
        let text = Localizer::new(AppLocale::En);
        let mut frame = |events| {
            ctx.run_ui(
                egui::RawInput { screen_rect: Some(screen), events, ..Default::default() },
                |ui| build_result_ir_panel(ui.ctx(), &mut state, text),
            )
        };
        let visible_label = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == label
                {
                    let bounds = text.galley.rect.translate(text.pos.to_vec2());
                    if screen.contains_rect(bounds) && shape.clip_rect.contains_rect(bounds) {
                        return Some(bounds.center());
                    }
                }
                None
            })
        };

        frame(vec![]);
        let output = frame(vec![]);
        let first = visible_label(&output, "Player 1").expect("first rank should be visible");
        assert!(visible_label(&output, "Player 100").is_none());
        let window =
            ctx.memory(|memory| memory.area_rect(egui::Id::new("result_ir_overlay"))).unwrap();
        assert!(screen.contains_rect(window), "IR window outside viewport: {window:?}");

        frame(vec![
            egui::Event::PointerMoved(first),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -200.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        // ホイールの移動量は egui が複数フレームに分けて適用する。
        for _ in 0..12 {
            frame(vec![]);
        }
        let output = frame(vec![]);
        assert!(visible_label(&output, "Player 1").is_none());
        assert!(visible_label(&output, "Player 100").is_some(), "last rank should be visible");
    }
}
