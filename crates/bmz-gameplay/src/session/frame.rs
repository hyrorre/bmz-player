pub fn sync_judge_windows(session: &mut GameSession, now: TimeUs) {
    let percent = judge_percent_at_time_for_keymode(
        session.chart.metadata.judge_rank_spec,
        &session.chart.judge_rank_events,
        now,
        session.primary_key_mode,
        session.rule_mode,
    );
    let windows = judge_windows_for_rule_mode_and_keymode(
        session.base_judge_windows,
        percent,
        session.rule_mode,
        session.primary_key_mode,
    );
    session.judge.set_window_set(scale_judge_windows_for_playback_rate(
        windows,
        session.audio_clock.playback_rate_percent(),
    ));
}

use super::judgement::{
    update_failed_state_from_gauge, update_gauge_increase_timer_state, update_gauge_max_timer,
    update_gauge_max_timer_state,
};

pub(super) fn sync_input_timestamp_anchor(session: &mut GameSession, audio_now: TimeUs) {
    session.input_timestamp_anchor = if session.audio_clock.running {
        Some(InputTimestampAnchor { monotonic_ns: monotonic_timestamp_ns(), audio_time: audio_now })
    } else {
        None
    };
}

pub fn advance_session_frame(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
) -> SessionFrame {
    let times = compute_frame_times(session);
    sync_input_timestamp_anchor(session, times.audio_now);
    rebase_pre_ready_visual_times(session, times.audio_now);
    update_scratch_angle_phase(session, times.audio_now);
    let mut judgements = Vec::new();

    if session.state == PlayState::Ready && times.audio_now.0 < 0 {
        drain_pre_ready_visual_inputs(session, times.audio_now);
    } else if session.state == PlayState::Ready {
        session.state = PlayState::Playing;
    }

    if matches!(session.state, PlayState::Ready | PlayState::Playing) {
        // BGMはchart 0に間に合うようREADY中もschedule-aheadする。
        // 判定・keysound・MineはPlayingに入るまで開始しない。
        session.bgm_scheduler.schedule_until(
            &session.chart,
            &session.audio_clock,
            times.audio_schedule_until,
            session.audio_mix.master_volume
                * session.audio_mix.effective_normalization_gain()
                * session.audio_mix.bgm_volume,
            audio,
        );

        // キー音自動再生モード: ノーツの押下有無に関わらず、譜面の生タイミングで
        // キー音を鳴らす。入力オフセット・表示オフセットは適用しない。
        // 押鍵時のキー音は `schedule_keysounds` 側で抑制する。
        if session.audio_mix.auto_keysound {
            session.auto_keysound_scheduler.schedule_until(
                &session.chart,
                &session.display_only_lane_mask,
                &session.audio_clock,
                times.audio_schedule_until,
                session.audio_mix.master_volume
                    * session.audio_mix.effective_normalization_gain()
                    * session.audio_mix.key_volume,
                audio,
            );
        }
    }

    if session.state == PlayState::Playing {
        sync_judge_windows(session, times.audio_now);

        if session.replay_player.is_some() && session.replay_lane_mask.is_none() {
            drain_human_inputs(session);
            judgements.extend(process_replay_inputs(session, times.audio_now));
        } else {
            if session.autoplay.as_ref().is_some_and(AutoplayController::is_full) {
                // フルオート中は人間のキー入力を判定にも視覚エフェクトにも渡さない。
                // キービームは process_autoplay_inputs 側(ノーツ処理時)で発火する。
                // (ハイスピード等のオプション操作は app 側で別途処理される)
                discard_human_inputs(session);
            } else {
                judgements.extend(process_human_inputs(session));
            }
            if session.replay_player.is_some() {
                judgements.extend(process_replay_inputs(session, times.audio_now));
            }
            judgements.extend(process_autoplay_inputs(session, times.audio_now));
            if session.autoplay.is_some() {
                apply_auto_key_release(session, times.audio_now);
            }
        }
        judgements.extend(process_mine_passes(session, times.audio_now));
        judgements.extend(process_misses(session, times.audio_now));
        update_hcn_lane_timers(session, times.audio_now);
        apply_hcn_gauge(session, times.audio_now);
        update_failed_state_from_gauge(session);
        schedule_keysounds(session, audio);
        update_recent_judgements(session, &judgements, times.audio_now);
        update_full_combo_timer(session, &judgements);
        advance_battle_opponent(session, times.audio_now);

        if should_finish(session, times.audio_now) {
            session.state = PlayState::Finished;
        }
    }
    update_gauge_max_timer(session, times.audio_now);

    let mine_hits = std::mem::take(&mut session.pending_mine_hits);
    let keysound_volumes = std::mem::take(&mut session.pending_keysound_volumes);
    let skin_events = std::mem::take(&mut session.pending_skin_events);
    SessionFrame {
        times,
        judgements,
        mine_hits,
        keysound_volumes,
        skin_events,
        state: session.state,
    }
}

/// Viewer の途中再生開始前に、過去の入力・判定 cursor を無音で進める。
/// 開始時刻より前に判定時刻を迎えたscore対象ノートはPGREATとして事前集計し、
/// 境界時刻のノートはその位置からのAutoplay対象として残す。
pub fn prepare_viewer_seek(session: &mut GameSession, start_time: TimeUs) {
    session.judge.skip_before(&session.chart, start_time);
    apply_viewer_pgreat_prefix(session, start_time);
    if let Some(autoplay) = &mut session.autoplay {
        autoplay.skip_before(&session.chart, start_time);
        for lane in Lane::ALL {
            if autoplay.is_lane_enabled(lane)
                && session.judge.lanes[lane.index()].active_long.is_some()
            {
                session.lane_keyon_started_at[lane.index()] = Some(start_time);
                session.lane_keyoff_started_at[lane.index()] = None;
                session.lane_auto_release_at[lane.index()] = None;
            }
        }
    }
    if let Some(replay) = &mut session.replay_player {
        replay.skip_before(start_time);
    }
    if let Some(opponent) = &mut session.battle_opponent {
        opponent.judge.skip_before(&opponent.chart, start_time);
        apply_battle_opponent_viewer_pgreat_prefix(opponent, start_time);
        if let Some(autoplay) = &mut opponent.autoplay {
            autoplay.skip_before(&opponent.chart, start_time);
            for lane in Lane::ALL {
                if autoplay.is_lane_enabled(lane)
                    && opponent.judge.lanes[lane.index()].active_long.is_some()
                {
                    opponent.lane_keyon_started_at[lane.index()] = Some(start_time);
                }
            }
        }
        if let Some(replay) = &mut opponent.replay_player {
            replay.skip_before(start_time);
        }
    }
    session.bgm_scheduler = BgmScheduler::starting_at(&session.chart, start_time);
}

fn apply_viewer_pgreat_prefix(session: &mut GameSession, start_time: TimeUs) {
    let display_only_lane_mask = session.display_only_lane_mask;
    let events = viewer_pgreat_prefix_events(&session.chart, start_time, |lane| {
        !display_only_lane_mask[lane.index()]
    });
    for event in events {
        session.score.apply(&event);
        session.gauge.apply_judge(Judge::PGreat, 1.0);
        if let Some(note_id) = event.note_id {
            session.judge.judged_notes.insert(note_id, Judge::PGreat);
            session.result_judgements.insert(
                note_id,
                ResultJudgementDetail {
                    judge: Judge::PGreat,
                    side: TimingSide::Slow,
                    delta: TimeUs(0),
                    time: event.time,
                },
            );
        }
    }
    session.course_max_combo = session.course_max_combo.max(session.display_combo());
    if session.scored_total_notes != 0
        && session.score.past_notes == session.scored_total_notes
        && session.score.combo == session.scored_total_notes
    {
        session.full_combo_started_at = Some(start_time);
    }

    if session.battle_opponent.is_none() {
        let opponent_events = viewer_pgreat_prefix_events(&session.chart, start_time, |lane| {
            display_only_lane_mask[lane.index()]
        });
        for event in opponent_events {
            if let Some(score) = &mut session.opponent_score {
                score.apply(&event);
            }
            if let Some(gauge) = &mut session.opponent_gauge {
                gauge.apply_judge(Judge::PGreat, 1.0);
            }
        }
        if session.scored_total_notes != 0
            && session.opponent_score.as_ref().is_some_and(|score| {
                score.past_notes == session.scored_total_notes
                    && score.combo == session.scored_total_notes
            })
        {
            session.opponent_full_combo_started_at = Some(start_time);
        }
    }
}

fn apply_battle_opponent_viewer_pgreat_prefix(
    opponent: &mut BattleOpponentSession,
    start_time: TimeUs,
) {
    for event in viewer_pgreat_prefix_events(&opponent.chart, start_time, |_| true) {
        opponent.score.apply(&event);
        opponent.gauge.apply_judge(Judge::PGreat, 1.0);
    }
    if opponent.scored_total_notes != 0
        && opponent.score.past_notes == opponent.scored_total_notes
        && opponent.score.combo == opponent.scored_total_notes
    {
        opponent.full_combo_started_at = Some(start_time);
    }
}

fn viewer_pgreat_prefix_events(
    chart: &PlayableChart,
    start_time: TimeUs,
    mut includes_lane: impl FnMut(Lane) -> bool,
) -> Vec<JudgementEvent> {
    let mut events = Vec::new();
    for lane in Lane::ALL {
        if !includes_lane(lane) {
            continue;
        }
        for note in chart.notes_for_lane(lane).iter().filter(|note| note.time < start_time) {
            let note_id = match note.kind {
                NoteKind::Tap => Some(note.id),
                NoteKind::LongStart => chart
                    .long_notes
                    .iter()
                    .find(|pair| pair.start_note_id == note.id)
                    .filter(|pair| {
                        matches!(
                            pair.mode.unwrap_or(chart.metadata.long_note_mode),
                            LongNoteMode::Cn | LongNoteMode::Hcn
                        )
                    })
                    .map(|_| note.id),
                NoteKind::LongEnd => {
                    chart.long_notes.iter().find(|pair| pair.end_note_id == note.id).map(|pair| {
                        if pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Ln {
                            pair.start_note_id
                        } else {
                            pair.end_note_id
                        }
                    })
                }
                NoteKind::Invisible | NoteKind::Mine => None,
            };
            let Some(note_id) = note_id else {
                continue;
            };
            events.push(JudgementEvent {
                note_id: Some(note_id),
                lane,
                judge: Judge::PGreat,
                side: TimingSide::Slow,
                delta: TimeUs(0),
                time: note.time,
                affects_score: true,
            });
        }
    }
    events.sort_by_key(|event| (event.time, event.lane.index(), event.note_id));
    events
}

fn advance_battle_opponent(session: &mut GameSession, now: TimeUs) {
    let playback_rate_percent = session.audio_clock.playback_rate_percent();
    let Some(opponent) = &mut session.battle_opponent else {
        return;
    };
    let display_uses_primary_arrangement = opponent.display_uses_primary_arrangement;
    let publish_display_judgements = opponent.publish_display_judgements;

    let percent = judge_percent_at_time_for_keymode(
        opponent.chart.metadata.judge_rank_spec,
        &opponent.chart.judge_rank_events,
        now,
        opponent.key_mode,
        opponent.rule_mode,
    );
    let windows = judge_windows_for_rule_mode_and_keymode(
        opponent.base_judge_windows,
        percent,
        opponent.rule_mode,
        opponent.key_mode,
    );
    opponent
        .judge
        .set_window_set(scale_judge_windows_for_playback_rate(windows, playback_rate_percent));

    let inputs = if let Some(replay) = &mut opponent.replay_player {
        replay.poll_until(now)
    } else if let Some(autoplay) = &mut opponent.autoplay {
        autoplay.poll_until(&opponent.chart, now)
    } else {
        Vec::new()
    };
    let mut display_judgements = Vec::new();
    for input in inputs {
        match input.kind {
            InputKind::Press => {
                opponent.lane_keyon_started_at[input.lane.index()] = Some(input.time)
            }
            InputKind::Release => opponent.lane_keyon_started_at[input.lane.index()] = None,
        }
        let outcome = opponent.judge.process_input(&opponent.chart, input);
        display_judgements.extend(apply_battle_opponent_outcome(opponent, outcome));
    }
    let mine_outcome =
        opponent.judge.process_mine_passes(&opponent.chart, now, &opponent.lane_keyon_started_at);
    display_judgements.extend(apply_battle_opponent_outcome(opponent, mine_outcome));
    let miss_outcome = opponent.judge.process_misses(&opponent.chart, now);
    display_judgements.extend(apply_battle_opponent_outcome(opponent, miss_outcome));
    update_battle_opponent_skin_timers(opponent, &display_judgements, now);

    if !publish_display_judgements {
        return;
    }

    for display in &mut display_judgements {
        let source_lane = if display_uses_primary_arrangement {
            display
                .judgement
                .note_id
                .and_then(|note_id| session.chart.note_by_id(note_id))
                .map_or(display.judgement.lane, |note| note.lane)
        } else {
            display.judgement.lane
        };
        display.judgement.lane = second_player_lane(source_lane);
        display.judgement.affects_score = false;
        push_skin_runtime_event(
            session,
            SkinRuntimeEventKind::Judgement(display.judgement.clone()),
        );
    }
    session
        .recent_judgements
        .extend(display_judgements.iter().map(|display| display.judgement.clone()));
    session.recent_display_judgements.extend(display_judgements);
}

fn apply_battle_opponent_outcome(
    opponent: &mut BattleOpponentSession,
    outcome: JudgeOutcome,
) -> Vec<DisplayJudgementEvent> {
    let mut display_judgements = Vec::with_capacity(outcome.events.len());
    for event in outcome.events {
        if event.affects_score {
            opponent.score.apply(&event);
            let previous_gauge = opponent.gauge.current().value;
            opponent.gauge.apply_judge(event.judge, 1.0);
            let current = opponent.gauge.current();
            update_gauge_increase_timer_state(
                &mut opponent.gauge_increase_started_at,
                previous_gauge,
                current.value,
                current.definition.max,
                event.time,
            );
        }
        display_judgements
            .push(DisplayJudgementEvent { judgement: event, combo: opponent.score.combo });
    }
    for mine in outcome.mine_hits {
        opponent.gauge.apply_mine(mine.damage);
    }
    display_judgements
}

fn update_battle_opponent_skin_timers(
    opponent: &mut BattleOpponentSession,
    display_judgements: &[DisplayJudgementEvent],
    now: TimeUs,
) {
    let current = opponent.gauge.current();
    update_gauge_max_timer_state(
        &mut opponent.gauge_increase_started_at,
        &mut opponent.gauge_max_started_at,
        current.value,
        current.definition.max,
        now,
    );
    if opponent.full_combo_started_at.is_some()
        || opponent.scored_total_notes == 0
        || opponent.score.past_notes < opponent.scored_total_notes
        || opponent.score.combo < opponent.scored_total_notes
    {
        return;
    }
    opponent.full_combo_started_at = display_judgements
        .iter()
        .rev()
        .find(|display| display.judgement.affects_score && display.judgement.note_id.is_some())
        .map(|display| display.judgement.time)
        .or(Some(TimeUs(now.0.max(0))));
}

fn second_player_lane(lane: Lane) -> Lane {
    match lane {
        Lane::Scratch => Lane::Scratch2,
        Lane::Key1 => Lane::Key8,
        Lane::Key2 => Lane::Key9,
        Lane::Key3 => Lane::Key10,
        Lane::Key4 => Lane::Key11,
        Lane::Key5 => Lane::Key12,
        Lane::Key6 => Lane::Key13,
        Lane::Key7 => Lane::Key14,
        lane => lane,
    }
}

pub(super) fn update_full_combo_timer(session: &mut GameSession, judgements: &[JudgementEvent]) {
    update_opponent_full_combo_timer(session, judgements);
    if session.full_combo_started_at.is_some()
        || session.scored_total_notes == 0
        || session.score.past_notes < session.scored_total_notes
        || session.score.combo < session.scored_total_notes
    {
        return;
    }
    session.full_combo_started_at = judgements
        .iter()
        .rev()
        .find(|event| event.affects_score && event.note_id.is_some())
        .map(|event| event.time)
        .or_else(|| Some(session.audio_clock.now()));
}

fn update_opponent_full_combo_timer(session: &mut GameSession, judgements: &[JudgementEvent]) {
    let Some(score) = &session.opponent_score else {
        return;
    };
    if session.opponent_full_combo_started_at.is_some()
        || session.scored_total_notes == 0
        || score.past_notes < session.scored_total_notes
        || score.combo < session.scored_total_notes
    {
        return;
    }
    session.opponent_full_combo_started_at = judgements
        .iter()
        .rev()
        .find(|event| session.display_only_lane_mask[event.lane.index()] && event.note_id.is_some())
        .map(|event| event.time)
        .or_else(|| Some(session.audio_clock.now()));
}

pub fn should_finish(session: &GameSession, audio_now: TimeUs) -> bool {
    session.judge.is_exhausted(&session.chart)
        && session.bgm_scheduler.is_done(&session.chart)
        && audio_now.0 > session.chart.end_time.0.saturating_add(SESSION_END_MARGIN_US)
}

/// 最終ノーツに対する Poor / Empty Poor / Mine の SLOW 側受付が終了し、
/// これ以上スコアが変化しない状態かを返す。
pub fn result_is_settled(session: &GameSession, audio_now: TimeUs) -> bool {
    let result_settle_at =
        session.chart.end_time.0.saturating_add(session.judge.window_set.result_settle_margin_us());
    session.judge.is_exhausted(&session.chart) && audio_now.0 > result_settle_at
}
use super::*;
