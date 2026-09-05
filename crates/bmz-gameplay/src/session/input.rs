pub fn apply_input_offset_auto_adjust(session: &mut GameSession, events: &[JudgementEvent]) {
    let Some(state) = &mut session.input_offset_auto_adjust else {
        return;
    };
    // beatoraja/LR2-style judge timing adjustment shifts the note display timing.
    // The low-level input offset remains a separate BMZ-only calibration knob.
    for event in events {
        if !event.affects_score
            || event.note_id.is_none()
            || !counts_for_input_offset_auto_adjust(event.judge)
        {
            continue;
        }
        state.sum_delta_us = state.sum_delta_us.saturating_add(event.delta.0);
        state.count = state.count.saturating_add(1);
        if state.count < INPUT_OFFSET_AUTO_ADJUST_BATCH {
            continue;
        }

        let offset_delta = match state.sum_delta_us.cmp(&0) {
            std::cmp::Ordering::Greater => INPUT_OFFSET_AUTO_ADJUST_STEP_US,
            std::cmp::Ordering::Less => -INPUT_OFFSET_AUTO_ADJUST_STEP_US,
            std::cmp::Ordering::Equal => 0,
        };
        if offset_delta != 0 {
            session.offsets.visual_offset_us = session
                .offsets
                .visual_offset_us
                .saturating_add(offset_delta)
                .clamp(INPUT_OFFSET_AUTO_ADJUST_MIN_US, INPUT_OFFSET_AUTO_ADJUST_MAX_US);
        }
        *state = InputOffsetAutoAdjustState::default();
    }
}

use super::audio::counts_for_input_offset_auto_adjust;
use super::judgement::push_skin_runtime_event;
use super::state::{INPUT_OFFSET_AUTO_ADJUST_BATCH, INPUT_OFFSET_AUTO_ADJUST_STEP_US};

pub fn update_recent_judgements(session: &mut GameSession, events: &[JudgementEvent], now: TimeUs) {
    for event in events {
        if !event.affects_score {
            continue;
        }
        session.hit_error_ring.push_judgement(event.judge, event.delta.0);
    }
    session.recent_judgements.extend(
        events
            .iter()
            .filter(|event| {
                event.affects_score || session.display_only_lane_mask[event.lane.index()]
            })
            .cloned(),
    );
    session.recent_judgements.retain(|event| now.0 <= event.time.0 + JUDGEMENT_DISPLAY_US);
    session
        .recent_display_judgements
        .retain(|event| now.0 <= event.judgement.time.0 + JUDGEMENT_DISPLAY_US);
}

pub fn update_recent_inputs(session: &mut GameSession, inputs: &[InputEvent], now: TimeUs) {
    session
        .recent_inputs
        .extend(inputs.iter().copied().filter(|input| input.kind == InputKind::Press));
    session.recent_inputs.retain(|input| now.0 <= input.time.0 + INPUT_DISPLAY_US);
}

/// レーンごとのキー押下/解放状態を更新する。beatoraja の KEYON/KEYOFF タイマー相当。
/// Press → keyon を新しい時刻にセット、keyoff をクリア(autoplay 入力なら 80ms 後の自動 release も予約)。
/// Release → 押下中だった場合のみ keyoff をセット、keyon と auto_release_at をクリア。
pub fn update_lane_key_states(session: &mut GameSession, inputs: &[InputEvent]) {
    for input in inputs {
        let lane_index = input.lane.index();
        match input.kind {
            InputKind::Press => {
                session.lane_keyon_started_at[lane_index] = Some(input.time);
                session.lane_keyoff_started_at[lane_index] = None;
                session.lane_scratch_direction[lane_index] = input.scratch_direction;
                session.lane_auto_release_at[lane_index] = match input.source {
                    InputSource::Auto => Some(TimeUs(input.time.0 + AUTO_KEYBEAM_DURATION_US)),
                    _ => None,
                };
            }
            InputKind::Release => {
                if session.lane_keyon_started_at[lane_index].is_some() {
                    session.lane_keyoff_started_at[lane_index] = Some(input.time);
                    session.lane_keyon_started_at[lane_index] = None;
                }
                if input.scratch_direction.is_none()
                    || session.lane_scratch_direction[lane_index] == input.scratch_direction
                {
                    session.lane_scratch_direction[lane_index] = None;
                }
                session.lane_auto_release_at[lane_index] = None;
            }
        }
    }
}

/// オートプレイで Press したレーンを `AUTO_KEYBEAM_DURATION_US` 経過後に自動で release する。
/// beatoraja の `auto_minduration` (80ms) 経過で `auto_presstime` を MIN_VALUE に戻す挙動に対応。
/// beatoraja 同様、LN ホールド中 (`state.processing != null`) は release しない。
pub fn apply_auto_key_release(session: &mut GameSession, audio_now: TimeUs) {
    for lane_index in 0..LANE_COUNT {
        if let Some(release_at) = session.lane_auto_release_at[lane_index]
            && audio_now.0 >= release_at.0
            && session.judge.lanes[lane_index].active_long.is_none()
        {
            push_skin_runtime_event(
                session,
                SkinRuntimeEventKind::Input(InputEvent {
                    lane: Lane::ALL[lane_index],
                    kind: InputKind::Release,
                    time: release_at,
                    source: InputSource::Auto,
                    device_kind: Default::default(),
                    scratch_direction: session.lane_scratch_direction[lane_index],
                }),
            );
            session.lane_keyoff_started_at[lane_index] = Some(release_at);
            session.lane_keyon_started_at[lane_index] = None;
            session.lane_scratch_direction[lane_index] = None;
            session.lane_auto_release_at[lane_index] = None;
        }
    }
}

pub fn process_human_inputs(session: &mut GameSession) -> Vec<JudgementEvent> {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let mut inputs = session.input_system.collect_game_inputs(&ctx);
    if let Some(autoplay) = &session.autoplay {
        inputs.retain(|input| !autoplay.is_lane_enabled(input.lane));
    }
    if let Some(replay_lane_mask) = &session.replay_lane_mask {
        inputs.retain(|input| !replay_lane_mask[input.lane.index()]);
    }
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);
    let mut judgements = Vec::new();
    for input in inputs {
        if !session.display_only_lane_mask[input.lane.index()] {
            if let Some(projection) = &session.replay_lane_projection {
                if let Some(source_lane) = projection.record_to_source[input.lane.index()] {
                    session.replay_recorder.record(InputEvent { lane: source_lane, ..input });
                }
            } else {
                session.replay_recorder.record(input);
            }
        }
        let events = process_session_input(session, input);
        apply_input_offset_auto_adjust(session, &events);
        judgements.extend(events);
    }
    judgements
}

/// リプレイ再生中は人間入力を判定に渡さない。視覚エフェクト用に recent_inputs だけ更新する。
pub fn drain_human_inputs(session: &mut GameSession) {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let inputs = session.input_system.collect_game_inputs(&ctx);
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);
}

/// READY 開始前の play 導入中に、判定へは渡さず keybeam / lazer 用の lane key 状態だけ更新する。
/// 入力時刻は壁時計ベースの `visual_now` に揃える。
pub fn drain_pre_ready_visual_inputs(session: &mut GameSession, visual_now: TimeUs) {
    if session.autoplay.as_ref().is_some_and(AutoplayController::is_full) {
        discard_human_inputs(session);
        return;
    }

    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: None,
    };
    let inputs = session.input_system.collect_game_inputs(&ctx);
    let visual_inputs: Vec<InputEvent> = inputs
        .into_iter()
        .map(|mut input| {
            input.time = visual_now;
            input
        })
        .collect();
    update_recent_inputs(session, &visual_inputs, visual_now);
    update_lane_key_states(session, &visual_inputs);
    update_scratch_angle_phase(session, visual_now);
}

pub fn is_wall_clock_lane_key_time(started_at: TimeUs, chart_time: TimeUs) -> bool {
    started_at.0 > chart_time.0 + CHART_KEY_FUTURE_SLACK_US
}

/// READY 開始時のwall-clockからchart-clockへの切替に合わせて、
/// pre-ready visual timer の経過時間を保ったまま時刻を移す。
///
/// 押下中のkeyonをchart 0で破棄すると、beatorajaと異なりkeybeamが
/// READY/PLAY境界で消える。最後のpre-ready frame時刻と現在のchart時刻の
/// 差を対象timerに加え、同じ経過時間でchart-clockへ引き継ぐ。
pub fn rebase_pre_ready_visual_times(session: &mut GameSession, chart_time: TimeUs) {
    let Some(wall_clock_now) = session.scratch_angle_last_render_at else {
        return;
    };
    // 通常のchart-clockは単調増加する。現在値が前frameより小さい場合だけ、
    // READY開始によるclock domain切替と扱う。短い導入でも検出できるよう
    // key timer判定用の1秒slackはここでは使わない。
    if wall_clock_now.0 <= chart_time.0 {
        return;
    }

    let clock_delta_us = chart_time.0.saturating_sub(wall_clock_now.0);
    for lane_index in 0..LANE_COUNT {
        for time in [
            &mut session.lane_keyon_started_at[lane_index],
            &mut session.lane_keyoff_started_at[lane_index],
            &mut session.lane_auto_release_at[lane_index],
        ]
        .into_iter()
        .flatten()
        {
            time.0 = time.0.saturating_add(clock_delta_us);
        }
    }
    for input in &mut session.recent_inputs {
        input.time.0 = input.time.0.saturating_add(clock_delta_us);
    }
    session.scratch_angle_last_render_at = Some(chart_time);
}

pub fn update_scratch_angle_phase(session: &mut GameSession, render_now: TimeUs) {
    let Some(last_render_at) = session.scratch_angle_last_render_at else {
        session.scratch_angle_last_render_at = Some(render_now);
        return;
    };
    session.scratch_angle_last_render_at = Some(render_now);
    let delta_ms = ((render_now.0 - last_render_at.0) / 1_000).max(0);
    if delta_ms == 0 {
        return;
    }

    for lane_index in 0..LANE_COUNT {
        if session.lane_keyon_started_at[lane_index].is_none() {
            continue;
        }
        let sign =
            match session.lane_scratch_direction[lane_index].unwrap_or(ScratchDirection::Down) {
                ScratchDirection::Up => 1,
                ScratchDirection::Down => -1,
            };
        session.lane_scratch_angle_delta_ms[lane_index] =
            (session.lane_scratch_angle_delta_ms[lane_index] + sign * delta_ms.saturating_mul(2))
                .rem_euclid(SCRATCH_ANGLE_PERIOD_MS);
    }
}

/// 入力バックエンドを drain するだけで、判定にも視覚エフェクト(recent_inputs)にも反映しない。
/// オートプレイ中はキービームをノーツ処理側で発火させるため、人間入力はここで捨てる。
pub fn discard_human_inputs(session: &mut GameSession) {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let _ = session.input_system.collect_game_inputs(&ctx);
}

pub fn process_replay_inputs(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let Some(player) = &mut session.replay_player else {
        return Vec::new();
    };

    let mut inputs = player.poll_until(audio_now);
    if let Some(replay_lane_mask) = &session.replay_lane_mask {
        inputs.retain(|input| replay_lane_mask[input.lane.index()]);
    }
    let inputs =
        inputs.into_iter().map(|input| project_replay_input(session, input)).collect::<Vec<_>>();
    update_lane_key_states(session, &inputs);
    let mut judgements = Vec::new();
    for input in inputs {
        judgements.extend(process_session_input(session, input));
    }
    judgements
}

fn project_replay_input(session: &mut GameSession, mut input: InputEvent) -> InputEvent {
    let Some(projection) = session.replay_lane_projection.as_ref() else {
        return input;
    };
    if input.lane != Lane::Scratch {
        input.lane = projection.playback_to_chart[input.lane.index()];
        return input;
    }

    let fallback = projection.playback_to_chart[Lane::Scratch.index()];
    let scratch_lane_mask = projection.playback_scratch_lane_mask;
    let active_lane = projection.active_playback_scratch_lane;
    let projected_lane = match input.kind {
        InputKind::Press => closest_pending_scratch_lane(
            &session.chart,
            &session.judge,
            input.time,
            &scratch_lane_mask,
        )
        .unwrap_or(fallback),
        InputKind::Release => active_lane.unwrap_or(fallback),
    };
    if let Some(projection) = &mut session.replay_lane_projection {
        projection.active_playback_scratch_lane = match input.kind {
            InputKind::Press => Some(projected_lane),
            InputKind::Release => None,
        };
    }
    input.lane = projected_lane;
    input
}

fn closest_pending_scratch_lane(
    chart: &PlayableChart,
    judge: &JudgeEngine,
    time: TimeUs,
    scratch_lane_mask: &[bool; LANE_COUNT],
) -> Option<Lane> {
    Lane::ALL
        .iter()
        .copied()
        .filter(|lane| scratch_lane_mask[lane.index()])
        .flat_map(|lane| chart.notes_for_lane(lane))
        .filter(|note| matches!(note.kind, NoteKind::Tap | NoteKind::LongStart | NoteKind::Mine))
        .filter(|note| match note.kind {
            NoteKind::Mine => judge.lanes[note.lane.index()].last_mine_hit_time != Some(note.time),
            _ => !judge.judged_notes.contains_key(&note.id),
        })
        .min_by_key(|note| note.time.0.abs_diff(time.0))
        .map(|note| note.lane)
}

pub fn process_autoplay_inputs(
    session: &mut GameSession,
    audio_now: TimeUs,
) -> Vec<JudgementEvent> {
    let Some(auto) = &mut session.autoplay else {
        return Vec::new();
    };

    let inputs = auto.poll_until(&session.chart, audio_now);
    // オートプレイのキービームはノーツ処理時(=この autoplay 入力)で発火させる。
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);

    let mut judgements = Vec::new();
    for input in inputs {
        judgements.extend(process_session_input(session, input));
    }
    judgements
}

pub(super) fn process_session_input(
    session: &mut GameSession,
    input: InputEvent,
) -> Vec<JudgementEvent> {
    push_skin_runtime_event(session, SkinRuntimeEventKind::Input(input));
    let mut outcome = session.judge.process_input(&session.chart, input);
    if input.kind == InputKind::Press {
        let hcn_passing = hcn_passing_at(session, input.lane, input.time);
        if hcn_passing && outcome.events.iter().all(|event| event.judge == Judge::EmptyPoor) {
            outcome.events.clear();
            outcome.keysounds.clear();
            outcome.consumed_input = false;
        }
        if outcome.events.is_empty()
            && outcome.keysounds.is_empty()
            && !outcome.consumed_input
            && !hcn_passing
            && let Some(note_id) = fallback_keysound_note_id(session, input.lane, input.time)
        {
            outcome.keysounds.push(KeySoundEvent {
                note_id,
                time: input.time,
                trigger: KeySoundTrigger::Fallback,
            });
        }
    }
    apply_judge_outcome(session, outcome)
}

fn hcn_passing_at(session: &GameSession, lane: Lane, time: TimeUs) -> bool {
    session.chart.long_notes.iter().any(|pair| {
        pair.lane == lane
            && pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == LongNoteMode::Hcn
            && time.0 >= pair.start_time.0
            && time.0 < pair.end_time.0
    })
}

fn fallback_keysound_note_id(session: &GameSession, lane: Lane, time: TimeUs) -> Option<NoteId> {
    if session.judge.lanes[lane.index()].active_long.is_some() {
        return None;
    }

    let notes = session.chart.notes_for_lane(lane);
    let mut candidate = notes
        .iter()
        .find(|note| is_fallback_playable_note(note) && !is_processed_long_start(session, note));

    for note in notes {
        if note.time.0 >= time.0 {
            break;
        }
        match note.kind {
            NoteKind::Invisible => candidate = Some(note),
            NoteKind::Tap | NoteKind::LongStart if !is_processed_long_start(session, note) => {
                if candidate.is_none_or(|current| current.time.0 <= note.time.0) {
                    candidate = Some(note);
                }
            }
            NoteKind::Tap | NoteKind::LongEnd | NoteKind::Mine | NoteKind::LongStart => {}
        }
    }

    candidate.map(|note| note.id)
}

fn is_fallback_playable_note(note: &NoteEvent) -> bool {
    matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
}

fn is_processed_long_start(session: &GameSession, note: &NoteEvent) -> bool {
    note.kind == NoteKind::LongStart && session.judge.judged_notes.contains_key(&note.id)
}

pub fn process_mine_passes(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let mut lane_keyon_started_at = [None; LANE_COUNT];
    for lane in Lane::ALL {
        let idx = lane.index();
        let Some(keyon_started_at) = session.lane_keyon_started_at[idx] else {
            continue;
        };
        if session.autoplay.as_ref().is_some_and(|autoplay| autoplay.is_lane_enabled(lane)) {
            continue;
        }
        lane_keyon_started_at[idx] = Some(keyon_started_at);
    }

    let outcome =
        session.judge.process_mine_passes(&session.chart, audio_now, &lane_keyon_started_at);
    apply_judge_outcome(session, outcome)
}

pub fn process_misses(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let outcome = session.judge.process_misses(&session.chart, audio_now);
    apply_judge_outcome(session, outcome)
}
use super::*;
