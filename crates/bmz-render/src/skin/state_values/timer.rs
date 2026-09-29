use super::*;

pub(super) fn skin_timer_elapsed_ms(timer: Option<i32>, state: &SkinDrawState) -> Option<i32> {
    if let Some(start) = timer.and_then(|id| state.lua_custom_timer_us.get(&id)) {
        return start.map(|start| {
            (i64::from(state.elapsed_ms).saturating_mul(1_000).saturating_sub(start) / 1_000)
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
        });
    }
    match timer {
        None => Some(state.elapsed_ms),
        Some(0) => Some(state.elapsed_ms),
        Some(1) => state.start_input_ms,
        Some(2) => state.fadeout_ms,
        Some(3) => state.failed_ms,
        Some(SKIN_TIMER_BMZ_INPUT_BASE..=SKIN_TIMER_BMZ_INPUT_LAST) => {
            state.logical_input_press_ms[(timer.unwrap() - SKIN_TIMER_BMZ_INPUT_BASE) as usize]
        }
        Some(SKIN_TIMER_BMZ_E1_E2_PRESS) => state.e1_e2_press_ms,
        Some(SKIN_TIMER_BMZ_E1_E2_RELEASE) => state.e1_e2_release_ms,
        Some(SKIN_TIMER_BMZ_JUDGE_LANE_BASE..=SKIN_TIMER_BMZ_JUDGE_LANE_LAST) => {
            state.judge_lane_ms[(timer.unwrap() - SKIN_TIMER_BMZ_JUDGE_LANE_BASE) as usize]
        }
        Some(150) => state.result_graph_begin_ms,
        Some(151) => state.result_graph_end_ms,
        Some(152) => state.result_update_score_ms,
        // TIMER_IR_CONNECT_BEGIN/SUCCESS/FAIL.
        Some(172) => state.ir_ranking.connect_begin_ms,
        Some(173) => state.ir_ranking.connect_success_ms,
        Some(174) => state.ir_ranking.connect_fail_ms,
        Some(40) => state.ready_timer_ms,
        Some(41) => state.play_timer_ms,
        Some(140) => state.rhythm_timer_ms,
        Some(42) => state.gauge_increase_ms,
        Some(43) => state.gauge_increase_2p_ms,
        Some(44) => state.gauge_max_ms,
        Some(45) => state.gauge_max_2p_ms,
        Some(11) => Some(state.select_bar_elapsed_ms),
        Some(21..=26) => (state.select_option_panel == (timer.unwrap() - 20) as u8)
            .then_some(state.select_option_panel_elapsed_ms),
        Some(31..=36) => state.select_option_panel_off_elapsed_ms[(timer.unwrap() - 31) as usize],
        Some(348..=352) => score_target_timer_elapsed_ms(timer.unwrap(), state),
        Some(46) => state.judge_ms[0],
        Some(47) => state.judge_ms[1],
        Some(247) => state.judge_ms[2],
        Some(446) => state.judge_ms[0],
        Some(447) => state.judge_ms[1],
        Some(448) => state.judge_ms[2],
        Some(48) => state.full_combo_ms,
        Some(49) => state.full_combo_2p_ms,
        Some(908) => state.music_end_ms,
        Some(50..=57) => state.bomb_ms[(timer.unwrap() - 50) as usize],
        Some(58..=59) => state.bomb_ms[Lane::Key8.index() + (timer.unwrap() - 58) as usize],
        // 2P bomb: timer 60=Scratch2, 61-67=Key8-14
        Some(60) => state.bomb_ms[Lane::Scratch2.index()],
        Some(61..=67) => state.bomb_ms[Lane::Key8.index() + (timer.unwrap() - 61) as usize],
        // 1P hold: timer 70=Scratch, 71-77=Key1-7
        Some(70..=77) => state.hold_ms[(timer.unwrap() - 70) as usize],
        Some(78..=79) => state.hold_ms[Lane::Key8.index() + (timer.unwrap() - 78) as usize],
        // 2P hold: timer 80=Scratch2, 81-87=Key8-14
        Some(80) => state.hold_ms[Lane::Scratch2.index()],
        Some(81..=87) => state.hold_ms[Lane::Key8.index() + (timer.unwrap() - 81) as usize],
        Some(100..=107) => state.keyon_ms[(timer.unwrap() - 100) as usize],
        Some(108..=109) => state.keyon_ms[Lane::Key8.index() + (timer.unwrap() - 108) as usize],
        // 2P keyon: timer 110=Scratch2, 111-117=Key8-14
        Some(110) => state.keyon_ms[Lane::Scratch2.index()],
        Some(111..=117) => state.keyon_ms[Lane::Key8.index() + (timer.unwrap() - 111) as usize],
        Some(120..=127) => state.keyoff_ms[(timer.unwrap() - 120) as usize],
        Some(128..=129) => state.keyoff_ms[Lane::Key8.index() + (timer.unwrap() - 128) as usize],
        // 2P keyoff: timer 130=Scratch2, 131-137=Key8-14
        Some(130) => state.keyoff_ms[Lane::Scratch2.index()],
        Some(131..=137) => state.keyoff_ms[Lane::Key8.index() + (timer.unwrap() - 131) as usize],
        Some(143) => state.end_of_note_ms,
        Some(144) => state.end_of_note_2p_ms,
        // 1P HCN active: timer 250=Scratch, 251-257=Key1-7
        Some(250..=257) => state.hcn_active_ms[(timer.unwrap() - 250) as usize],
        Some(258..=259) => {
            state.hcn_active_ms[Lane::Key8.index() + (timer.unwrap() - 258) as usize]
        }
        // 2P HCN active: timer 260=Scratch2, 261-267=Key8-14
        Some(260) => state.hcn_active_ms[Lane::Scratch2.index()],
        Some(261..=267) => {
            state.hcn_active_ms[Lane::Key8.index() + (timer.unwrap() - 261) as usize]
        }
        // 1P HCN damage: timer 270=Scratch, 271-277=Key1-7
        Some(270..=277) => state.hcn_damage_ms[(timer.unwrap() - 270) as usize],
        Some(278..=279) => {
            state.hcn_damage_ms[Lane::Key8.index() + (timer.unwrap() - 278) as usize]
        }
        // 2P HCN damage: timer 280=Scratch2, 281-287=Key8-14
        Some(280) => state.hcn_damage_ms[Lane::Scratch2.index()],
        Some(281..=287) => {
            state.hcn_damage_ms[Lane::Key8.index() + (timer.unwrap() - 281) as usize]
        }
        Some(id)
            if (SKIN_DYNAMIC_TIMER_BASE
                ..SKIN_DYNAMIC_TIMER_BASE + SKIN_DYNAMIC_TIMER_COUNT as i32)
                .contains(&id) =>
        {
            let idx = (id - SKIN_DYNAMIC_TIMER_BASE) as usize;
            state.dynamic_timer_ms[idx]
        }
        Some(id) => state.fixed_delay_timer_ms.get(&id).copied(),
    }
}

/// renderer runtime を持たない単発の state 構築用フォールバック。
/// 実描画では [`DynamicTimerRuntime::start_input_elapsed_ms`] が最初の発火フレームを
/// 0 ms としてラッチする。ここでも開始条件の `now > skin.input` は合わせる。
pub fn skin_start_input_elapsed_ms(elapsed_ms: i32, input_ms: i32) -> Option<i32> {
    (elapsed_ms > input_ms).then_some(elapsed_ms.saturating_sub(input_ms))
}
