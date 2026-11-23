pub fn sync_judge_windows(session: &mut GameSession, now: TimeUs) {
    let rate = if session.replay_player.is_some() {
        session.replay_judge_rate_percent
    } else {
        session.audio_clock.playback_rate_percent()
    };
    set_judge_windows(
        &mut session.judge,
        &session.chart,
        session.base_judge_windows,
        session.primary_key_mode,
        session.rule_mode,
        now,
        rate,
    );
}

fn set_judge_windows(
    judge: &mut JudgeEngine,
    chart: &PlayableChart,
    base: crate::judge::model::JudgeWindows,
    key_mode: KeyMode,
    rule: RuleMode,
    now: TimeUs,
    rate: u16,
) {
    let percent = judge_percent_at_time_for_keymode(
        chart.metadata.judge_rank_spec,
        &chart.judge_rank_events,
        now,
        key_mode,
        rule,
    );
    let windows = judge.window_rates.for_rank(base, percent, rule, key_mode);
    judge.set_window_set(scale_judge_windows_for_playback_rate(windows, rate));
}
use super::judgement::{
    update_failed_state_from_gauge, update_gauge_increase_timer_state, update_gauge_max_timer,
    update_gauge_max_timer_state,
};

pub(super) fn sync_input_timestamp_anchor(session: &mut GameSession) {
    session.input_timestamp_anchor = if session.audio_clock.running {
        let (at, monotonic_ns) = crate::input::backend::monotonic_observation();
        Some(InputTimestampAnchor { monotonic_ns, audio_time: session.audio_clock.now_at(at) })
    } else {
        None
    };
}

pub fn advance_session_frame(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
) -> SessionFrame {
    super::latency::begin_advance();
    let times = compute_frame_times(session);
    sync_input_timestamp_anchor(session);
    rebase_pre_ready_visual_times(session, times.audio_now);
    update_scratch_angle_phase(session, times.audio_now);
    let mut judgements = Vec::new();

    if session.state == PlayState::Ready && times.audio_now.0 < 0 {
        drain_pre_ready_visual_inputs(session, times.audio_now);
    } else if session.state == PlayState::Ready {
        session.state = PlayState::Playing;
    }

    if matches!(session.state, PlayState::Ready | PlayState::Playing)
        && (session.state == PlayState::Ready || session.chart.metadata.conditional.is_none())
    {
        schedule_chart_audio(session, times.audio_schedule_until, audio);
    }

    session.apply_play_end_request(false);
    if session.state == PlayState::Playing {
        let full_replay = session.replay_player.is_some() && session.replay_lane_mask.is_none();
        let cutoff =
            session.recorded_replay_end().map(|end| end.scored_through.unwrap_or(end.ended_at));
        if session.scoring_closed_at.is_none() {
            if full_replay {
                drain_human_inputs(session);
            } else {
                super::input::collect_player_inputs(session);
            }
        }
        let score_now = primary_score_time(session, times.audio_now);
        let score_now = cutoff.map_or(score_now, |end| end.min(score_now));

        if session.scoring_closed_at.is_some() {
            // 採点確定後も、キー表示・キー音・skin入力イベントは従来どおり進める。
            present_inputs_after_scoring_closed(session, times.audio_now);
        } else {
            advance_scoring(
                session,
                times.audio_now,
                score_now,
                cutoff,
                ScoringTarget::Primary { audio: Some(audio), judgements: &mut judgements },
            );
        }
        update_failed_state_from_gauge(session);
        session.apply_play_end_request(false);
        if !session.play_end_reached() && session.chart.metadata.conditional.is_some() {
            schedule_chart_audio(session, times.audio_schedule_until, audio);
        }
        if !session.play_end_reached() {
            schedule_keysounds(session, audio);
        }
        update_recent_inputs(session, &[], times.audio_now);
        update_recent_judgements(session, &judgements, times.audio_now);
        update_full_combo_timer(session, &judgements);
        if !session.play_end_reached() {
            advance_battle_opponent(session, times.audio_now, cutoff);
            session.apply_play_end_request(true);
        }

        if let Some(end) =
            session.recorded_replay_end().filter(|evidence| times.audio_now >= evidence.ended_at)
        {
            if end.manually_stopped {
                session.manually_stopped = true;
                session.state = PlayState::Failed;
            } else {
                session.request_finish();
            }
        }

        // この更新で取得した入力・見逃し・地雷・LN処理を終えた後に採点を閉じる。
        if session.scoring_closed_at.is_none()
            && session.state == PlayState::Playing
            && natural_result_settled(session, times.audio_now)
        {
            session.scoring_closed_at = Some(times.audio_now);
            session.natural_result_settled_at = Some(times.audio_now);
        }
        let finish_clock =
            if session.scoring_closed_at.is_some() { times.audio_now } else { score_now };
        if session.state == PlayState::Playing && playback_deadline_passed(session, finish_clock) {
            if session.judge.is_exhausted(&session.chart) {
                session.state = PlayState::Finished;
            } else if session.timing_map.bpm_reflection_at(times.audio_now).is_some() {
                // The reflected clock can leave notes beyond reach for the rest of
                // the song. LR2 ends playback with a failed result in this case.
                session.gauge.set_initial_value(0.0);
                session.state = PlayState::Failed;
            }
        }
    }
    if session.state == PlayState::Ready
        && session
            .recorded_replay_end()
            .is_some_and(|end| end.manually_stopped && times.audio_now >= end.ended_at)
    {
        session.manually_stopped = true;
        session.state = PlayState::Failed;
    }
    if session.scoring_closed_at.is_none()
        && matches!(session.state, PlayState::Finished | PlayState::Failed)
    {
        // 途中落ち・手動中断・明示的な退出は、その停止境界で確定する。
        session.scoring_closed_at = Some(times.audio_now);
    }
    update_gauge_max_timer(session, times.audio_now);
    if matches!(session.state, PlayState::Failed | PlayState::Finished) {
        let end = session.scoring_closed_at.unwrap_or(times.audio_now);
        super::conditional::finalize_defaults(&session.chart, &mut session.conditional, end);
        if let Some(rival) = &mut session.opponent.independent {
            super::conditional::finalize_defaults(&rival.chart, &mut rival.conditional, end);
        }
    }

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

enum ScoringTarget<'a> {
    Primary { audio: Option<&'a mut dyn AudioScheduler>, judgements: &'a mut Vec<JudgementEvent> },
    Opponent,
}

/// Only the caller supplies the audio and scoring horizons. Live play, replay,
/// battle and seek all complete the same prefix before selecting a new chart.
fn advance_scoring(
    session: &mut GameSession,
    audio_now: TimeUs,
    score_now: TimeUs,
    cutoff: Option<TimeUs>,
    mut target: ScoringTarget<'_>,
) {
    loop {
        let (chart, runtime, player) = match target {
            ScoringTarget::Primary { .. } => (
                &session.chart,
                &session.conditional,
                session.replay_player.as_ref().filter(|_| session.replay_lane_mask.is_none()),
            ),
            ScoringTarget::Opponent => {
                let Some(rival) = &session.opponent.independent else { return };
                (&rival.chart, &rival.conditional, rival.replay_player.as_ref())
            }
        };
        let step = super::conditional::next_scoring_step(
            chart, runtime, player, audio_now, score_now, cutoff,
        );
        match &mut target {
            ScoringTarget::Primary { audio, judgements } => {
                let full_replay = player.is_some();
                if let Some(replay) = player.filter(|_| !super::input::uses_ordered_inputs(session))
                {
                    let ticks = replay.scoring_times_through(step.until, cutoff);
                    for time in ticks {
                        if session.play_end_reached() || session.apply_play_end_request(false) {
                            return;
                        }
                        score_primary_at(session, time, true, step.input_limit, judgements);
                        session
                            .replay_player
                            .as_mut()
                            .unwrap()
                            .complete_scoring_tick(session.scoring_closed_at.unwrap_or(time));
                        if session.state != PlayState::Playing {
                            return;
                        }
                    }
                } else {
                    score_primary_at(
                        session,
                        step.until,
                        full_replay,
                        step.input_limit,
                        judgements,
                    );
                }
                let Some(at) = step.branch else { return };
                if session.state != PlayState::Playing {
                    return;
                }
                if let Some(audio) = audio.as_deref_mut() {
                    schedule_chart_audio(session, TimeUs(at.0.saturating_sub(1)), audio);
                }
                if session.apply_play_end_request(false) {
                    return;
                }
                super::conditional::evaluate(session, at);
            }
            ScoringTarget::Opponent => {
                score_battle_opponent_through(session, &step, cutoff);
                let Some(at) = step.branch else { return };
                if session.play_end_reached() || session.apply_play_end_request(false) {
                    return;
                }
                let rival = session.opponent.independent.as_mut().unwrap();
                if opponent_hold_failed(rival) {
                    return;
                }
                super::conditional::evaluate_opponent(rival, at);
            }
        }
    }
}

fn score_primary_at(
    session: &mut GameSession,
    now: TimeUs,
    full_replay: bool,
    input_limit: usize,
    judgements: &mut Vec<JudgementEvent>,
) {
    judgements.extend(super::input::process_timed_inputs(session, now, input_limit));
    if super::input::uses_ordered_inputs(session) || session.judge.has_hcn(&session.chart) {
        judgements.extend(super::conditional::advance_time(session, now));
        return;
    }
    sync_judge_windows(session, now);
    if session.state == PlayState::Playing || full_replay {
        judgements.extend(process_mine_passes(session, now));
    }
    if session.state == PlayState::Playing
        || full_replay
        || !session.chart.metadata.kaleid.effective.hln
    {
        judgements.extend(process_misses(session, now));
    }
    update_hcn_lane_timers(session, now);
    update_failed_state_from_gauge(session);
    session.conditional.processed_time = Some(now);
}

/// Viewer の途中再生開始前に、過去の入力・判定 cursor を無音で進める。
/// 開始時刻より前に判定時刻を迎えたscore対象ノートはPGREATとして事前集計し、
/// 境界時刻のノートはその位置からのAutoplay対象として残す。
pub fn prepare_viewer_seek(session: &mut GameSession, start_time: TimeUs) {
    // Inputs held for a judgement time at the previous position never score after the seek.
    session.conditional.pending_inputs.clear();
    if session.chart.metadata.conditional.is_some() {
        let previous_state = session.state;
        session.state = PlayState::Playing;
        let before = TimeUs(start_time.0.saturating_sub(1));
        let cutoff =
            session.recorded_replay_end().map(|end| end.scored_through.unwrap_or(end.ended_at));
        let audio_now =
            session.recorded_replay_end().map_or(before, |end| before.min(end.ended_at));
        advance_scoring(
            session,
            audio_now,
            cutoff.map_or(before, |end| before.min(end)),
            cutoff,
            ScoringTarget::Primary { audio: None, judgements: &mut Vec::new() },
        );
        advance_battle_opponent(session, audio_now, cutoff);
        // Fast-forward judgement state without emitting historical audio or effects.
        session.pending_keysounds.clear();
        session.pending_keysound_volumes.clear();
        session.pending_mine_hits.clear();
        session.pending_skin_events.clear();
        if session.state == PlayState::Playing {
            session.state = previous_state;
        }
        session.bgm_scheduler = BgmScheduler::starting_at(&session.chart, start_time);
        session.auto_keysound_scheduler =
            AutoKeysoundScheduler::starting_at(&session.chart, start_time);
        return;
    }
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
    // The seek already restored prefix judgements. Start hold integration here.
    let before = TimeUs(start_time.0.saturating_sub(1));
    session.conditional.processed_time = Some(before);
    if session.judge.has_hcn(&session.chart) {
        update_hcn_lane_timers(session, start_time);
        session.last_hcn_gauge_at = Some(start_time);
    }
    if let Some(opponent) = &mut session.opponent.independent {
        opponent.judge.skip_before(&opponent.chart, start_time);
        apply_battle_opponent_viewer_pgreat_prefix(opponent, start_time);
        // The prefix has already been restored above. Never run its scoring clock
        // again from zero; leave the exact seek boundary available for inputs.
        opponent.conditional.processed_time = Some(before);
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
        if opponent.judge.has_hcn(&opponent.chart) {
            update_battle_opponent_hcn(opponent, start_time);
        }
    }
    session.bgm_scheduler = BgmScheduler::starting_at(&session.chart, start_time);
    session.auto_keysound_scheduler =
        AutoKeysoundScheduler::starting_at(&session.chart, start_time);
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

    if session.opponent.independent.is_none() {
        let opponent_events = viewer_pgreat_prefix_events(&session.chart, start_time, |lane| {
            display_only_lane_mask[lane.index()]
        });
        for event in opponent_events {
            if let Some(score) = &mut session.opponent.score {
                score.apply(&event);
            }
            if let Some(gauge) = &mut session.opponent.gauge {
                gauge.apply_judge(Judge::PGreat, 1.0);
            }
        }
        if session.scored_total_notes != 0
            && session.opponent.score.as_ref().is_some_and(|score| {
                score.past_notes == session.scored_total_notes
                    && score.combo == session.scored_total_notes
            })
        {
            session.opponent.full_combo_started_at = Some(start_time);
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
            if chart.is_free_zone_inner(note.id) {
                continue;
            }
            let note_id = match note.kind {
                NoteKind::Tap | NoteKind::FreeZoneEnd => Some(note.id),
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
                        if matches!(
                            pair.mode.unwrap_or(chart.metadata.long_note_mode),
                            LongNoteMode::Ln | LongNoteMode::Hln
                        ) {
                            pair.start_note_id
                        } else {
                            pair.end_note_id
                        }
                    })
                }
                NoteKind::Invisible
                | NoteKind::Mine
                | NoteKind::Potion
                | NoteKind::DummyTap
                | NoteKind::DummyLongStart
                | NoteKind::DummyLongEnd
                | NoteKind::DummyMine
                | NoteKind::DummyPotion => None,
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
                advances_score: true,
            });
        }
    }
    events.sort_by_key(|event| (event.time, event.lane.index(), event.note_id));
    events
}

fn advance_battle_opponent(
    session: &mut GameSession,
    audio_now: TimeUs,
    primary_cutoff: Option<TimeUs>,
) {
    let Some(opponent) = &session.opponent.independent else { return };
    if opponent_hold_failed(opponent) {
        return;
    }
    let end = opponent.replay_player.as_ref().and_then(ReplayPlayer::explicit_finish_at);
    let failed = session.scoring_closed_at.filter(|_| session.state == PlayState::Failed);
    let cutoff = end.into_iter().chain(primary_cutoff).chain(failed).min();
    advance_scoring(
        session,
        audio_now,
        cutoff.map_or(audio_now, |end| end.min(audio_now)),
        cutoff,
        ScoringTarget::Opponent,
    );
    let rival = session.opponent.independent.as_mut().unwrap();
    if let Some(end) = rival.replay_player.as_ref().and_then(|p| p.kaleid.as_ref())
        && end.finished_early
        && end.ended_at <= audio_now
    {
        super::conditional::finalize_defaults(&rival.chart, &mut rival.conditional, end.ended_at);
    }
}

fn score_battle_opponent_through(
    session: &mut GameSession,
    step: &super::conditional::ScoringStep,
    cutoff: Option<TimeUs>,
) {
    let opponent = session.opponent.independent.as_ref().unwrap();
    let replay_times =
        opponent.replay_player.as_ref().map(|p| p.scoring_times_through(step.until, cutoff));
    let mut ticks = replay_times.map(Vec::into_iter);
    while !session.play_end_reached() && !session.apply_play_end_request(false) {
        let time = match &mut ticks {
            Some(ticks) => ticks.next(),
            None => next_battle_opponent_time(
                session.opponent.independent.as_ref().unwrap(),
                step.until,
            ),
        };
        let Some(time) = time else { break };
        advance_battle_opponent_at(session, time, step.input_limit);
        let rival = session.opponent.independent.as_mut().unwrap();
        if let Some(replay) = &mut rival.replay_player {
            replay.complete_scoring_tick(time);
        }
        if opponent_hold_failed(rival) {
            return;
        }
    }
    // Conditional charts also finish the exact (possibly fractional) interval, like
    // the primary judge. Two inputs on this boundary can straddle the chart change.
    if !session.play_end_reached()
        && session.opponent.independent.as_ref().unwrap().chart.metadata.conditional.is_some()
    {
        advance_battle_opponent_at(session, step.until, step.input_limit);
    }
}

/// Preserve the fixed scoring cadence while a judgement/hold is pending, but
/// jump across idle chart intervals. Compute lazily so a stall does not allocate
/// one timestamp per millisecond, and newly selected branches are seen immediately.
pub(super) fn next_battle_opponent_time(
    opponent: &BattleOpponentSession,
    now: TimeUs,
) -> Option<TimeUs> {
    let previous = opponent.conditional.processed_time.unwrap_or(TimeUs(-1));
    if previous >= now {
        return None;
    }
    if !uses_fixed_opponent_clock(opponent) {
        return Some(now);
    }
    let next_tick = TimeUs(previous.0.div_euclid(1_000).saturating_add(1).saturating_mul(1_000));
    let pending = opponent.judge.hln_body_visuals().next().is_some()
        || opponent.lane_hcn_timer.iter().any(Option::is_some)
        || Lane::ALL.iter().any(|lane| {
            let state = &opponent.judge.lanes[lane.index()];
            state.active_long.is_some()
                || opponent.chart.notes_for_lane(*lane)[state.next_note_index..]
                    .iter()
                    .take_while(|note| note.time <= previous)
                    .any(|note| {
                        matches!(note.kind, NoteKind::Tap | NoteKind::LongStart)
                            && !opponent.judge.judged_notes.contains_key(&note.id)
                    })
        });
    if pending {
        return Some(next_tick.min(now));
    }
    let next_event = opponent
        .chart
        .lane_notes
        .iter()
        .filter_map(|notes| {
            notes.get(notes.partition_point(|note| note.time <= previous)).map(|note| note.time)
        })
        .chain(super::conditional::next_evaluation_for(&opponent.chart, &opponent.conditional))
        .min()
        .unwrap_or(now);
    // Events were previously consumed at the following millisecond tick. Keep
    // that rounding, while still processing a partial final frame at `now`.
    let event_tick =
        TimeUs(next_event.0.saturating_add(999).div_euclid(1_000).saturating_mul(1_000));
    Some(event_tick.max(next_tick).min(now))
}

fn uses_fixed_opponent_clock(opponent: &BattleOpponentSession) -> bool {
    super::conditional::uses_ordered_timeline(&opponent.chart)
        || opponent.chart.long_notes.iter().any(|pair| {
            pair.mode.unwrap_or(opponent.chart.metadata.long_note_mode) == LongNoteMode::Hln
        })
}

fn advance_battle_opponent_at(session: &mut GameSession, now: TimeUs, input_limit: usize) {
    let boundaries = {
        let Some(opponent) = session.opponent.independent.as_ref() else { return };
        let previous = opponent.conditional.processed_time.unwrap_or(TimeUs(-1));
        let reflections = opponent.judge.bpm_reflections(&opponent.chart);
        let start = reflections.partition_point(|point| point.time <= previous);
        let mut boundaries = reflections[start..]
            .iter()
            .take_while(|point| point.time <= now)
            .flat_map(|point| [TimeUs(point.time.0 - 1), point.time])
            .filter(|&time| time > previous && time < now)
            .collect::<Vec<_>>();
        opponent.judge.append_hcn_boundaries(&opponent.chart, previous, now, &mut boundaries);
        boundaries.sort_unstable();
        boundaries.dedup();
        boundaries
    };
    for time in boundaries {
        score_battle_opponent_at(session, time, input_limit);
    }
    score_battle_opponent_at(session, now, input_limit);
}

fn score_battle_opponent_at(session: &mut GameSession, now: TimeUs, input_limit: usize) {
    use crate::replay::TimedInput;
    let playback_rate_percent = session.audio_clock.playback_rate_percent();
    let Some(opponent) = &mut session.opponent.independent else {
        return;
    };
    let display_uses_primary_arrangement = opponent.display_uses_primary_arrangement;
    let publish_display_judgements = opponent.publish_display_judgements;
    if opponent.conditional.processed_time.is_some_and(|time| now < time) {
        return;
    }
    opponent.conditional.processed_time = Some(now);
    if opponent_hold_failed(opponent) {
        return;
    }

    let rate = if opponent.replay_player.is_some() {
        opponent.replay_judge_rate_percent
    } else {
        playback_rate_percent
    };
    // The first update may jump directly to a note/zone boundary. Set this
    // before inputs, which can themselves settle a zone at that same timestamp.
    opponent.judge.free_zone_autoplay = std::array::from_fn(|i| {
        opponent.autoplay.as_ref().is_some_and(|auto| auto.is_lane_enabled(Lane::ALL[i]))
    });

    let inputs: Vec<_> = if let Some(replay) = &mut opponent.replay_player {
        replay.poll_timed_until(now, input_limit).collect()
    } else if let Some(autoplay) = &mut opponent.autoplay {
        autoplay
            .poll_until(&opponent.chart, now)
            .into_iter()
            .map(|input| TimedInput::new(input, None))
            .collect()
    } else {
        Vec::new()
    };
    if !inputs.is_empty() {
        sync_opponent_judge_windows(opponent, now, rate);
    }
    let mut display_judgements = super::input::apply_timed_inputs(opponent, inputs, now);
    sync_opponent_judge_windows(opponent, now, rate);
    advance_opponent_passage(opponent, now, true, &mut display_judgements);
    update_battle_opponent_hcn(opponent, now);
    update_battle_opponent_skin_timers(opponent, &display_judgements, now);

    if !publish_display_judgements {
        return;
    }

    for display in &mut display_judgements {
        let source_lane = if display_uses_primary_arrangement {
            display
                .judgement
                .note_id
                .and_then(|note_id| session.judge.note_by_id(&session.chart, note_id))
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

fn sync_opponent_judge_windows(opponent: &mut BattleOpponentSession, now: TimeUs, rate: u16) {
    set_judge_windows(
        &mut opponent.judge,
        &opponent.chart,
        opponent.base_judge_windows,
        opponent.key_mode,
        opponent.rule_mode,
        now,
        rate,
    );
}
impl super::input::InputScoring for BattleOpponentSession {
    type Event = DisplayJudgementEvent;
    fn stopped(&self) -> bool {
        opponent_hold_failed(self)
    }
    fn has_mines(&self) -> bool {
        self.judge.has_mines(&self.chart)
    }

    fn advance_to_input(
        &mut self,
        first: crate::replay::TimedInput,
        has_mines: bool,
        events: &mut Vec<Self::Event>,
    ) -> TimeUs {
        advance_opponent_passage(self, first.before_input(), has_mines, events);
        let time = first.delivery_time();
        events.extend(apply_battle_opponent_hcn_gauge(self, time));
        if self.replay_player.is_some() {
            sync_opponent_judge_windows(self, first.input.time, self.replay_judge_rate_percent);
        }
        time
    }

    fn judge_edge(
        &mut self,
        edge: crate::replay::TimedInput,
        _now: TimeUs,
    ) -> (InputEvent, JudgeOutcome) {
        let input = edge.input;
        let judge_time = if input.source == InputSource::Auto {
            input.time
        } else {
            opponent_reflected_time(self, input.time).unwrap_or(input.time)
        };
        let lane = input.lane.index();
        let held = &mut self.lane_key_hold[lane];
        held.update(input.kind, input.scratch_direction);
        if input.kind == InputKind::Press {
            self.lane_keyon_started_at[lane] = Some(input.time);
        } else if !held.is_held() {
            self.lane_keyon_started_at[lane] = None;
        }
        (input, self.judge.process_input_at(&self.chart, input, judge_time))
    }
    fn pass_mines(&mut self, time: TimeUs) -> Vec<Self::Event> {
        let outcome = battle_opponent_passes(self, time);
        apply_battle_opponent_outcome(self, outcome)
    }
    fn apply_judgement(&mut self, _input: InputEvent, outcome: JudgeOutcome) -> Vec<Self::Event> {
        apply_battle_opponent_outcome(self, outcome)
    }
    fn update_hold(&mut self, time: TimeUs) {
        update_battle_opponent_hcn(self, time);
    }
}

fn opponent_hold_failed(opponent: &BattleOpponentSession) -> bool {
    opponent.gauge.current_closes_play_on_zero() && opponent.judge.has_hcn(&opponent.chart)
}

fn advance_opponent_passage(
    opponent: &mut BattleOpponentSession,
    at: TimeUs,
    check_mines: bool,
    judgements: &mut Vec<DisplayJudgementEvent>,
) {
    judgements.extend(apply_battle_opponent_hcn_gauge(opponent, at));
    if check_mines && !opponent_hold_failed(opponent) {
        let outcome = battle_opponent_passes(opponent, at);
        judgements.extend(apply_battle_opponent_outcome(opponent, outcome));
    }
    if !opponent_hold_failed(opponent) {
        let outcome = battle_opponent_misses(opponent, at);
        judgements.extend(apply_battle_opponent_outcome(opponent, outcome));
    }
}

fn opponent_reflected_time(opponent: &BattleOpponentSession, at: TimeUs) -> Option<TimeUs> {
    use bmz_chart::timing::{bpm_reflection_at, reflect_time};

    bpm_reflection_at(&opponent.judge.bpm_reflections(&opponent.chart), at)
        .map(|point| reflect_time(at, point.time))
}

fn battle_opponent_misses(opponent: &mut BattleOpponentSession, at: TimeUs) -> JudgeOutcome {
    let reflected = opponent_reflected_time(opponent, at);
    let autoplay = opponent.autoplay.as_ref();
    opponent.judge.process_misses_at(
        &opponent.chart,
        at,
        |lane| {
            if autoplay.is_some_and(|auto| auto.is_lane_enabled(lane)) {
                at
            } else {
                reflected.unwrap_or(at)
            }
        },
        false,
    )
}

fn battle_opponent_passes(opponent: &mut BattleOpponentSession, at: TimeUs) -> JudgeOutcome {
    let mut held = opponent.lane_keyon_started_at;
    if let Some(autoplay) = &opponent.autoplay {
        for lane in Lane::ALL {
            if autoplay.is_lane_enabled(lane) {
                held[lane.index()] = None;
            }
        }
    }
    let reflected = opponent_reflected_time(opponent, at);
    opponent.judge.process_mine_passes_at(&opponent.chart, at, reflected, &held)
}

pub(super) fn apply_battle_opponent_outcome(
    opponent: &mut BattleOpponentSession,
    mut outcome: JudgeOutcome,
) -> Vec<DisplayJudgementEvent> {
    if opponent_hold_failed(opponent) {
        return Vec::new();
    }
    let has_hln = opponent.chart.long_notes.iter().any(|pair| {
        pair.mode.unwrap_or(opponent.chart.metadata.long_note_mode) == LongNoteMode::Hln
    });
    if has_hln || !outcome.hold_ticks.is_empty() || !outcome.potion_hits.is_empty() {
        enum Effect {
            Judge(JudgementEvent),
            Hold(crate::judge::model::HoldGaugeEvent),
            Mine(crate::judge::model::MineHitEvent),
            Potion(crate::judge::model::PotionHitEvent),
        }
        let mut effects = Vec::new();
        effects
            .extend(outcome.events.into_iter().map(|e| (e.time, e.lane.index(), Effect::Judge(e))));
        effects.extend(
            outcome.hold_ticks.into_iter().map(|e| (e.time, e.lane.index(), Effect::Hold(e))),
        );
        effects.extend(
            outcome.mine_hits.into_iter().map(|e| (e.time, e.lane.index(), Effect::Mine(e))),
        );
        effects.extend(
            outcome.potion_hits.into_iter().map(|e| (e.time, e.lane.index(), Effect::Potion(e))),
        );
        effects
            .sort_by_key(|(time, lane, effect)| (*time, !matches!(effect, Effect::Hold(_)), *lane));
        let mut judgements = Vec::new();
        for (time, _, effect) in effects {
            if opponent.gauge.current_closes_play_on_zero() {
                break;
            }
            let previous = opponent.gauge.current().value;
            match effect {
                Effect::Judge(event) => {
                    opponent.score.apply(&event);
                    if event.affects_score {
                        opponent.gauge.apply_judge(event.judge, 1.0);
                    }
                    judgements.push(DisplayJudgementEvent {
                        display_time: event.time,
                        judgement: event,
                        combo: opponent.score.combo,
                    });
                }
                Effect::Hold(event) if event.increase => opponent.gauge.apply_hcn_hold(),
                Effect::Hold(_) => opponent.gauge.apply_hcn_drain(),
                Effect::Mine(event) => opponent.gauge.apply_mine(event.damage),
                Effect::Potion(event) => opponent.gauge.apply_potion(event.recovery),
            }
            let current = opponent.gauge.current();
            update_gauge_increase_timer_state(
                &mut opponent.gauge_increase_started_at,
                previous,
                current.value,
                current.definition.max,
                time,
            );
        }
        return judgements;
    }
    let mut display_judgements = Vec::with_capacity(outcome.events.len());
    outcome.events.sort_by_key(|event| event.time);
    for event in outcome.events {
        opponent.score.apply(&event);
        if event.affects_score {
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
        display_judgements.push(DisplayJudgementEvent {
            display_time: event.time,
            judgement: event,
            combo: opponent.score.combo,
        });
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
        || opponent.chart.long_notes.iter().any(|pair| {
            pair.mode.unwrap_or(opponent.chart.metadata.long_note_mode) == LongNoteMode::Hln
                && pair.end_time > now
        })
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
        || super::conditional::next_evaluation(session).is_some()
        || session.chart.long_notes.iter().any(|pair| {
            pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == LongNoteMode::Hln
                && pair.end_time > session.audio_clock.now()
        })
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
    let Some(score) = &session.opponent.score else {
        return;
    };
    if session.opponent.full_combo_started_at.is_some()
        || session.chart.long_notes.iter().any(|pair| {
            pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == LongNoteMode::Hln
                && pair.end_time > session.audio_clock.now()
        })
        || session.scored_total_notes == 0
        || score.past_notes < session.scored_total_notes
        || score.combo < session.scored_total_notes
    {
        return;
    }
    session.opponent.full_combo_started_at = judgements
        .iter()
        .rev()
        .find(|event| session.display_only_lane_mask[event.lane.index()] && event.note_id.is_some())
        .map(|event| event.time)
        .or_else(|| Some(session.audio_clock.now()));
}

pub fn should_finish(session: &GameSession, audio_now: TimeUs) -> bool {
    session.judge.is_exhausted(&session.chart) && playback_deadline_passed(session, audio_now)
}

fn playback_deadline_passed(session: &GameSession, audio_now: TimeUs) -> bool {
    let end_time = session.result_wait_end_time.unwrap_or(session.chart.end_time);
    super::conditional::next_evaluation(session).is_none()
        && session.bgm_scheduler.is_done(&session.chart)
        && audio_now.0 > end_time.0.saturating_add(SESSION_END_MARGIN_US)
}

/// 採用する結果が確定し、これ以上スコア・ゲージ・判定詳細が変化しない状態かを返す。
///
/// 一度採点を閉じたセッションは、その後のオフセット変更や時計によらず確定のまま扱う。
/// 未確定のセッションでは [`natural_result_settled`] の条件を `audio_now` で確認する。
pub fn result_is_settled(session: &GameSession, audio_now: TimeUs) -> bool {
    session.scoring_closed_at.is_some() || natural_result_settled(session, audio_now)
}

/// 通常完走で、最終ノーツに対する Poor / Empty Poor / Mine の SLOW 側受付が終わり、
/// その範囲の採点処理も完了したかを返す。
///
/// - 全ノーツ（LN/CN/HCN の終端を含む）が判定済みであること。
/// - 人間入力が採点対象に残る場合、以後に取得する入力の判定時刻
///   (`取得時刻 + input_offset_us`) が `E + W` を超えること。
///   負の補正では入力配送の猶予も含めて待つ。
/// - フルリプレイでは、外側の時計ではなく実際に処理を終えた 1ms tick が `E + W` を超えること。
///   記録済み入力には現在のオフセットを再適用しないため、負オフセット分は待たない。
/// - 独立した対戦相手を持つ場合は、その採点も同じ条件で終わっていること。
///
/// `E` は不可視ノーツを除く判定対象・地雷・LN終端の最終時刻
/// （対象が無い譜面では従来の `chart.end_time`）、`W` は適用済み判定窓の
/// [`JudgeWindows::result_settle_margin_us`]（ルール・キー種別・ランク・再生速度を反映済み）。
/// 判定窓の境界 (`delta == W`) は Empty Poor として受け付けるため、比較は厳密な `>` にする。
/// `PlayState` は確認しない。採点を閉じる判断は `Playing` の更新内でだけ行う。
pub fn natural_result_settled(session: &GameSession, audio_now: TimeUs) -> bool {
    if super::conditional::next_evaluation(session).is_some()
        || !session.judge.is_exhausted(&session.chart)
    {
        return false;
    }
    let deadline = session
        .judge
        .result_end_time(&session.chart)
        .0
        .saturating_add(session.judge.window_set.result_settle_margin_us());
    let primary_settled = if session.replay_player.is_some() && session.replay_lane_mask.is_none() {
        let scored = if super::input::uses_ordered_inputs(session) {
            session.conditional.processed_time
        } else {
            session.replay_player.as_ref().and_then(ReplayPlayer::scored_until)
        };
        scored.is_some_and(|scored| scored.0 > deadline)
    } else {
        let delay = session.input_scoring_delay_us();
        audio_now.0.saturating_sub(delay) > deadline
            && (delay == 0 || session.conditional.processed_time.is_some_and(|at| at.0 > deadline))
    };
    primary_settled
        && session
            .opponent
            .independent
            .as_ref()
            .is_none_or(|opponent| battle_opponent_result_settled(opponent, audio_now))
}

impl GameSession {
    /// Chart-time delay for passive passage. Leave 3 ms of real time for input delivery.
    pub(crate) fn input_scoring_delay_us(&self) -> i64 {
        if self.offsets.input_offset_us >= 0
            || (self.replay_player.is_some() && self.replay_lane_mask.is_none())
            || self.autoplay.as_ref().is_some_and(AutoplayController::is_full)
        {
            return 0;
        }
        let delivery_us = 3_000 * i64::from(self.audio_clock.playback_rate_percent()) / 100;
        self.offsets.input_offset_us.saturating_neg().saturating_add(delivery_us)
    }
}

fn primary_score_time(session: &GameSession, audio_now: TimeUs) -> TimeUs {
    let delay = session.input_scoring_delay_us();
    if delay == 0 {
        return audio_now;
    }
    // A received input can be judged immediately. Integrate everything preceding it
    // first, and never undo that progress on a later wake without input.
    session
        .conditional
        .pending_inputs
        .iter()
        .map(|input| input.time)
        .filter(|&time| time <= audio_now)
        .chain(session.conditional.processed_time)
        .fold(TimeUs(audio_now.0.saturating_sub(delay)), TimeUs::max)
}

fn battle_opponent_result_settled(opponent: &BattleOpponentSession, audio_now: TimeUs) -> bool {
    if opponent_hold_failed(opponent) {
        return true;
    }
    if super::conditional::next_evaluation_for(&opponent.chart, &opponent.conditional).is_some() {
        return false;
    }
    if let Some(replay) = &opponent.replay_player
        && let Some(end) = replay.explicit_finish_at()
    {
        return replay.scored_until().is_some_and(|scored| scored >= end)
            || (end.0 < 0 && replay.events.first().is_none_or(|e| e.delivery_time() > end));
    }
    if !opponent.judge.is_exhausted(&opponent.chart) {
        return audio_now.0 > opponent.chart.end_time.0.saturating_add(SESSION_END_MARGIN_US)
            && bmz_chart::timing::bpm_reflection_at(
                &opponent.judge.bpm_reflections(&opponent.chart),
                audio_now,
            )
            .is_some();
    }
    let deadline = opponent
        .judge
        .result_end_time(&opponent.chart)
        .0
        .saturating_add(opponent.judge.window_set.result_settle_margin_us());
    let progress = match &opponent.replay_player {
        Some(replay) => replay.scored_until().map_or(i64::MIN, |scored| scored.0),
        None => audio_now.0,
    };
    progress > deadline
}

use super::*;

fn schedule_chart_audio(session: &mut GameSession, until: TimeUs, audio: &mut dyn AudioScheduler) {
    // A queued DEFAULT voice cannot be recalled reliably by the audio callback.
    // Keep the look-ahead strictly before the next undecided branch, even in READY.
    let until = super::conditional::next_evaluation(session)
        .map_or(until, |at| until.min(TimeUs(at.0.saturating_sub(1))));
    // BGM・自動キー音はchart 0に間に合うようREADY中もschedule-aheadする。
    // 判定・入力起因のkeysound・MineはPlayingに入るまで開始しない。
    session.bgm_scheduler.schedule_until(
        &session.chart,
        &session.audio_clock,
        until,
        session.audio_mix.master_volume
            * session.audio_mix.effective_normalization_gain()
            * session.audio_mix.bgm_volume,
        audio,
    );

    // キー音自動再生モード: ノーツの押下有無に関わらず、譜面の生タイミングで
    // キー音を鳴らす。入力オフセット・表示オフセットは適用しない。
    // 押鍵時のキー音は `schedule_keysounds` 側で抑制する。
    // 部分オートでは、自動演奏のレーンだけを予約する。
    if let Some(lanes) = scheduled_keysound_lanes(session) {
        let skipped = std::array::from_fn(|i| !lanes[i]);
        session.auto_keysound_scheduler.schedule_until(
            &session.chart,
            &skipped,
            &session.audio_clock,
            until,
            session.audio_mix.master_volume
                * session.audio_mix.effective_normalization_gain()
                * session.audio_mix.key_volume,
            audio,
            &session.play_end,
        );
    }
}
