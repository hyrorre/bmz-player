use super::*;
use crate::config::profile_config::{
    KeyModeConversionConfig, SevenToNinePattern, SevenToNineRuleMode, SevenToNineType,
};

pub(in crate::ui::profile_panel) fn build_profile_play_section(
    ui: &mut egui::Ui,
    section: &mut ProfileSectionContext<'_>,
) {
    let profile = &mut *section.profile;
    let unrestricted = section.unrestricted;
    let text = section.text;
    SettingsSection::new(SettingsPage::Play, tr!(text, "profile-play-title"))
        .scope(tr!(text, "settings-scope-profile"))
        .id_salt("profile_play")
        .show(ui, |ui| {
            if !unrestricted {
                ui.disable();
            }
            egui::ComboBox::new("profile_rule", tr!(text, "profile-play-rule"))
                .selected_text(rule_mode_label(profile.play.rule_mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut profile.play.rule_mode,
                        RuleMode::Beatoraja,
                        rule_mode_label(RuleMode::Beatoraja),
                    );
                    ui.selectable_value(
                        &mut profile.play.rule_mode,
                        RuleMode::Lr2Oraja,
                        rule_mode_label(RuleMode::Lr2Oraja),
                    );
                    ui.selectable_value(
                        &mut profile.play.rule_mode,
                        RuleMode::Dx,
                        rule_mode_label(RuleMode::Dx),
                    );
                });
            egui::ComboBox::new("profile_ln_mode", tr!(text, "profile-play-ln-mode"))
                .selected_text(profile.play.ln_mode_policy.display_label())
                .show_ui(ui, |ui| {
                    for value in LnPolicySetting::ORDER {
                        ui.selectable_value(
                            &mut profile.play.ln_mode_policy,
                            value,
                            value.display_label(),
                        );
                    }
                });
            egui::ComboBox::new("profile_gauge", tr!(text, "profile-play-gauge"))
                .selected_text(gauge_label(profile.play.gauge))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (GaugeTypeConfig::AssistEasy, "ASSIST EASY"),
                        (GaugeTypeConfig::Easy, "EASY"),
                        (GaugeTypeConfig::Normal, "NORMAL"),
                        (GaugeTypeConfig::Hard, "HARD"),
                        (GaugeTypeConfig::ExHard, "EX HARD"),
                        (GaugeTypeConfig::Hazard, "HAZARD"),
                        (GaugeTypeConfig::AutoShift, "AUTO SHIFT"),
                    ] {
                        ui.selectable_value(&mut profile.play.gauge, value, label);
                    }
                });
            egui::ComboBox::new(
                "profile_gauge_auto_shift",
                tr!(text, "profile-play-gauge-auto-shift"),
            )
            .selected_text(gauge_auto_shift_label(profile.play.gauge_auto_shift))
            .show_ui(ui, |ui| {
                for (value, label) in [
                    (GaugeAutoShiftConfig::Off, "OFF"),
                    (GaugeAutoShiftConfig::Continue, "CONTINUE"),
                    (GaugeAutoShiftConfig::HardToGroove, "HARD->GROOVE"),
                    (GaugeAutoShiftConfig::BestClear, "BEST CLEAR"),
                    (GaugeAutoShiftConfig::SelectToUnder, "SELECT UNDER"),
                ] {
                    ui.selectable_value(&mut profile.play.gauge_auto_shift, value, label);
                }
            });
            egui::ComboBox::new("profile_gas_floor", tr!(text, "profile-play-gas-floor"))
                .selected_text(bottom_shiftable_gauge_label(profile.play.bottom_shiftable_gauge))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (BottomShiftableGaugeConfig::AssistEasy, "ASSIST EASY"),
                        (BottomShiftableGaugeConfig::Easy, "EASY"),
                        (BottomShiftableGaugeConfig::Normal, "NORMAL"),
                    ] {
                        ui.selectable_value(&mut profile.play.bottom_shiftable_gauge, value, label);
                    }
                });
            egui::ComboBox::new("profile_random", tr!(text, "profile-play-random"))
                .selected_text(random_label(profile.play.random))
                .show_ui(ui, |ui| {
                    for (value, label) in random_options() {
                        ui.selectable_value(&mut profile.play.random, value, label);
                    }
                });
            egui::ComboBox::new("profile_random_2p", tr!(text, "profile-play-random-2p"))
                .selected_text(random_label(profile.play.random2))
                .show_ui(ui, |ui| {
                    for (value, label) in random_options() {
                        ui.selectable_value(&mut profile.play.random2, value, label);
                    }
                });
            egui::ComboBox::new("profile_dp_option", tr!(text, "profile-play-dp-option"))
                .selected_text(double_option_label(profile.play.double_option))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (DoubleOptionConfig::Off, "OFF"),
                        (DoubleOptionConfig::Flip, "FLIP"),
                        (DoubleOptionConfig::Battle, "BATTLE"),
                        (DoubleOptionConfig::BattleAutoScratch, "BATTLE AS"),
                    ] {
                        ui.selectable_value(&mut profile.play.double_option, value, label);
                    }
                });
            egui::ComboBox::from_label("HS-FIX")
                .selected_text(hs_fix_label(profile.play.hs_fix))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (HsFixConfig::Off, "OFF"),
                        (HsFixConfig::StartBpm, "START BPM"),
                        (HsFixConfig::MaxBpm, "MAX BPM"),
                        (HsFixConfig::MainBpm, "MAIN BPM"),
                        (HsFixConfig::MinBpm, "MIN BPM"),
                    ] {
                        ui.selectable_value(&mut profile.play.hs_fix, value, label);
                    }
                });
            egui::ComboBox::new("profile_target", tr!(text, "profile-play-target"))
                .selected_text(target_label(profile.play.target))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (TargetOptionConfig::None, "NONE"),
                        (TargetOptionConfig::RankA, "RANK_A"),
                        (TargetOptionConfig::RankAaMinus, "RANK_AA-"),
                        (TargetOptionConfig::RankAa, "RANK_AA"),
                        (TargetOptionConfig::RankAaaMinus, "RANK_AAA-"),
                        (TargetOptionConfig::RankAaa, "RANK_AAA"),
                        (TargetOptionConfig::RankMaxMinus, "RANK_MAX-"),
                        (TargetOptionConfig::Max, "MAX"),
                        (TargetOptionConfig::RankNext, "RANK_NEXT"),
                        (TargetOptionConfig::IrTop, "IR_TOP"),
                        (TargetOptionConfig::IrNext, "IR_NEXT"),
                        (TargetOptionConfig::RivalTop, "RIVAL TOP"),
                        (TargetOptionConfig::RivalNext, "RIVAL NEXT"),
                    ] {
                        ui.selectable_value(&mut profile.play.target, value, label);
                    }
                });
            egui::ComboBox::from_label("BGA")
                .selected_text(bga_mode_label(profile.play.bga))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (BgaModeConfig::On, "ON"),
                        (BgaModeConfig::Auto, "AUTO"),
                        (BgaModeConfig::Off, "OFF"),
                    ] {
                        ui.selectable_value(&mut profile.play.bga, value, label);
                    }
                });
            egui::ComboBox::new("profile_bga_expand", tr!(text, "profile-play-bga-display"))
                .selected_text(bga_expand_label(profile.play.bga_expand))
                .show_ui(ui, |ui| {
                    for (value, label) in [
                        (BgaExpandConfig::KeepAspect, "KEEP ASPECT"),
                        (BgaExpandConfig::Full, "FULL"),
                        (BgaExpandConfig::Off, "OFF"),
                    ] {
                        ui.selectable_value(&mut profile.play.bga_expand, value, label);
                    }
                });
            let mut session_mode = profile.play.session_mode.unwrap_or(if profile.play.auto_play {
                SessionMode::Autoplay
            } else {
                SessionMode::Normal
            });
            egui::ComboBox::new("profile_session_mode", tr!(text, "profile-play-session-mode"))
                .selected_text(session_mode.as_str())
                .show_ui(ui, |ui| {
                    for value in SessionMode::VALUES {
                        ui.selectable_value(&mut session_mode, value, value.as_str());
                    }
                });
            profile.play.session_mode = Some(session_mode);
            profile.play.auto_play = session_mode.primary_autoplay();
            let previous_conversion = profile.play.key_mode_conversion;
            egui::ComboBox::new(
                "profile_key_mode_conversion",
                tr!(text, "profile-play-key-mode-conversion"),
            )
            .selected_text(profile.play.key_mode_conversion.as_str())
            .show_ui(ui, |ui| {
                for value in KeyModeConversionConfig::VALUES {
                    ui.selectable_value(
                        &mut profile.play.key_mode_conversion,
                        value,
                        value.as_str(),
                    );
                }
            });
            ui.label(tr!(text, "profile-play-key-mode-conversion-help"));
            if profile.play.key_mode_conversion != previous_conversion
                && profile.play.key_mode_conversion != KeyModeConversionConfig::Off
            {
                profile.play.double_option = DoubleOptionConfig::Off;
            }
            if profile.play.key_mode_conversion == KeyModeConversionConfig::SevenToNine {
                egui::ComboBox::new(
                    "profile_seven_to_nine_pattern",
                    tr!(text, "profile-play-seven-to-nine-pattern"),
                )
                .selected_text(profile.play.seven_to_nine_pattern.label())
                .show_ui(ui, |ui| {
                    for value in SevenToNinePattern::VALUES {
                        ui.selectable_value(
                            &mut profile.play.seven_to_nine_pattern,
                            value,
                            value.label(),
                        );
                    }
                });
                egui::ComboBox::new(
                    "profile_seven_to_nine_type",
                    tr!(text, "profile-play-seven-to-nine-type"),
                )
                .selected_text(profile.play.seven_to_nine_type.label())
                .show_ui(ui, |ui| {
                    for value in SevenToNineType::VALUES {
                        ui.selectable_value(
                            &mut profile.play.seven_to_nine_type,
                            value,
                            value.label(),
                        );
                    }
                });
                egui::ComboBox::new("profile_seven_to_nine_rule_mode", "7K TO 9K RULE")
                    .selected_text(profile.play.seven_to_nine_rule_mode.as_str())
                    .show_ui(ui, |ui| {
                        for value in SevenToNineRuleMode::VALUES {
                            ui.selectable_value(
                                &mut profile.play.seven_to_nine_rule_mode,
                                value,
                                value.as_str(),
                            );
                        }
                    });
                ui.label(match profile.play.seven_to_nine_rule_mode {
                    SevenToNineRuleMode::Keys7 => {
                        "7K judging/gauge; score, lamp, replay and IR can be saved"
                    }
                    SevenToNineRuleMode::Keys9 => {
                        "9K judging/gauge; score, lamp, replay and IR are not saved"
                    }
                });
            }
            SettingsSection::new(SettingsPage::Play, "ASSIST / MODIFIERS")
                .scope(tr!(text, "settings-scope-profile"))
                .id_salt("profile_play_assist")
                .show(ui, |ui| {
                    let assist = &mut profile.play.assist;
                    ui.checkbox(&mut assist.expand_judge, "EXPAND JUDGE");
                    ui.checkbox(&mut assist.judge_area, "JUDGE AREA");
                    ui.checkbox(&mut assist.mark_note, "MARK NOTE");
                    ui.checkbox(&mut assist.bpm_guide, "BPM GUIDE");

                    egui::ComboBox::from_label("SCROLL")
                        .selected_text(match assist.scroll_mode {
                            AssistScrollMode::Off => "OFF",
                            AssistScrollMode::Remove => "REMOVE",
                            AssistScrollMode::Add => "ADD",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut assist.scroll_mode,
                                AssistScrollMode::Off,
                                "OFF",
                            );
                            ui.selectable_value(
                                &mut assist.scroll_mode,
                                AssistScrollMode::Remove,
                                "REMOVE (CONSTANT)",
                            );
                            ui.selectable_value(
                                &mut assist.scroll_mode,
                                AssistScrollMode::Add,
                                "ADD",
                            );
                        });
                    if assist.scroll_mode == AssistScrollMode::Add {
                        ui.add(
                            egui::Slider::new(&mut assist.scroll_section, 1..=64)
                                .text("SCROLL SECTION"),
                        );
                        ui.add(
                            egui::Slider::new(&mut assist.scroll_rate, 0.0..=1.0)
                                .text("SCROLL RATE"),
                        );
                    }

                    egui::ComboBox::from_label("LONGNOTE")
                        .selected_text(match assist.long_note_mode {
                            AssistLongNoteMode::Off => "OFF",
                            AssistLongNoteMode::Remove => "REMOVE",
                            AssistLongNoteMode::AddLn => "ADD LN",
                            AssistLongNoteMode::AddCn => "ADD CN",
                            AssistLongNoteMode::AddHcn => "ADD HCN",
                            AssistLongNoteMode::AddAll => "ADD ALL",
                        })
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (AssistLongNoteMode::Off, "OFF"),
                                (AssistLongNoteMode::Remove, "REMOVE (LEGACY NOTE)"),
                                (AssistLongNoteMode::AddLn, "ADD LN"),
                                (AssistLongNoteMode::AddCn, "ADD CN"),
                                (AssistLongNoteMode::AddHcn, "ADD HCN"),
                                (AssistLongNoteMode::AddAll, "ADD ALL"),
                            ] {
                                ui.selectable_value(&mut assist.long_note_mode, value, label);
                            }
                        });
                    if assist.long_note_mode != AssistLongNoteMode::Off {
                        ui.add(
                            egui::Slider::new(&mut assist.long_note_rate, 0.0..=1.0)
                                .text("LONGNOTE RATE"),
                        );
                    }

                    egui::ComboBox::from_label("MINE")
                        .selected_text(match assist.mine_mode {
                            AssistMineMode::Off => "OFF",
                            AssistMineMode::Remove => "REMOVE",
                            AssistMineMode::AddRandom => "ADD RANDOM",
                            AssistMineMode::AddNear => "ADD NEAR",
                            AssistMineMode::AddBlank => "ADD BLANK",
                        })
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (AssistMineMode::Off, "OFF"),
                                (AssistMineMode::Remove, "REMOVE (NO MINE)"),
                                (AssistMineMode::AddRandom, "ADD RANDOM"),
                                (AssistMineMode::AddNear, "ADD NEAR"),
                                (AssistMineMode::AddBlank, "ADD BLANK"),
                            ] {
                                ui.selectable_value(&mut assist.mine_mode, value, label);
                            }
                        });

                    ui.add(
                        egui::Slider::new(&mut assist.extra_note_depth, 0..=16)
                            .text("EXTRA NOTE DEPTH"),
                    );
                    ui.checkbox(&mut assist.extra_note_scratch, "EXTRA NOTE SCRATCH");
                    ui.add(
                        egui::Slider::new(&mut assist.extra_note_type, 0..=2)
                            .text("EXTRA NOTE TYPE (beatoraja reserved)"),
                    );

                    if assist.expand_judge {
                        ui.separator();
                        ui.label("JUDGE WINDOW RATE (%)");
                        for (value, label) in [
                            (&mut assist.key_pgreat_rate, "KEY PGREAT"),
                            (&mut assist.key_great_rate, "KEY GREAT"),
                            (&mut assist.key_good_rate, "KEY GOOD"),
                            (&mut assist.scratch_pgreat_rate, "SCRATCH PGREAT"),
                            (&mut assist.scratch_great_rate, "SCRATCH GREAT"),
                            (&mut assist.scratch_good_rate, "SCRATCH GOOD"),
                            (&mut assist.long_note_margin_rate, "LN MARGIN"),
                        ] {
                            ui.add(egui::Slider::new(value, 0..=400).text(label));
                        }
                    }
                });
            ui.checkbox(&mut profile.play.note_retention, tr!(text, "profile-play-note-retention"));
            ui.checkbox(&mut profile.play.show_ln_tail_cap, tr!(text, "profile-play-ln-tail-cap"));
            ui.checkbox(&mut profile.play.guide_se, tr!(text, "profile-play-guide-se"));
            ui.checkbox(
                &mut profile.play.wait_all_notes_result,
                tr!(text, "profile-play-wait-all-notes-result"),
            )
            .on_hover_text(tr!(text, "settings-entry-description-wait-all-notes-result"));
            ui.checkbox(
                &mut profile.play.play_keysound_on_miss,
                tr!(text, "profile-play-play-keysound-on-miss"),
            )
            .on_hover_text(tr!(text, "settings-entry-description-play-keysound-on-miss"));
            ui.add(
                egui::Slider::new(&mut profile.play.misslayer_duration_ms, 0..=5000)
                    .text(tr!(text, "profile-play-miss-layer-duration")),
            );
            ui.checkbox(
                &mut profile.play.hide_misslayer_on_good,
                tr!(text, "profile-play-hide-misslayer-on-good"),
            );
            ui.add(
                egui::Slider::new(&mut profile.play.play_exit_hold_ms, 100..=5000)
                    .text(tr!(text, "profile-play-exit-hold-duration")),
            );
        });
}
