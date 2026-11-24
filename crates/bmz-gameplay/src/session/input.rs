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
use super::judgement::{apply_judge_outcome_with_display_time, push_skin_runtime_event};
use super::state::{INPUT_OFFSET_AUTO_ADJUST_BATCH, INPUT_OFFSET_AUTO_ADJUST_STEP_US};
use crate::replay::TimedInput;

pub fn update_recent_judgements(session: &mut GameSession, events: &[JudgementEvent], now: TimeUs) {
    for event in events {
        if !event.affects_score {
            continue;
        }
        session.hit_error_ring.push_judgement(event.judge, event.delta.0);
        if is_misslayer_judge(event.judge) {
            // An older miss delivered later never replaces the latest one.
            if session.last_miss_judgement_time.is_none_or(|last| last <= event.time) {
                session.last_miss_judgement_time = Some(event.time);
                session.miss_hidden_by_good_time = None;
            }
        } else if matches!(event.judge, Judge::PGreat | Judge::Great | Judge::Good)
            && session.miss_hidden_by_good_time.is_none()
            && session.last_miss_judgement_time.is_some_and(|last| last <= event.time)
        {
            session.miss_hidden_by_good_time = Some(event.time);
        }
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
        .retain(|event| now.0 <= event.display_time.0.saturating_add(JUDGEMENT_DISPLAY_US));
}

/// ミスレイヤー (ミス BGA) と BMS `#TEXT00` のミス表示を出す判定。BAD と POOR
/// (見逃し・早離し) が対象で、空 POOR は含めない。nanasi の `#TEXT00` の対象判定は
/// 未確認のため、BMZ ではミスレイヤーに合わせる。
pub fn is_misslayer_judge(judge: Judge) -> bool {
    matches!(judge, Judge::Bad | Judge::Poor)
}

pub fn update_recent_inputs(session: &mut GameSession, inputs: &[InputEvent], now: TimeUs) {
    session
        .recent_inputs
        .extend(inputs.iter().copied().filter(|input| input.kind == InputKind::Press));
    session.recent_inputs.retain(|input| now.0 <= input.time.0 + INPUT_DISPLAY_US);
}

/// A directionless key and the two scratch directions can be held independently.
#[derive(Debug, Clone, Copy, Default)]
pub struct LaneKeyHold {
    held: [bool; 3],
    pub direction: Option<ScratchDirection>,
}

impl LaneKeyHold {
    pub fn update(&mut self, kind: InputKind, direction: Option<ScratchDirection>) {
        use ScratchDirection::{Down, Up};
        let index = match direction {
            None => 0,
            Some(Up) => 1,
            Some(Down) => 2,
        };
        self.held[index] = kind == InputKind::Press;
        self.direction = if kind == InputKind::Press {
            direction
        } else if self.held[1] {
            Some(Up)
        } else if self.held[2] {
            Some(Down)
        } else {
            None
        };
    }

    pub fn is_held(self) -> bool {
        self.held.contains(&true)
    }
}

/// レーンごとのキー押下/解放状態を更新する。beatoraja の KEYON/KEYOFF タイマー相当。
/// Press → keyon を新しい時刻にセット、keyoff をクリア(autoplay 入力なら 80ms 後の自動 release も予約)。
/// Release → 押下中だった場合のみ keyoff をセット、keyon と auto_release_at をクリア。
pub fn update_lane_key_states(session: &mut GameSession, inputs: &[InputEvent]) {
    for input in inputs {
        let lane_index = input.lane.index();
        let held = &mut session.lane_key_hold[lane_index];
        held.update(input.kind, input.scratch_direction);
        match input.kind {
            InputKind::Press => {
                session.lane_keyon_started_at[lane_index] = Some(input.time);
                session.lane_keyoff_started_at[lane_index] = None;
                session.lane_auto_release_at[lane_index] = match input.source {
                    InputSource::Auto => Some(TimeUs(input.time.0 + AUTO_KEYBEAM_DURATION_US)),
                    _ => None,
                };
            }
            InputKind::Release => {
                if held.is_held() {
                    continue;
                }
                if session.lane_keyon_started_at[lane_index].take().is_some() {
                    session.lane_keyoff_started_at[lane_index] = Some(input.time);
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
                    scratch_direction: session.lane_key_hold[lane_index].direction,
                }),
            );
            session.lane_keyoff_started_at[lane_index] = Some(release_at);
            session.lane_keyon_started_at[lane_index] = None;
            session.lane_key_hold[lane_index] = Default::default();
            session.lane_auto_release_at[lane_index] = None;
        }
    }
}

/// Human inputs waiting for their offset-adjusted time, with the instant each batch was collected.
#[derive(Debug, Default)]
pub struct PendingInputs(Vec<(InputEvent, std::time::Instant)>);

impl PendingInputs {
    pub fn push(&mut self, input: InputEvent) {
        self.0.push((input, std::time::Instant::now()));
    }

    pub fn iter(&self) -> impl Iterator<Item = &InputEvent> {
        self.0.iter().map(|(input, _)| input)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    fn take_due(&mut self, now: TimeUs, processed: Option<TimeUs>) -> Vec<TimedInput> {
        let mut due = Vec::new();
        self.0.retain(|&(input, collected_at)| {
            if input.time > now {
                return true;
            }
            due.push(TimedInput {
                collected_at: Some(collected_at),
                ..TimedInput::new(input, processed)
            });
            false
        });
        due.sort_by_key(|edge| edge.input.time);
        due
    }
}

impl Extend<InputEvent> for PendingInputs {
    fn extend<T: IntoIterator<Item = InputEvent>>(&mut self, inputs: T) {
        let at = std::time::Instant::now();
        self.0.extend(inputs.into_iter().map(|input| (input, at)));
    }
}

/// Collect once per owner update, before choosing its scoring horizon or evaluating branches.
pub(super) fn collect_player_inputs(session: &mut GameSession) {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let (batch, collected_at) = session.input_system.collect_game_inputs_with_time(&ctx);
    let inputs = &mut session.conditional.pending_inputs.0;
    inputs.extend(batch.into_iter().map(|input| (input, collected_at)));
    if let Some(autoplay) = &session.autoplay {
        inputs.retain(|(input, _)| !autoplay.is_lane_enabled(input.lane));
    }
    if let Some(replay_lane_mask) = &session.replay_lane_mask {
        inputs.retain(|(input, _)| !replay_lane_mask[input.lane.index()]);
    }
}

fn take_due_player_inputs(session: &mut GameSession, now: TimeUs) -> Vec<InputEvent> {
    collect_player_inputs(session);
    let inputs: Vec<_> = session
        .conditional
        .pending_inputs
        .take_due(now, None)
        .into_iter()
        .map(|edge| edge.input)
        .collect();
    update_recent_inputs(session, &inputs, now);
    update_lane_key_states(session, &inputs);
    inputs
}

pub fn process_human_inputs(session: &mut GameSession) -> Vec<JudgementEvent> {
    let now = session.audio_clock.now();
    collect_player_inputs(session);
    let inputs = take_human_scoring_inputs(session, now);
    apply_timed_inputs(session, inputs, now)
}

pub(super) fn uses_ordered_inputs(session: &GameSession) -> bool {
    session.rule_mode == RuleMode::Legacy
        || super::conditional::uses_ordered_timeline(&session.chart)
        || !session.timing_map.bpm_reflections().is_empty()
}

pub(super) fn uses_legacy_frame_clock(session: &GameSession) -> bool {
    session.rule_mode == RuleMode::Legacy
        && !super::conditional::uses_ordered_timeline(&session.chart)
        && session.timing_map.bpm_reflections().is_empty()
        && session.chart.long_notes.iter().all(|pair| {
            pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == LongNoteMode::Ln
        })
}

/// 採点を閉じた後の入力を、判定・リプレイ記録・ゲージへ渡さずに表示とキー音へ反映する。
///
/// 自然確定後の入力は判定窓の外にあるため、通常の処理でも対象ノーツを持たない。
/// その場合と同じく、キービーム・skin入力イベント・押鍵時のfallbackキー音だけを扱う。
pub fn present_inputs_after_scoring_closed(session: &mut GameSession, audio_now: TimeUs) {
    let full_replay = session.replay_player.is_some() && session.replay_lane_mask.is_none();
    if full_replay {
        drain_human_inputs(session);
    } else if session.autoplay.as_ref().is_some_and(AutoplayController::is_full) {
        discard_human_inputs(session);
    } else {
        let inputs = take_due_player_inputs(session, audio_now);
        for input in inputs {
            present_unscored_input(session, input);
        }
    }

    if let Some(mut inputs) =
        session.replay_player.as_mut().map(|player| player.poll_until(audio_now))
    {
        if let Some(replay_lane_mask) = &session.replay_lane_mask {
            inputs.retain(|input| replay_lane_mask[input.lane.index()]);
        }
        let inputs = inputs
            .into_iter()
            .map(|input| project_replay_input(session, input))
            .collect::<Vec<_>>();
        update_lane_key_states(session, &inputs);
        for input in inputs {
            present_unscored_input(session, input);
        }
    }

    let chart = &session.chart;
    if let Some(inputs) = session.autoplay.as_mut().map(|auto| auto.poll_until(chart, audio_now)) {
        update_recent_inputs(session, &inputs, session.audio_clock.now());
        update_lane_key_states(session, &inputs);
        for input in inputs {
            present_unscored_input(session, input);
        }
        apply_auto_key_release(session, audio_now);
    }
}

fn present_unscored_input(session: &mut GameSession, input: InputEvent) {
    push_skin_runtime_event(session, SkinRuntimeEventKind::Input(input));
    if input.kind != InputKind::Press
        || session.judge.consume_scratch_press_suppression(input)
        || hold_note_passing_at(session, input.lane, input.time, LongNoteMode::Hcn)
        || hold_note_passing_at(session, input.lane, input.time, LongNoteMode::Hln)
    {
        return;
    }
    let Some(note_id) = fallback_keysound_note_id(session, input.lane, input.time) else {
        return;
    };
    let display_only = session
        .judge
        .note_by_id(&session.chart, note_id)
        .is_some_and(|note| session.display_only_lane_mask[note.lane.index()]);
    if !display_only {
        session.pending_keysounds.push(KeySoundEvent {
            note_id,
            time: input.time,
            trigger: KeySoundTrigger::Fallback,
        });
    }
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
            match session.lane_key_hold[lane_index].direction.unwrap_or(ScratchDirection::Down) {
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
    if session.play_end_reached() {
        return Vec::new();
    }
    let mut inputs = Vec::new();
    append_replay_inputs(session, audio_now, usize::MAX, &mut inputs);
    apply_timed_inputs(session, inputs, audio_now)
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
            session.timing_map.reflected_judge_time(input.time),
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
        .flat_map(|lane| chart.notes_for_lane(lane).iter().enumerate())
        .filter(|(index, note)| match note.kind {
            NoteKind::Mine => *index >= judge.lanes[note.lane.index()].next_mine_index,
            NoteKind::Tap | NoteKind::LongStart => !judge.judged_notes.contains_key(&note.id),
            _ => false,
        })
        .min_by_key(|(_, note)| note.time.0.abs_diff(time.0))
        .map(|(_, note)| note.lane)
}

pub fn process_autoplay_inputs(
    session: &mut GameSession,
    audio_now: TimeUs,
) -> Vec<JudgementEvent> {
    if session.play_end_reached() {
        return Vec::new();
    }
    let Some(auto) = &mut session.autoplay else {
        return Vec::new();
    };

    let inputs = auto.poll_until(&session.chart, audio_now);
    // オートプレイのキービームはノーツ処理時(=この autoplay 入力)で発火させる。
    update_recent_inputs(session, &inputs, session.audio_clock.now());
    update_lane_key_states(session, &inputs);

    let mut judgements = Vec::new();
    for input in inputs {
        if session.play_end_reached() {
            break;
        }
        judgements.extend(process_session_input(session, input));
    }
    judgements
}

fn take_human_scoring_inputs(session: &mut GameSession, now: TimeUs) -> Vec<TimedInput> {
    let mut human =
        session.conditional.pending_inputs.take_due(now, session.conditional.processed_time);
    if session.replay_player.is_some() && session.replay_lane_mask.is_none() {
        for edge in &human {
            update_recent_inputs(session, &[edge.input], now);
        }
        human.clear();
    }
    human
}

fn append_replay_inputs(
    session: &mut GameSession,
    now: TimeUs,
    limit: usize,
    inputs: &mut Vec<TimedInput>,
) {
    if let Some(player) = &mut session.replay_player {
        inputs.extend(player.poll_timed_until(now, limit).filter(|timed| {
            session.replay_lane_mask.as_ref().is_none_or(|mask| mask[timed.input.lane.index()])
        }));
    }
}

/// 通過判定・時間積分のため、人間・Replay・Autoplay の入力を一本の時系列へまとめる。
pub(super) fn process_timed_inputs(
    session: &mut GameSession,
    now: TimeUs,
    input_limit: usize,
) -> Vec<JudgementEvent> {
    let mut inputs = take_human_scoring_inputs(session, now);
    append_replay_inputs(session, now, input_limit, &mut inputs);
    if let Some(auto) = &mut session.autoplay {
        inputs.extend(
            auto.poll_until(&session.chart, now)
                .into_iter()
                .map(|input| TimedInput::new(input, None)),
        );
    }
    let events = if uses_legacy_frame_clock(session) {
        process_legacy_timed_inputs(session, inputs, now)
    } else {
        apply_timed_inputs(session, inputs, now)
    };
    if session.autoplay.is_some() {
        apply_auto_key_release(session, now);
    }
    events
}

/// LEGACY on OpenLR2's polling clock: every lane is polled each integer
/// millisecond, scratch first, and each lane's judgements and mines reach the
/// gauge in that order. Mines share the lane cursor with notes.
///
/// An edge is polled at its own millisecond. One that arrives after that
/// millisecond was already polled is polled again at the processed
/// millisecond, as OpenLR2 does on its next loop; `applied_after` records this
/// so a replay polls it the same way.
fn process_legacy_timed_inputs(
    session: &mut GameSession,
    mut inputs: Vec<TimedInput>,
    now: TimeUs,
) -> Vec<JudgementEvent> {
    let previous = session.conditional.processed_time.filter(|at| at.0 >= 0);
    let ms_of = |time: TimeUs| time.0 / 1000 * 1000;
    for timed in &mut inputs {
        // A same-millisecond edge after the poll keeps its own time as the
        // boundary: `TimedInput` drops a boundary earlier than the input.
        if timed.applied_after.is_none()
            && previous.is_some_and(|at| ms_of(timed.input.time) <= at.0)
        {
            timed.applied_after = Some(timed.input.time);
        }
    }
    inputs.sort_by_key(TimedInput::order);
    let poll_of = |timed: &TimedInput| match timed.applied_after {
        Some(after) => (ms_of(after), true),
        None => (ms_of(timed.input.time), false),
    };
    let repoll_previous =
        previous.is_some_and(|at| inputs.iter().any(|timed| poll_of(timed) == (at.0, true)));
    let mut tick = match previous {
        Some(at) if repoll_previous => at.0,
        Some(at) => at.0 + 1000,
        None => 0,
    };
    let mut pending: std::collections::VecDeque<_> = inputs.into();
    let mut events = Vec::new();
    session.judge.set_legacy_inline_mines(true);
    while tick <= now.0 && session.state == PlayState::Playing && !session.play_end_reached() {
        let time = TimeUs(tick);
        super::frame::sync_judge_windows(session, time);
        let first_poll = previous.is_none_or(|at| tick > at.0);
        for repoll in [false, true] {
            if !repoll && !first_poll {
                continue;
            }
            let mut group = Vec::new();
            pending.retain(|timed| {
                let (at, again) = poll_of(timed);
                let take = at <= tick && again == repoll;
                if take {
                    group.push(*timed);
                }
                !take
            });
            if repoll && group.is_empty() {
                continue;
            }
            legacy_poll(session, &group, time, now, &mut events);
        }
        session.conditional.processed_time = Some(time);
        tick += 1000;
    }
    events
}

/// One OpenLR2 loop over all lanes at `time` with the edges of this poll.
fn legacy_poll(
    session: &mut GameSession,
    group: &[TimedInput],
    time: TimeUs,
    now: TimeUs,
    events: &mut Vec<JudgementEvent>,
) {
    let mut human_events = Vec::new();
    for lane in crate::judge::engine::LEGACY_LANE_ORDER {
        let mut has_edge = false;
        for timed in group.iter().filter(|timed| timed.input.lane == lane) {
            record_human_edge(session, *timed);
            let mut input = timed.input;
            if input.source == InputSource::Replay {
                input = project_replay_input(session, input);
            }
            update_recent_inputs(session, &[input], now);
            update_lane_key_states(session, &[input]);
            // OpenLR2 merges both scratch directions into one key state:
            // releasing one direction while the other is held stays a hold.
            if input.kind == InputKind::Release && session.lane_key_hold[lane.index()].is_held() {
                push_skin_runtime_event(session, SkinRuntimeEventKind::Input(input));
                continue;
            }
            has_edge = true;
            let outcome = evaluate_session_input(session, input);
            if input.source == InputSource::Human {
                human_events.extend(outcome.events.iter().cloned());
                if let Some(collected_at) = timed.collected_at {
                    super::latency::judged(collected_at);
                }
            }
            // LR2 applies each judgement and mine to the gauge as its lane is
            // polled; the order matters at the gauge limits.
            events.extend(apply_judge_outcome(session, outcome));
        }
        if !has_edge {
            let outcome = session.judge.process_legacy_lane_poll(
                &session.chart,
                lane,
                time,
                session.lane_key_hold[lane.index()].is_held(),
                session.play_keysound_on_miss,
            );
            events.extend(apply_judge_outcome(session, outcome));
        }
    }
    // LR2 checks gauge failure only after polling all lanes, so a gauge that
    // fails mid-poll still lets the remaining lanes (and any BAD recursion) be
    // judged and scored; the loop stops before the next poll.
    apply_input_offset_auto_adjust(session, &human_events);
}

/// Writes a human edge to the replay with the boundary it was applied after.
fn record_human_edge(session: &mut GameSession, timed: TimedInput) {
    let input = timed.input;
    if input.source != InputSource::Human || session.display_only_lane_mask[input.lane.index()] {
        return;
    }
    let lane = session
        .replay_lane_projection
        .as_ref()
        .map_or(Some(input.lane), |p| p.record_to_source[input.lane.index()]);
    if let Some(lane) = lane {
        session.replay_recorder.record_after(InputEvent { lane, ..input }, timed.applied_after);
    }
}

/// Player-specific effects of an input. The order in which passage, key state,
/// mines, judgements and hold timers see it is shared by live play and both replays.
pub(super) trait InputScoring {
    type Event;
    fn stopped(&self) -> bool;
    fn has_mines(&self) -> bool;
    fn advance_to_input(
        &mut self,
        first: TimedInput,
        has_mines: bool,
        events: &mut Vec<Self::Event>,
    ) -> TimeUs;
    fn judge_edge(&mut self, edge: TimedInput, now: TimeUs) -> (InputEvent, JudgeOutcome);
    fn pass_mines(&mut self, time: TimeUs) -> Vec<Self::Event>;
    fn apply_judgement(&mut self, input: InputEvent, outcome: JudgeOutcome) -> Vec<Self::Event>;
    fn update_hold(&mut self, time: TimeUs);
}

pub(super) fn apply_timed_inputs<S: InputScoring>(
    player: &mut S,
    mut inputs: Vec<TimedInput>,
    now: TimeUs,
) -> Vec<S::Event> {
    let mut events = Vec::new();
    if inputs.is_empty() {
        return events;
    }
    inputs.sort_by_key(TimedInput::order);
    let has_mines = player.has_mines();
    for group in inputs.chunk_by(TimedInput::same_input_time) {
        if player.stopped() {
            break;
        }
        let time = player.advance_to_input(group[0], has_mines, &mut events);
        if player.stopped() {
            break;
        }
        // A mine at this timestamp sees the final held state before the input
        // judgements affect the gauge. Earlier mines saw the previous state.
        let outcomes: Vec<_> = group.iter().map(|&edge| player.judge_edge(edge, now)).collect();
        if has_mines {
            events.extend(player.pass_mines(time));
        }
        for (edge, (input, outcome)) in group.iter().zip(outcomes) {
            if player.stopped() {
                break;
            }
            events.extend(player.apply_judgement(input, outcome));
            if input.source == InputSource::Human
                && let Some(collected_at) = edge.collected_at
            {
                super::latency::judged(collected_at);
            }
        }
        player.update_hold(time);
    }
    events
}

impl InputScoring for GameSession {
    type Event = JudgementEvent;
    fn stopped(&self) -> bool {
        self.state == PlayState::Failed || self.play_end_reached()
    }
    fn has_mines(&self) -> bool {
        self.judge.has_mines(&self.chart)
    }

    fn advance_to_input(
        &mut self,
        first: TimedInput,
        has_mines: bool,
        events: &mut Vec<JudgementEvent>,
    ) -> TimeUs {
        let before = first.before_input();
        let time = first.delivery_time();
        let hold = self.conditional.processed_time.map_or(time, |last| time.max(last));
        if uses_ordered_inputs(self) || self.judge.has_hcn(&self.chart) {
            events.extend(super::conditional::advance_time(self, before));
            if self.state == PlayState::Playing {
                events.extend(apply_hcn_gauge(self, hold));
            }
        } else {
            super::frame::sync_judge_windows(self, before);
            if has_mines {
                events.extend(process_mine_passes(self, before));
            }
            if self.state == PlayState::Playing {
                events.extend(process_misses(self, before));
            }
        }
        // Use the edge's own rank even if a late delivery crosses a rank change.
        super::frame::sync_judge_windows(self, first.input.time);
        hold
    }

    fn judge_edge(&mut self, timed: TimedInput, now: TimeUs) -> (InputEvent, JudgeOutcome) {
        record_human_edge(self, timed);
        let mut input = timed.input;
        if input.source == InputSource::Replay {
            input = project_replay_input(self, input);
        }
        update_recent_inputs(self, &[input], now);
        update_lane_key_states(self, &[input]);
        (input, evaluate_session_input(self, input))
    }

    fn pass_mines(&mut self, time: TimeUs) -> Vec<JudgementEvent> {
        process_mine_passes(self, time)
    }

    fn apply_judgement(&mut self, input: InputEvent, outcome: JudgeOutcome) -> Vec<JudgementEvent> {
        let events = apply_input_outcome(self, input, outcome);
        if input.source == InputSource::Human {
            apply_input_offset_auto_adjust(self, &events);
        }
        events
    }

    fn update_hold(&mut self, time: TimeUs) {
        if uses_ordered_inputs(self) || self.judge.has_hcn(&self.chart) {
            update_hcn_lane_timers(self, time);
            if self.lane_hcn_timer.iter().any(Option::is_some) {
                self.last_hcn_gauge_at = Some(time);
            }
            self.conditional.processed_time = Some(time);
        }
    }
}
pub(super) fn process_session_input(
    session: &mut GameSession,
    input: InputEvent,
) -> Vec<JudgementEvent> {
    let outcome = evaluate_session_input(session, input);
    apply_input_outcome(session, input, outcome)
}

fn evaluate_session_input(session: &mut GameSession, input: InputEvent) -> JudgeOutcome {
    push_skin_runtime_event(session, SkinRuntimeEventKind::Input(input));
    let judge_time = if input.source == InputSource::Auto {
        input.time
    } else {
        session.timing_map.reflected_judge_time(input.time)
    };
    let mut outcome = session.judge.process_input_at(
        &session.chart,
        input,
        judge_time,
        session.play_keysound_on_miss,
    );
    if input.kind == InputKind::Press {
        let hcn_passing = hold_note_passing_at(session, input.lane, input.time, LongNoteMode::Hcn);
        let hln_passing = hold_note_passing_at(session, input.lane, input.time, LongNoteMode::Hln);
        if outcome.events.is_empty()
            && outcome.keysounds.is_empty()
            && !outcome.consumed_input
            && !hcn_passing
            && !hln_passing
            && !session.chart.metadata.free_zones.iter().any(|zone| {
                zone.lane == input.lane
                    && input.time >= zone.start_time
                    && input.time <= zone.end_time
            })
            && let Some(note_id) = fallback_keysound_note_id(session, input.lane, judge_time)
        {
            outcome.keysounds.push(KeySoundEvent {
                note_id,
                time: input.time,
                trigger: KeySoundTrigger::Fallback,
            });
        }
    }
    outcome
}

fn apply_input_outcome(
    session: &mut GameSession,
    input: InputEvent,
    outcome: JudgeOutcome,
) -> Vec<JudgementEvent> {
    // 判定・記録・発音には補正済み input.time を維持し、表示は打鍵時刻と判定時刻の遅い方から
    // 始める。正のオフセットで判定時刻まで保留した入力も、アニメーションを途中から始めない。
    // 処理時の now ではなく入力の時刻を使い、入力配送や描画の遅延を引き継ぐ。
    // Replay / Auto は譜面時刻が確定済みなので現在の profile offset を引かない。
    let display_time = (input.source == InputSource::Human).then(|| {
        TimeUs(input.time.0.max(input.time.0.saturating_sub(session.offsets.input_offset_us)))
    });
    apply_judge_outcome_with_display_time(session, outcome, display_time)
}

fn hold_note_passing_at(
    session: &GameSession,
    lane: Lane,
    time: TimeUs,
    mode: LongNoteMode,
) -> bool {
    session.chart.long_notes.iter().any(|pair| {
        pair.lane == lane
            && pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == mode
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
            NoteKind::Tap
            | NoteKind::LongEnd
            | NoteKind::Mine
            | NoteKind::LongStart
            | NoteKind::Potion
            | NoteKind::DummyTap
            | NoteKind::DummyLongStart
            | NoteKind::DummyLongEnd
            | NoteKind::DummyMine
            | NoteKind::DummyPotion
            | NoteKind::FreeZoneEnd => {}
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

    let reflected_time = session
        .timing_map
        .bpm_reflection_at(audio_now)
        .map(|point| bmz_chart::timing::reflect_time(audio_now, point.time));
    let outcome = session.judge.process_mine_passes_at(
        &session.chart,
        audio_now,
        reflected_time,
        &lane_keyon_started_at,
    );
    apply_judge_outcome(session, outcome)
}

pub fn process_misses(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    session.judge.free_zone_autoplay = std::array::from_fn(|i| {
        session.autoplay.as_ref().is_some_and(|auto| auto.is_lane_enabled(Lane::ALL[i]))
    });
    let judge_time = session.timing_map.reflected_judge_time(audio_now);
    let auto_lanes = session.autoplay.as_ref();
    let outcome = session.judge.process_misses_at(
        &session.chart,
        audio_now,
        |lane| {
            if auto_lanes.is_some_and(|auto| auto.is_lane_enabled(lane)) {
                audio_now
            } else {
                judge_time
            }
        },
        session.play_keysound_on_miss,
    );
    apply_judge_outcome(session, outcome)
}
use super::*;
