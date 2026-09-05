pub fn apply_judge_outcome(
    session: &mut GameSession,
    mut outcome: JudgeOutcome,
) -> Vec<JudgementEvent> {
    if session.battle_opponent.is_some() {
        // An independent battle opponent is the sole authority for 2P score
        // and judgement presentation. The primary JudgeEngine still advances
        // the cloned presentation notes, but must not publish synthetic POORs
        // or duplicate replay judgements for those lanes.
        let had_display_only_event =
            outcome.events.iter().any(|event| session.display_only_lane_mask[event.lane.index()]);
        outcome.events.retain(|event| !session.display_only_lane_mask[event.lane.index()]);
        outcome.mine_hits.retain(|hit| !session.display_only_lane_mask[hit.lane.index()]);
        if had_display_only_event {
            outcome.keysound_volumes.clear();
        }
    }
    let has_display_only_event =
        outcome.events.iter().any(|event| session.display_only_lane_mask[event.lane.index()]);
    let mut display_only_combos = Vec::with_capacity(outcome.events.len());
    for event in &mut outcome.events {
        if session.display_only_lane_mask[event.lane.index()] {
            if event.affects_score {
                if let Some(score) = &mut session.opponent_score {
                    score.apply(event);
                }
                if let Some(gauge) = &mut session.opponent_gauge {
                    let previous_gauge = gauge.current().value;
                    gauge.apply_judge(event.judge, 1.0);
                    let current = gauge.current();
                    update_gauge_increase_timer_state(
                        &mut session.opponent_gauge_increase_started_at,
                        previous_gauge,
                        current.value,
                        current.definition.max,
                        event.time,
                    );
                }
            }
            event.affects_score = false;
            display_only_combos
                .push(session.opponent_score.as_ref().map_or(0, |score| score.combo));
        } else {
            display_only_combos.push(0);
        }
    }
    outcome.mine_hits.retain(|hit| {
        if session.display_only_lane_mask[hit.lane.index()] {
            if let Some(gauge) = &mut session.opponent_gauge {
                gauge.apply_mine(hit.damage);
            }
            false
        } else {
            true
        }
    });
    if has_display_only_event {
        // HCN の早離しに伴う音量変更も共有 sound id へ作用するため、表示専用の
        // opponent レーンから発生したものは primary の音声へ反映しない。
        outcome.keysound_volumes.clear();
    }
    outcome.keysounds.retain(|keysound| {
        session
            .chart
            .note_by_id(keysound.note_id)
            .is_none_or(|note| !session.display_only_lane_mask[note.lane.index()])
    });
    let mut events = Vec::with_capacity(outcome.events.len());
    for (event, display_only_combo) in outcome.events.into_iter().zip(display_only_combos) {
        if event.affects_score {
            session.score.apply(&event);
            update_course_combo_state(session, &event);
            let previous_gauge = session.gauge.current().value;
            session.gauge.apply_judge(event.judge, 1.0);
            update_gauge_increase_timer(session, previous_gauge, event.time);
            if let Some(note_id) = event.note_id {
                session.result_judgements.insert(
                    note_id,
                    ResultJudgementDetail {
                        judge: event.judge,
                        side: event.side,
                        delta: event.delta,
                        time: event.time,
                    },
                );
            }
        }
        let combo = if session.display_only_lane_mask[event.lane.index()] {
            display_only_combo
        } else {
            session.display_combo()
        };
        if event.affects_score || session.display_only_lane_mask[event.lane.index()] {
            session
                .recent_display_judgements
                .push(DisplayJudgementEvent { judgement: event.clone(), combo });
        }
        push_skin_runtime_event(session, SkinRuntimeEventKind::Judgement(event.clone()));
        events.push(event);
    }
    for hit in outcome.mine_hits {
        // Mine はスコア/コンボに影響を与えず、ゲージのみ削る。譜面指定音は通常の
        // keysound scheduler へ渡し、未指定時の既定SE判定はフレーム終端へ運ぶ。
        session.gauge.apply_mine(hit.damage);
        if hit.sound.is_some() {
            session.pending_keysounds.push(KeySoundEvent {
                note_id: hit.note_id,
                time: hit.time,
                trigger: KeySoundTrigger::Mine,
            });
        }
        session.pending_mine_hits.push(hit);
    }
    session.pending_keysounds.extend(outcome.keysounds);
    session.pending_keysound_volumes.extend(outcome.keysound_volumes);
    update_failed_state_from_gauge(session);
    events
}

pub(super) fn push_skin_runtime_event(session: &mut GameSession, kind: SkinRuntimeEventKind) {
    let sequence = session.next_skin_event_sequence;
    session.next_skin_event_sequence = session
        .next_skin_event_sequence
        .checked_add(1)
        .expect("skin runtime event sequence exhausted");
    session.pending_skin_events.push(SkinRuntimeEvent { sequence, kind });
}

impl GameSession {
    pub fn display_combo(&self) -> u32 {
        if self.course_combo_carry_active {
            self.course_combo_carry.saturating_add(self.score.combo)
        } else {
            self.score.combo
        }
    }

    pub fn display_max_combo(&self) -> u32 {
        self.course_max_combo.max(self.score.max_combo)
    }
}

fn update_course_combo_state(session: &mut GameSession, event: &JudgementEvent) {
    match event.judge {
        Judge::PGreat | Judge::Great | Judge::Good => {
            session.course_max_combo = session.course_max_combo.max(session.display_combo());
        }
        Judge::Bad | Judge::Poor => {
            session.course_combo_carry_active = false;
        }
        Judge::EmptyPoor if session.score.empty_poor_breaks_combo => {
            session.course_combo_carry_active = false;
        }
        Judge::EmptyPoor => {}
    }
}

pub(super) fn update_failed_state_from_gauge(session: &mut GameSession) {
    if session.state == PlayState::Playing && session.gauge.current_closes_play_on_zero() {
        session.state = PlayState::Failed;
    }
}

pub(super) fn update_gauge_increase_timer(
    session: &mut GameSession,
    previous_value: f32,
    now: TimeUs,
) {
    let current = session.gauge.current();
    update_gauge_increase_timer_state(
        &mut session.gauge_increase_started_at,
        previous_value,
        current.value,
        current.definition.max,
        now,
    );
}

pub(super) fn update_gauge_increase_timer_state(
    started_at: &mut Option<TimeUs>,
    previous_value: f32,
    current_value: f32,
    max_value: f32,
    now: TimeUs,
) {
    if current_value > previous_value + f32::EPSILON {
        *started_at = (current_value < max_value.max(1.0)).then_some(TimeUs(now.0.max(0)));
    }
}

pub(super) fn update_gauge_max_timer(session: &mut GameSession, now: TimeUs) {
    let current = session.gauge.current();
    update_gauge_max_timer_state(
        &mut session.gauge_increase_started_at,
        &mut session.gauge_max_started_at,
        current.value,
        current.definition.max,
        now,
    );
    if let Some(opponent_gauge) = &session.opponent_gauge {
        let current = opponent_gauge.current();
        update_gauge_max_timer_state(
            &mut session.opponent_gauge_increase_started_at,
            &mut session.opponent_gauge_max_started_at,
            current.value,
            current.definition.max,
            now,
        );
    }
}

pub(super) fn update_gauge_max_timer_state(
    increase_started_at: &mut Option<TimeUs>,
    max_started_at: &mut Option<TimeUs>,
    current_value: f32,
    max_value: f32,
    now: TimeUs,
) {
    let is_max = current_value >= max_value.max(1.0);
    if is_max {
        *increase_started_at = None;
    }
    match (is_max, *max_started_at) {
        (true, None) => *max_started_at = Some(TimeUs(now.0.max(0))),
        (false, Some(_)) => *max_started_at = None,
        _ => {}
    }
}
use super::*;
