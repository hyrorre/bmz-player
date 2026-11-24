use super::*;
use crate::screens::result_ir::{IrSubmitState, RankingLoadState, ResultIrState, ResultRankingTab};

pub(super) fn build_result_ir_content(
    ui: &mut egui::Ui,
    state: &mut ResultIrState,
    text: Localizer,
) {
    if let Some(notice) = &state.local_submission_notice {
        ui.small(notice);
        ui.separator();
    }
    for submission in &state.provider_submissions {
        let bokutachi = crate::ir::bokutachi::is_provider(&submission.provider_key);
        let dan = state.is_course() && bokutachi;
        ui.horizontal(|ui| {
            ui.strong(&submission.display_name);
            if dan {
                ui.small(text.text("ir-bokutachi-dan"));
            }
            if submission.primary {
                ui.small(tr!(text, "result-ir-primary"));
            }
        });
        if dan {
            match &submission.state {
                IrSubmitState::Sending | IrSubmitState::BokutachiAccepted => {
                    ui.spinner();
                    ui.label(text.text("ir-bokutachi-dan-pending"));
                }
                IrSubmitState::Done { failed, message, status, .. } => {
                    if status.is_some() || message.is_some() {
                        ui.label(crate::ui::ir_manual_ui::bokutachi_dan_status(
                            status.map(|s| s.as_str()).unwrap_or(if *failed > 0 {
                                "failed"
                            } else {
                                "pending"
                            }),
                            text,
                        ));
                        if let Some(message) = message {
                            ui.small(message);
                        }
                    } else {
                        ui.label(text.text("result-ir-nothing-to-submit"));
                    }
                }
            }
            ui.add_space(4.0);
            continue;
        }
        match &submission.state {
            IrSubmitState::BokutachiAccepted => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!(text, "ir-bokutachi-accepted"));
                });
            }
            IrSubmitState::Sending => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tr!(text, "result-ir-submitting"));
                });
            }
            IrSubmitState::Done { submitted, failed, message, status } => {
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
                    let label = if bokutachi {
                        tr!(text, "ir-queue-registered")
                    } else {
                        tr!(text, "result-ir-submitted", "submitted" => *submitted)
                    };
                    ui.colored_label(egui::Color32::LIGHT_GREEN, label);
                } else if let Some(message) = message {
                    if bokutachi {
                        ui.label(
                            if *status
                                == Some(crate::storage::network_db::IrScoreJobStatus::NoChange)
                            {
                                tr!(text, "ir-bokutachi-no-change")
                            } else if *status
                                == Some(crate::storage::network_db::IrScoreJobStatus::Ineligible)
                            {
                                tr!(text, "ir-bokutachi-ineligible")
                            } else {
                                tr!(text, "ir-bokutachi-pending")
                            },
                        );
                    }
                    ui.small(message);
                } else {
                    ui.label(tr!(text, "result-ir-nothing-to-submit"));
                }
                if *submitted > 0
                    && *failed == 0
                    && let Some(message) = message
                {
                    ui.small(message);
                }
            }
        }
        ui.add_space(4.0);
    }

    ui.separator();
    if !state.supports_tab(ResultRankingTab::Global) {
        ui.label(tr!(text, "ir-bokutachi-no-ranking"));
        return;
    }
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
    fn result_ranking_stays_selected_and_scrolls_to_last_entry() {
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
            None,
            LnScorePolicy::AutoLn,
            DoubleOptionScoreBucket::Off,
            bmz_gameplay::rule::RuleMode::Beatoraja,
            None,
            bmz_chart::kaleid::KaleidProvenance::imported(),
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
        let mut overlay = ResultPanelState::default();
        let started_at = std::time::Instant::now();
        let current_result = Some(started_at);
        let mut frame = |events, result_start: Option<std::time::Instant>| {
            if result_start.is_none() {
                overlay.request_raise();
            }
            ctx.run_ui(
                egui::RawInput { screen_rect: Some(screen), events, ..Default::default() },
                |ui| {
                    if let Some(started_at) = result_start {
                        overlay.show(
                            ui.ctx(),
                            ResultPanelDetails::Play(None),
                            Some(&mut state),
                            started_at,
                            text,
                        );
                    }
                },
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

        frame(vec![], current_result);
        let output = frame(vec![], current_result);
        assert!(visible_label(&output, "Player 1").is_none());
        assert!(visible_label(&output, "Lane results are unavailable.").is_some());
        let tab = visible_label(&output, "IR Rankings").unwrap();
        let button = |pressed| egui::Event::PointerButton {
            pos: tab,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(tab), button(true)], current_result);
        frame(vec![button(false)], current_result);
        let output = frame(vec![], None);
        assert!(visible_label(&output, "Player 1").is_none());
        let output = frame(vec![], current_result);
        let first = visible_label(&output, "Player 1").expect("first rank should be visible");
        assert!(visible_label(&output, "Player 100").is_none());
        let window =
            ctx.memory(|memory| memory.area_rect(egui::Id::new("result_overlay"))).unwrap();
        assert!(screen.contains_rect(window), "IR window outside viewport: {window:?}");

        frame(
            vec![
                egui::Event::PointerMoved(first),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -200.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            current_result,
        );
        // ホイールの移動量は egui が複数フレームに分けて適用する。
        for _ in 0..12 {
            frame(vec![], current_result);
        }
        let output = frame(vec![], current_result);
        assert!(visible_label(&output, "Player 1").is_none());
        assert!(visible_label(&output, "Player 100").is_some(), "last rank should be visible");
        let output = frame(vec![], Some(started_at + std::time::Duration::from_secs(1)));
        assert!(visible_label(&output, "Player 100").is_some());
        assert!(visible_label(&output, "Lane results are unavailable.").is_none());
    }
}
