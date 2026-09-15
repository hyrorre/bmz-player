use std::collections::HashSet;

use bmz_core::lane::{KeyMode, Lane};

use crate::config::key_config::{is_scratch_down_control, is_scratch_up_control};
use crate::config::play_input::{is_gamepad_device, resolve_play_bindings};
use crate::config::profile_config::{
    BindingConfigEntry, InputActionConfig, LaneConfig, ProfileInputConfig, ScratchDirectionConfig,
    SelectInputModeConfig,
};

pub(super) struct SelectKeyBindings {
    start: Vec<String>,
    e_action_controls: Vec<(InputActionConfig, String)>,
    e2_action_controls: Vec<String>,
    e3_action_controls: Vec<String>,
    enter: Vec<String>,
    back: Vec<String>,
    key1_controls: Vec<String>,
    key2_controls: Vec<String>,
    key3_controls: Vec<String>,
    key4_controls: Vec<String>,
    key5_controls: Vec<String>,
    key6_controls: Vec<String>,
    key7_controls: Vec<String>,
    key8_controls: Vec<String>,
    key9_controls: Vec<String>,
    key10_controls: Vec<String>,
    key11_controls: Vec<String>,
    key12_controls: Vec<String>,
    key13_controls: Vec<String>,
    key14_controls: Vec<String>,
    scratch_up_controls: Vec<String>,
    scratch_down_controls: Vec<String>,
    select_scratch_up_controls: Vec<String>,
    select_scratch_down_controls: Vec<String>,
    select_previous_controls: Vec<String>,
    select_next_controls: Vec<String>,
    target_previous_controls: Vec<String>,
    target_next_controls: Vec<String>,
    favorite_song_controls: Vec<String>,
    favorite_chart_controls: Vec<String>,
    same_folder_controls: Vec<String>,
    mode_filter_controls: Vec<String>,
    sort_controls: Vec<String>,
    ln_mode_controls: Vec<String>,
    difficulty_filter_controls: Vec<String>,
    replay_cycle_controls: Vec<String>,
    replay_play_controls: Vec<String>,
    open_folder_controls: Vec<String>,
    reload_controls: Vec<String>,
    autoplay_folder_controls: Vec<String>,
    open_ir_controls: Vec<String>,
    open_key_config_controls: Vec<String>,
    screenshot_controls: Vec<String>,
    rival_cycle_controls: Vec<String>,
    open_documents_controls: Vec<String>,
    cycle_bga: Option<String>,
    key_hint: String,
    option_hint: String,
}

impl SelectKeyBindings {
    pub(super) fn from_profile(input: &ProfileInputConfig) -> Self {
        if input.select_input_mode == SelectInputModeConfig::Key9 {
            return Self::from_profile_9k(input);
        }

        let play_7k = resolve_play_bindings(input, KeyMode::K7).unwrap_or_default();
        let play_14k = resolve_play_bindings(input, KeyMode::K14).unwrap_or_default();
        let kb: Vec<_> = input.ui.bindings.iter().filter(|e| e.device == "keyboard").collect();
        let play_kb: Vec<_> = play_7k.iter().filter(|e| e.device == "keyboard").collect();
        let all_input: Vec<_> = input
            .ui
            .bindings
            .iter()
            .filter(|e| e.device == "keyboard" || is_gamepad_device(&e.device))
            .collect();
        let common_controls: HashSet<(String, String)> = all_input
            .iter()
            .filter(|entry| {
                matches!(
                    entry.action,
                    Some(
                        InputActionConfig::E1
                            | InputActionConfig::E2
                            | InputActionConfig::E3
                            | InputActionConfig::E4
                    )
                )
            })
            .map(|entry| (entry.device.clone(), entry.control.clone()))
            .collect();
        let play_all: Vec<_> = play_7k
            .iter()
            .filter(|e| e.device == "keyboard" || is_gamepad_device(&e.device))
            .collect();
        let play_14k_all: Vec<_> = play_14k
            .iter()
            .filter(|e| e.device == "keyboard" || is_gamepad_device(&e.device))
            .collect();

        // キーボード専用（ヒント文字列表示用）
        let kb_keys_for = |lane: LaneConfig| -> Vec<String> {
            play_kb
                .iter()
                .filter(|e| {
                    e.lane == Some(lane)
                        && !common_control_matches(&common_controls, &e.device, &e.control)
                })
                .map(|e| e.control.clone())
                .collect()
        };

        // キーボード + ゲームパッド（is_enter / is_back ルックアップ用）
        let keys_for = |lane: LaneConfig| -> Vec<String> {
            play_all
                .iter()
                .filter(|e| {
                    e.lane == Some(lane)
                        && !common_control_matches(&common_controls, &e.device, &e.control)
                })
                .map(|e| e.control.clone())
                .collect()
        };
        let keys_for_2p = |lane: LaneConfig| -> Vec<String> {
            play_14k_all
                .iter()
                .filter(|e| {
                    e.lane == Some(lane)
                        && !common_control_matches(&common_controls, &e.device, &e.control)
                        // Select uses the 7K keyboard first; a customized SP key
                        // must not also activate an unrelated default DP key.
                        && !(e.device == "keyboard"
                            && play_kb.iter().any(|primary| primary.control == e.control))
                })
                .map(|e| e.control.clone())
                .collect()
        };
        let kb_actions_for = |action: InputActionConfig| -> Vec<String> {
            kb.iter().filter(|e| e.action == Some(action)).map(|e| e.control.clone()).collect()
        };
        let common_actions_for = |action: InputActionConfig| {
            all_input
                .iter()
                .filter(|e| e.action == Some(action))
                .map(|e| e.control.clone())
                .collect::<Vec<_>>()
        };
        let select_actions_for = |action: InputActionConfig| {
            with_numeric_keypad_aliases(
                all_input
                    .iter()
                    .filter(|entry| {
                        entry.action == Some(action)
                            && !common_control_matches(
                                &common_controls,
                                &entry.device,
                                &entry.control,
                            )
                    })
                    .map(|entry| entry.control.clone())
                    .collect(),
                &kb,
            )
        };
        let select_action_with_default = |action: InputActionConfig, default: &str| {
            let configured = select_actions_for(action);
            if configured.is_empty()
                && common_control_matches(&common_controls, "keyboard", default)
            {
                return Vec::new();
            }
            with_numeric_keypad_aliases(select_controls_with_default(configured, default), &kb)
        };

        let key1_controls = keys_for(LaneConfig::Key1);
        let key2_controls = keys_for(LaneConfig::Key2);
        let key3_controls = keys_for(LaneConfig::Key3);
        let key4_controls = keys_for(LaneConfig::Key4);
        let key5_controls = keys_for(LaneConfig::Key5);
        let key6_controls = keys_for(LaneConfig::Key6);
        let key7_controls = keys_for(LaneConfig::Key7);
        let key8_controls = keys_for_2p(LaneConfig::Key8);
        let key9_controls = keys_for_2p(LaneConfig::Key9);
        let key10_controls = keys_for_2p(LaneConfig::Key10);
        let key11_controls = keys_for_2p(LaneConfig::Key11);
        let key12_controls = keys_for_2p(LaneConfig::Key12);
        let key13_controls = keys_for_2p(LaneConfig::Key13);
        let key14_controls = keys_for_2p(LaneConfig::Key14);

        let lane_enter_1p: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&lane| keys_for(lane))
                .collect();
        let lane_enter_2p: Vec<String> =
            [LaneConfig::Key8, LaneConfig::Key10, LaneConfig::Key12, LaneConfig::Key14]
                .iter()
                .flat_map(|&lane| keys_for_2p(lane))
                .collect();
        let lane_enter = merge_select_controls(lane_enter_1p, lane_enter_2p);
        let lane_back_1p: Vec<String> = [LaneConfig::Key2, LaneConfig::Key4, LaneConfig::Key6]
            .iter()
            .flat_map(|&lane| keys_for(lane))
            .collect();
        let lane_back_2p: Vec<String> = [LaneConfig::Key9, LaneConfig::Key11, LaneConfig::Key13]
            .iter()
            .flat_map(|&lane| keys_for_2p(lane))
            .collect();
        let lane_back = merge_select_controls(lane_back_1p, lane_back_2p);
        let enter = lane_enter;
        let back = merge_select_controls(common_actions_for(InputActionConfig::E2), lane_back);
        let e_action_controls: Vec<(InputActionConfig, String)> = [
            InputActionConfig::E1,
            InputActionConfig::E2,
            InputActionConfig::E3,
            InputActionConfig::E4,
        ]
        .into_iter()
        .flat_map(|action| {
            common_actions_for(action).into_iter().map(move |control| (action, control))
        })
        .collect();
        let e2_action_controls = common_actions_for(InputActionConfig::E2);
        let e3_action_controls = common_actions_for(InputActionConfig::E3);
        let favorite_song_controls =
            select_action_with_default(InputActionConfig::SelectFavoriteSong, "F8");
        let favorite_chart_controls =
            select_action_with_default(InputActionConfig::SelectFavoriteChart, "F9");
        let same_folder_controls = select_actions_for(InputActionConfig::SelectSameFolder);
        let mode_filter_controls = select_actions_for(InputActionConfig::SelectModeFilter);
        let sort_controls = select_actions_for(InputActionConfig::SelectSort);
        let ln_mode_controls = select_actions_for(InputActionConfig::SelectLnMode);
        let difficulty_filter_controls =
            select_action_with_default(InputActionConfig::SelectDifficultyFilter, "0");
        let replay_cycle_controls = select_actions_for(InputActionConfig::SelectReplayCycle);
        let replay_play_controls =
            select_action_with_default(InputActionConfig::SelectReplayPlay, "5");
        let open_folder_controls = select_actions_for(InputActionConfig::SelectOpenFolder);
        let reload_controls = select_actions_for(InputActionConfig::SelectReload);
        let autoplay_folder_controls = select_actions_for(InputActionConfig::SelectAutoplayFolder);
        let open_ir_controls = select_actions_for(InputActionConfig::SelectOpenIr);
        let open_key_config_controls = select_actions_for(InputActionConfig::SelectOpenKeyConfig);
        let screenshot_controls = select_actions_for(InputActionConfig::Screenshot);
        let rival_cycle_controls = select_actions_for(InputActionConfig::SelectRivalCycle);
        let open_documents_controls = select_actions_for(InputActionConfig::SelectOpenDocuments);
        let mut scratch_up_controls = Vec::new();
        let mut scratch_down_controls = Vec::new();
        let mut select_scratch_up_controls = Vec::new();
        let mut select_scratch_down_controls = Vec::new();
        for entry in play_all.iter().filter(|e| e.lane == Some(LaneConfig::Scratch)) {
            push_scratch_controls(entry, &mut scratch_up_controls, &mut scratch_down_controls);
            push_scratch_controls(
                entry,
                &mut select_scratch_up_controls,
                &mut select_scratch_down_controls,
            );
        }
        for entry in play_14k_all.iter().filter(|e| e.lane == Some(LaneConfig::Scratch2)) {
            push_scratch_controls(entry, &mut scratch_up_controls, &mut scratch_down_controls);
            push_scratch_controls(
                entry,
                &mut select_scratch_up_controls,
                &mut select_scratch_down_controls,
            );
        }
        let cycle_bga = keys_for(LaneConfig::Key1).into_iter().next();
        let mut start = common_actions_for(InputActionConfig::E1);
        if let Some(legacy_start) = input.start_key.clone()
            && !start.iter().any(|control| control == &legacy_start)
        {
            start.push(legacy_start);
        }
        if start.is_empty() {
            start.push("Q".to_string());
        }

        // ヒント文字列はキーボードバインドのみ使用
        let kb_lane_enter: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&lane| kb_keys_for(lane))
                .collect();
        let kb_lane_back: Vec<String> = [LaneConfig::Key2, LaneConfig::Key4, LaneConfig::Key6]
            .iter()
            .flat_map(|&lane| kb_keys_for(lane))
            .collect();
        let kb_enter = kb_lane_enter;
        let enter_str =
            if kb_enter.is_empty() { String::new() } else { format!("/{}", kb_enter.join("/")) };
        let back_str = if kb_lane_back.is_empty() {
            kb_actions_for(InputActionConfig::E2)
                .first()
                .map(|key| format!("/{key}"))
                .unwrap_or_default()
        } else {
            format!("/{}", kb_lane_back.join("/"))
        };
        let key2_str =
            kb_keys_for(LaneConfig::Key2).into_iter().next().unwrap_or_else(|| "Key2".to_string());
        let start_str = kb_actions_for(InputActionConfig::E1)
            .into_iter()
            .next()
            .or_else(|| input.start_key.clone())
            .unwrap_or_else(|| start.first().cloned().unwrap_or_else(|| "Q".to_string()));
        let key_hint =
            format!("UP DOWN  RIGHT{enter_str}:ENTER  LEFT{back_str}:BACK  ENTER {start_str}");

        let kb_bga_str = kb_keys_for(LaneConfig::Key1).into_iter().next();
        let bga_str = kb_bga_str.as_deref().unwrap_or("?");
        let shortcut_hint = |action: InputActionConfig| {
            kb_actions_for(action).into_iter().next().unwrap_or_else(|| "-".to_string())
        };
        let open_folder_str = shortcut_hint(InputActionConfig::SelectOpenFolder);
        let reload_str = shortcut_hint(InputActionConfig::SelectReload);
        let autoplay_folder_str = shortcut_hint(InputActionConfig::SelectAutoplayFolder);
        let open_ir_str = shortcut_hint(InputActionConfig::SelectOpenIr);
        let key_config_str = shortcut_hint(InputActionConfig::SelectOpenKeyConfig);
        let screenshot_str = shortcut_hint(InputActionConfig::Screenshot);
        let rival_str = shortcut_hint(InputActionConfig::SelectRivalCycle);
        let documents_str = shortcut_hint(InputActionConfig::SelectOpenDocuments);
        let mode_filter_str = shortcut_hint(InputActionConfig::SelectModeFilter);
        let sort_str = shortcut_hint(InputActionConfig::SelectSort);
        let ln_mode_str = shortcut_hint(InputActionConfig::SelectLnMode);
        let replay_cycle_str = shortcut_hint(InputActionConfig::SelectReplayCycle);
        let same_folder_str = shortcut_hint(InputActionConfig::SelectSameFolder);
        let option_hint = format!(
            "F1 MENU  {open_folder_str}:FOLDER/HASH  {reload_str}:RELOAD  \
             {autoplay_folder_str}:AUTOPLAY  {open_ir_str}:IR  {key_config_str}:KEY CONFIG  \
             {screenshot_str}:SHOT  \
             {mode_filter_str}:MODE  {sort_str}:SORT  {ln_mode_str}:LN  \
             {replay_cycle_str}:CYCLE  {same_folder_str}:SAME  {rival_str}:RIVAL  {documents_str}:TEXT   \
             {start_str}:PLAY OPT  BACK:E2 OPT  {start_str}+BACK:DETAIL OPT  \
             {start_str}+K1/K2:1P ARR  {start_str}+2P K1/K2:2P ARR  {start_str}+K3/K4:GAUGE  \
             {start_str}+K5:HS-FIX  {start_str}+K6:DP OPT  {start_str}+K7:AUTOPLAY  \
             BACK+K1..K7:ASSIST  \
             {start_str}+BACK+{key2_str}:GAS  {start_str}+UP/DOWN:TARGET  {start_str}+{bga_str}:BGA  {start_str}+K4/K6:GREEN  {start_str}+K5/K7:TIMING  {start_str}+1..4:REPLAY  N5:REPLAY"
        );

        Self {
            start,
            e_action_controls,
            e2_action_controls,
            e3_action_controls,
            enter,
            back,
            key1_controls,
            key2_controls,
            key3_controls,
            key4_controls,
            key5_controls,
            key6_controls,
            key7_controls,
            key8_controls,
            key9_controls,
            key10_controls,
            key11_controls,
            key12_controls,
            key13_controls,
            key14_controls,
            scratch_up_controls,
            scratch_down_controls,
            select_scratch_up_controls,
            select_scratch_down_controls,
            select_previous_controls: Vec::new(),
            select_next_controls: Vec::new(),
            target_previous_controls: Vec::new(),
            target_next_controls: Vec::new(),
            favorite_song_controls,
            favorite_chart_controls,
            same_folder_controls,
            mode_filter_controls,
            sort_controls,
            ln_mode_controls,
            difficulty_filter_controls,
            replay_cycle_controls,
            replay_play_controls,
            open_folder_controls,
            reload_controls,
            autoplay_folder_controls,
            open_ir_controls,
            open_key_config_controls,
            screenshot_controls,
            rival_cycle_controls,
            open_documents_controls,
            cycle_bga,
            key_hint,
            option_hint,
        }
    }

    fn from_profile_9k(input: &ProfileInputConfig) -> Self {
        let play_9k = resolve_play_bindings(input, KeyMode::K9).unwrap_or_default();
        let kb: Vec<_> = input.ui.bindings.iter().filter(|e| e.device == "keyboard").collect();
        let play_kb: Vec<_> = play_9k.iter().filter(|e| e.device == "keyboard").collect();
        let all_input: Vec<_> = input
            .ui
            .bindings
            .iter()
            .filter(|e| e.device == "keyboard" || is_gamepad_device(&e.device))
            .collect();
        let common_controls: HashSet<(String, String)> = all_input
            .iter()
            .filter(|entry| {
                matches!(
                    entry.action,
                    Some(
                        InputActionConfig::E1
                            | InputActionConfig::E2
                            | InputActionConfig::E3
                            | InputActionConfig::E4
                    )
                )
            })
            .map(|entry| (entry.device.clone(), entry.control.clone()))
            .collect();
        let play_all: Vec<_> = play_9k
            .iter()
            .filter(|e| e.device == "keyboard" || is_gamepad_device(&e.device))
            .collect();
        let kb_keys_for = |lane: LaneConfig| -> Vec<String> {
            play_kb
                .iter()
                .filter(|e| {
                    e.lane == Some(lane)
                        && !common_control_matches(&common_controls, &e.device, &e.control)
                })
                .map(|e| e.control.clone())
                .collect()
        };
        let keys_for = |lane: LaneConfig| -> Vec<String> {
            play_all
                .iter()
                .filter(|e| {
                    e.lane == Some(lane)
                        && !common_control_matches(&common_controls, &e.device, &e.control)
                })
                .map(|e| e.control.clone())
                .collect()
        };
        let kb_actions_for = |action: InputActionConfig| -> Vec<String> {
            kb.iter().filter(|e| e.action == Some(action)).map(|e| e.control.clone()).collect()
        };
        let common_actions_for = |action: InputActionConfig| {
            all_input
                .iter()
                .filter(|e| e.action == Some(action))
                .map(|e| e.control.clone())
                .collect::<Vec<_>>()
        };
        let select_actions_for = |action: InputActionConfig| {
            with_numeric_keypad_aliases(
                all_input
                    .iter()
                    .filter(|entry| {
                        entry.action == Some(action)
                            && !common_control_matches(
                                &common_controls,
                                &entry.device,
                                &entry.control,
                            )
                    })
                    .map(|entry| entry.control.clone())
                    .collect(),
                &kb,
            )
        };
        let select_action_with_default = |action: InputActionConfig, default: &str| {
            let configured = select_actions_for(action);
            if configured.is_empty()
                && common_control_matches(&common_controls, "keyboard", default)
            {
                return Vec::new();
            }
            with_numeric_keypad_aliases(select_controls_with_default(configured, default), &kb)
        };

        let key1_controls = keys_for(LaneConfig::Key1);
        let key2_controls = keys_for(LaneConfig::Key2);
        let key3_controls = keys_for(LaneConfig::Key3);
        let key4_controls = keys_for(LaneConfig::Key4);
        let key5_controls = keys_for(LaneConfig::Key5);
        let key6_controls = keys_for(LaneConfig::Key6);
        let key7_controls = keys_for(LaneConfig::Key7);
        let key8_controls = keys_for(LaneConfig::Key8);
        let key9_controls = keys_for(LaneConfig::Key9);

        let enter = merge_select_controls(key5_controls.clone(), key7_controls.clone());
        let back =
            merge_select_controls(common_actions_for(InputActionConfig::E2), key3_controls.clone());
        let select_previous_controls = key4_controls.clone();
        let select_next_controls = key6_controls.clone();
        let target_previous_controls = key8_controls.clone();
        let target_next_controls = key9_controls.clone();
        let e_action_controls: Vec<(InputActionConfig, String)> = [
            InputActionConfig::E1,
            InputActionConfig::E2,
            InputActionConfig::E3,
            InputActionConfig::E4,
        ]
        .into_iter()
        .flat_map(|action| {
            common_actions_for(action).into_iter().map(move |control| (action, control))
        })
        .collect();
        let e2_action_controls = common_actions_for(InputActionConfig::E2);
        let e3_action_controls = common_actions_for(InputActionConfig::E3);
        let favorite_song_controls =
            select_action_with_default(InputActionConfig::SelectFavoriteSong, "F8");
        let favorite_chart_controls =
            select_action_with_default(InputActionConfig::SelectFavoriteChart, "F9");
        let same_folder_controls = select_actions_for(InputActionConfig::SelectSameFolder);
        let mode_filter_controls = select_actions_for(InputActionConfig::SelectModeFilter);
        let sort_controls = select_actions_for(InputActionConfig::SelectSort);
        let ln_mode_controls = select_actions_for(InputActionConfig::SelectLnMode);
        let difficulty_filter_controls =
            select_action_with_default(InputActionConfig::SelectDifficultyFilter, "0");
        let replay_cycle_controls = select_actions_for(InputActionConfig::SelectReplayCycle);
        let replay_play_controls =
            select_action_with_default(InputActionConfig::SelectReplayPlay, "5");
        let open_folder_controls = select_actions_for(InputActionConfig::SelectOpenFolder);
        let reload_controls = select_actions_for(InputActionConfig::SelectReload);
        let autoplay_folder_controls = select_actions_for(InputActionConfig::SelectAutoplayFolder);
        let open_ir_controls = select_actions_for(InputActionConfig::SelectOpenIr);
        let open_key_config_controls = select_actions_for(InputActionConfig::SelectOpenKeyConfig);
        let screenshot_controls = select_actions_for(InputActionConfig::Screenshot);
        let rival_cycle_controls = select_actions_for(InputActionConfig::SelectRivalCycle);
        let open_documents_controls = select_actions_for(InputActionConfig::SelectOpenDocuments);
        let cycle_bga = key1_controls.first().cloned();
        let mut start = common_actions_for(InputActionConfig::E1);
        if let Some(legacy_start) = input.start_key.clone()
            && !start.iter().any(|control| control == &legacy_start)
        {
            start.push(legacy_start);
        }
        if start.is_empty() {
            start.push("Q".to_string());
        }

        let start_str = kb_actions_for(InputActionConfig::E1)
            .into_iter()
            .next()
            .or_else(|| input.start_key.clone())
            .unwrap_or_else(|| start.first().cloned().unwrap_or_else(|| "Q".to_string()));
        let up_str =
            kb_keys_for(LaneConfig::Key6).into_iter().next().unwrap_or_else(|| "KEY6".to_string());
        let down_str =
            kb_keys_for(LaneConfig::Key4).into_iter().next().unwrap_or_else(|| "KEY4".to_string());
        let enter_str =
            merge_select_controls(kb_keys_for(LaneConfig::Key5), kb_keys_for(LaneConfig::Key7));
        let enter_str =
            if enter_str.is_empty() { String::new() } else { format!("/{}", enter_str.join("/")) };
        let back_str = kb_keys_for(LaneConfig::Key3)
            .into_iter()
            .next()
            .or_else(|| kb_actions_for(InputActionConfig::E2).into_iter().next())
            .unwrap_or_else(|| "KEY3".to_string());
        let key_hint = format!(
            "UP {up_str}  DOWN {down_str}  RIGHT{enter_str}:ENTER  LEFT/{back_str}:BACK  ENTER {start_str}"
        );
        let bga_str =
            kb_keys_for(LaneConfig::Key1).into_iter().next().unwrap_or_else(|| "?".to_string());
        let shortcut_hint = |action: InputActionConfig| {
            kb_actions_for(action).into_iter().next().unwrap_or_else(|| "-".to_string())
        };
        let open_folder_str = shortcut_hint(InputActionConfig::SelectOpenFolder);
        let reload_str = shortcut_hint(InputActionConfig::SelectReload);
        let autoplay_folder_str = shortcut_hint(InputActionConfig::SelectAutoplayFolder);
        let open_ir_str = shortcut_hint(InputActionConfig::SelectOpenIr);
        let key_config_str = shortcut_hint(InputActionConfig::SelectOpenKeyConfig);
        let screenshot_str = shortcut_hint(InputActionConfig::Screenshot);
        let rival_str = shortcut_hint(InputActionConfig::SelectRivalCycle);
        let documents_str = shortcut_hint(InputActionConfig::SelectOpenDocuments);
        let mode_filter_str = shortcut_hint(InputActionConfig::SelectModeFilter);
        let sort_str = shortcut_hint(InputActionConfig::SelectSort);
        let ln_mode_str = shortcut_hint(InputActionConfig::SelectLnMode);
        let replay_cycle_str = shortcut_hint(InputActionConfig::SelectReplayCycle);
        let same_folder_str = shortcut_hint(InputActionConfig::SelectSameFolder);
        let option_hint = format!(
            "F1 MENU  {open_folder_str}:FOLDER/HASH  {reload_str}:RELOAD  \
             {autoplay_folder_str}:AUTOPLAY  {open_ir_str}:IR  {key_config_str}:KEY CONFIG  \
             {screenshot_str}:SHOT  \
             {mode_filter_str}:MODE  {sort_str}:SORT  {ln_mode_str}:LN  \
             {replay_cycle_str}:CYCLE  {same_folder_str}:SAME  {rival_str}:RIVAL  {documents_str}:TEXT   \
             {start_str}:PLAY OPT  BACK:E2 OPT  {start_str}+BACK:DETAIL OPT  \
             {start_str}+K1/K2:1P ARR  {start_str}+K3:GAUGE  {start_str}+K5:HS-FIX  \
             BACK+K1..K7:ASSIST  \
             {start_str}+K8/K9:TARGET  {start_str}+{bga_str}:BGA  {start_str}+K4/K6:GREEN  {start_str}+K5/K7:TIMING  {start_str}+1..4:REPLAY  N5:REPLAY"
        );

        Self {
            start,
            e_action_controls,
            e2_action_controls,
            e3_action_controls,
            enter,
            back,
            key1_controls,
            key2_controls,
            key3_controls,
            key4_controls,
            key5_controls,
            key6_controls,
            key7_controls,
            key8_controls,
            key9_controls,
            key10_controls: Vec::new(),
            key11_controls: Vec::new(),
            key12_controls: Vec::new(),
            key13_controls: Vec::new(),
            key14_controls: Vec::new(),
            scratch_up_controls: Vec::new(),
            scratch_down_controls: Vec::new(),
            select_scratch_up_controls: Vec::new(),
            select_scratch_down_controls: Vec::new(),
            select_previous_controls,
            select_next_controls,
            target_previous_controls,
            target_next_controls,
            favorite_song_controls,
            favorite_chart_controls,
            same_folder_controls,
            mode_filter_controls,
            sort_controls,
            ln_mode_controls,
            difficulty_filter_controls,
            replay_cycle_controls,
            replay_play_controls,
            open_folder_controls,
            reload_controls,
            autoplay_folder_controls,
            open_ir_controls,
            open_key_config_controls,
            screenshot_controls,
            rival_cycle_controls,
            open_documents_controls,
            cycle_bga,
            key_hint,
            option_hint,
        }
    }

    pub(super) fn key_hint(&self) -> &str {
        &self.key_hint
    }

    pub(super) fn option_hint(&self) -> &str {
        &self.option_hint
    }

    pub(super) fn cycle_bga(&self) -> Option<&str> {
        self.cycle_bga.as_deref()
    }

    pub(super) fn is_enter(&self, control: &str) -> bool {
        contains(&self.enter, control)
    }

    pub(super) fn is_back(&self, control: &str) -> bool {
        contains(&self.back, control)
    }

    pub(super) fn is_start(&self, control: &str) -> bool {
        contains(&self.start, control)
    }

    pub(super) fn e_action_for_control(&self, control: &str) -> Option<InputActionConfig> {
        self.e_action_controls.iter().find_map(|(action, key)| (key == control).then_some(*action))
    }

    pub(super) fn is_key1(&self, control: &str) -> bool {
        contains(&self.key1_controls, control)
    }

    pub(super) fn is_key2(&self, control: &str) -> bool {
        contains(&self.key2_controls, control)
    }

    pub(super) fn is_key3(&self, control: &str) -> bool {
        contains(&self.key3_controls, control)
    }

    pub(super) fn is_key4(&self, control: &str) -> bool {
        contains(&self.key4_controls, control)
    }

    pub(super) fn is_key5(&self, control: &str) -> bool {
        contains(&self.key5_controls, control)
    }

    pub(super) fn is_key6(&self, control: &str) -> bool {
        contains(&self.key6_controls, control)
    }

    pub(super) fn is_key7(&self, control: &str) -> bool {
        contains(&self.key7_controls, control)
    }

    pub(super) fn is_key8(&self, control: &str) -> bool {
        contains(&self.key8_controls, control)
    }

    pub(super) fn is_key9(&self, control: &str) -> bool {
        contains(&self.key9_controls, control)
    }

    pub(super) fn is_key10(&self, control: &str) -> bool {
        contains(&self.key10_controls, control)
    }

    pub(super) fn is_key11(&self, control: &str) -> bool {
        contains(&self.key11_controls, control)
    }

    pub(super) fn is_key12(&self, control: &str) -> bool {
        contains(&self.key12_controls, control)
    }

    pub(super) fn is_key13(&self, control: &str) -> bool {
        contains(&self.key13_controls, control)
    }

    pub(super) fn is_key14(&self, control: &str) -> bool {
        contains(&self.key14_controls, control)
    }

    pub(super) fn is_ui_key1(&self, control: &str) -> bool {
        self.is_key1(control) || self.is_key8(control)
    }

    pub(super) fn is_ui_key2(&self, control: &str) -> bool {
        self.is_key2(control) || self.is_key9(control)
    }

    pub(super) fn is_ui_key3(&self, control: &str) -> bool {
        self.is_key3(control) || self.is_key10(control)
    }

    pub(super) fn is_ui_key4(&self, control: &str) -> bool {
        self.is_key4(control) || self.is_key11(control)
    }

    pub(super) fn is_ui_key5(&self, control: &str) -> bool {
        self.is_key5(control) || self.is_key12(control)
    }

    pub(super) fn is_ui_key6(&self, control: &str) -> bool {
        self.is_key6(control) || self.is_key13(control)
    }

    pub(super) fn is_ui_key7(&self, control: &str) -> bool {
        self.is_key7(control) || self.is_key14(control)
    }

    pub(super) fn ui_lane_for_control(&self, control: &str) -> Option<Lane> {
        if self.is_ui_key1(control) {
            Some(Lane::Key1)
        } else if self.is_ui_key2(control) {
            Some(Lane::Key2)
        } else if self.is_ui_key3(control) {
            Some(Lane::Key3)
        } else if self.is_ui_key4(control) {
            Some(Lane::Key4)
        } else if self.is_ui_key5(control) {
            Some(Lane::Key5)
        } else if self.is_ui_key6(control) {
            Some(Lane::Key6)
        } else if self.is_ui_key7(control) {
            Some(Lane::Key7)
        } else {
            None
        }
    }

    pub(super) fn is_e2_action(&self, control: &str) -> bool {
        contains(&self.e2_action_controls, control)
    }

    pub(super) fn is_e3_action(&self, control: &str) -> bool {
        contains(&self.e3_action_controls, control)
    }

    pub(super) fn is_scratch_up(&self, control: &str) -> bool {
        contains(&self.scratch_up_controls, control)
    }

    pub(super) fn is_scratch_down(&self, control: &str) -> bool {
        contains(&self.scratch_down_controls, control)
    }

    pub(super) fn is_select_scratch_up(&self, control: &str) -> bool {
        contains(&self.select_scratch_up_controls, control)
    }

    pub(super) fn is_select_scratch_down(&self, control: &str) -> bool {
        contains(&self.select_scratch_down_controls, control)
    }

    pub(super) fn is_select_previous(&self, control: &str) -> bool {
        contains(&self.select_previous_controls, control) || self.is_select_scratch_up(control)
    }

    pub(super) fn is_select_next(&self, control: &str) -> bool {
        contains(&self.select_next_controls, control) || self.is_select_scratch_down(control)
    }

    pub(super) fn is_target_previous(&self, control: &str) -> bool {
        contains(&self.target_previous_controls, control) || self.is_select_scratch_up(control)
    }

    pub(super) fn is_target_next(&self, control: &str) -> bool {
        contains(&self.target_next_controls, control) || self.is_select_scratch_down(control)
    }

    pub(super) fn is_favorite_song(&self, control: &str) -> bool {
        contains(&self.favorite_song_controls, control)
    }

    pub(super) fn is_favorite_chart(&self, control: &str) -> bool {
        contains(&self.favorite_chart_controls, control)
    }

    pub(super) fn is_same_folder(&self, control: &str) -> bool {
        contains(&self.same_folder_controls, control)
    }

    pub(super) fn is_mode_filter(&self, control: &str) -> bool {
        contains(&self.mode_filter_controls, control)
    }

    pub(super) fn is_sort(&self, control: &str) -> bool {
        contains(&self.sort_controls, control)
    }

    pub(super) fn is_ln_mode(&self, control: &str) -> bool {
        contains(&self.ln_mode_controls, control)
    }

    pub(super) fn is_difficulty_filter(&self, control: &str) -> bool {
        contains(&self.difficulty_filter_controls, control)
    }

    pub(super) fn is_replay_cycle(&self, control: &str) -> bool {
        contains(&self.replay_cycle_controls, control)
    }

    pub(super) fn is_replay_play(&self, control: &str) -> bool {
        contains(&self.replay_play_controls, control)
    }

    pub(super) fn is_open_folder(&self, control: &str) -> bool {
        contains(&self.open_folder_controls, control)
    }

    pub(super) fn is_reload(&self, control: &str) -> bool {
        contains(&self.reload_controls, control)
    }

    pub(super) fn is_autoplay_folder(&self, control: &str) -> bool {
        contains(&self.autoplay_folder_controls, control)
    }

    pub(super) fn is_open_ir(&self, control: &str) -> bool {
        contains(&self.open_ir_controls, control)
    }

    pub(super) fn is_open_key_config(&self, control: &str) -> bool {
        contains(&self.open_key_config_controls, control)
    }

    pub(super) fn is_screenshot(&self, control: &str) -> bool {
        contains(&self.screenshot_controls, control)
    }

    pub(super) fn is_rival_cycle(&self, control: &str) -> bool {
        contains(&self.rival_cycle_controls, control)
    }

    pub(super) fn is_open_documents(&self, control: &str) -> bool {
        contains(&self.open_documents_controls, control)
    }
}

/// アナログ tick の選曲スクロール寄与を返す。Next 方向を正とする。
/// scratch up/down にバインドされていない軸は `None`。
pub(super) fn select_analog_scroll_delta(
    axis: &str,
    ticks: i32,
    bindings: &SelectKeyBindings,
) -> Option<i32> {
    if ticks == 0 {
        return None;
    }
    let control = format!("{}{}", axis, if ticks > 0 { "+" } else { "-" });
    if bindings.is_select_scratch_down(&control) {
        Some(ticks.abs())
    } else if bindings.is_select_scratch_up(&control) {
        Some(-ticks.abs())
    } else {
        None
    }
}

pub(super) fn play_analog_lane_cover_delta(
    axis: &str,
    ticks: i32,
    bindings: &SelectKeyBindings,
) -> Option<i32> {
    if ticks == 0 {
        return None;
    }
    let control = format!("{}{}", axis, if ticks > 0 { "+" } else { "-" });
    if bindings.is_scratch_down(&control) {
        Some(ticks.abs())
    } else if bindings.is_scratch_up(&control) {
        Some(-ticks.abs())
    } else {
        None
    }
}

/// アナログスクロールバッファへ delta を蓄積する。
/// suppress 中は idle (200ms 以上の tick 途切れ) を観測するまで delta を捨てる。
/// idle 後の最初の delta から通常蓄積に戻る。
pub(super) fn update_analog_scroll_buffer(
    buffer: &mut i32,
    suppress: &mut bool,
    idle: bool,
    delta: i32,
) {
    if *suppress {
        if !idle {
            *buffer = 0;
            return;
        }
        *suppress = false;
    }
    if idle {
        *buffer = 0;
    }
    *buffer += delta;
}

/// バッファから ticks_per_scroll ごとの移動数を取り出す。端数はバッファに残す。
pub(super) fn take_analog_scroll_steps(buffer: &mut i32, ticks_per_scroll: i32) -> i32 {
    let steps = *buffer / ticks_per_scroll;
    *buffer %= ticks_per_scroll;
    steps
}

fn push_scratch_controls(
    entry: &BindingConfigEntry,
    up_controls: &mut Vec<String>,
    down_controls: &mut Vec<String>,
) {
    let control = entry.control.clone();
    // 明示の direction タグを最優先し、無ければコントロール名から推測する。
    match entry.scratch {
        Some(ScratchDirectionConfig::Up) => {
            push_scratch_control(up_controls, down_controls, control)
        }
        Some(ScratchDirectionConfig::Down) => {
            push_scratch_control(down_controls, up_controls, control);
        }
        None => {
            if is_scratch_up_control(&control) || is_legacy_keyboard_scratch_up_control(&control) {
                push_scratch_control(up_controls, down_controls, control);
            } else if is_scratch_down_control(&control)
                || is_legacy_keyboard_scratch_down_control(&control)
            {
                push_scratch_control(down_controls, up_controls, control);
            } else {
                push_unique_control(up_controls, control.clone());
                push_unique_control(down_controls, control);
            }
        }
    }
}

/// scratch direction が保存されていなかった旧 keyboard 設定の既定方向。
///
/// 旧 profile は `scratch` フィールドなしで Shift / Control を保存していたため、
/// 方向を推測できないまま両方の選曲移動へ登録されていた。
fn is_legacy_keyboard_scratch_up_control(control: &str) -> bool {
    matches!(control, "LShift" | "RShift")
}

fn is_legacy_keyboard_scratch_down_control(control: &str) -> bool {
    matches!(control, "LControl" | "RControl")
}

fn push_scratch_control(
    target_controls: &mut Vec<String>,
    opposite_controls: &[String],
    control: String,
) {
    if opposite_controls.iter().any(|existing| existing == &control) {
        return;
    }
    push_unique_control(target_controls, control);
}

fn push_unique_control(controls: &mut Vec<String>, control: String) {
    if !controls.iter().any(|existing| existing == &control) {
        controls.push(control);
    }
}

fn merge_select_controls(configured: Vec<String>, lane_controls: Vec<String>) -> Vec<String> {
    let mut merged = configured;
    for control in lane_controls {
        if !merged.iter().any(|existing| existing == &control) {
            merged.push(control);
        }
    }
    merged
}

fn select_controls_with_default(configured: Vec<String>, default_control: &str) -> Vec<String> {
    if configured.is_empty() { vec![default_control.to_string()] } else { configured }
}

fn with_numeric_keypad_aliases(
    mut controls: Vec<String>,
    keyboard_bindings: &[&BindingConfigEntry],
) -> Vec<String> {
    let configured = controls.clone();
    for control in configured {
        let Some(alias) = numeric_keypad_alias(control.as_str()) else { continue };
        if controls.iter().any(|existing| existing == alias)
            || keyboard_bindings.iter().any(|entry| entry.control == alias)
        {
            continue;
        }
        controls.push(alias.to_string());
    }
    controls
}

fn numeric_keypad_alias(control: &str) -> Option<&'static str> {
    match control {
        "0" => Some("Numpad0"),
        "1" => Some("Numpad1"),
        "2" => Some("Numpad2"),
        "3" => Some("Numpad3"),
        "4" => Some("Numpad4"),
        "5" => Some("Numpad5"),
        "6" => Some("Numpad6"),
        "7" => Some("Numpad7"),
        "8" => Some("Numpad8"),
        "9" => Some("Numpad9"),
        _ => None,
    }
}

fn common_control_matches(
    common_controls: &HashSet<(String, String)>,
    device: &str,
    control: &str,
) -> bool {
    common_controls.iter().any(|(common_device, common)| {
        (common_device == device
            || (is_gamepad_device(common_device) && device.eq_ignore_ascii_case("gamepad"))
            || (is_gamepad_device(device) && common_device.eq_ignore_ascii_case("gamepad")))
            && controls_match(common, control)
    })
}

fn controls_match(left: &str, right: &str) -> bool {
    left == right
        || numeric_keypad_alias(left) == Some(right)
        || numeric_keypad_alias(right) == Some(left)
}

fn contains(controls: &[String], control: &str) -> bool {
    controls.iter().any(|configured| configured == control)
}
