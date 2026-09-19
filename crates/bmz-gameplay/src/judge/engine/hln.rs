use super::*;
use crate::judge::model::{HoldGaugeEvent, HoldSoundEvent};

const INTERVAL_US: i64 = 200_000;

#[derive(Debug, Clone, Default)]
pub(super) struct HlnState {
    initialized: bool,
    bodies: Vec<HlnBody>,
    /// 鍵盤、方向未記録スクラッチ、上、下の押下を独立に保持する。
    held: [[u16; 3]; LANE_COUNT],
}

#[derive(Debug, Clone)]
struct HlnBody {
    pair_index: usize,
    active: Option<ActiveLongNote>,
    activated: Option<TimeUs>,
    last: TimeUs,
    counter: i64,
    held: bool,
    reactive: bool,
    since: TimeUs,
    sound_started: bool,
    ended: bool,
}

impl HlnState {
    pub(super) fn rebind(&mut self, old: &PlayableChart, new: &PlayableChart) {
        let previous = std::mem::take(&mut self.bodies);
        self.initialized = false;
        self.ensure(new);
        for body in &mut self.bodies {
            let id = new.long_notes[body.pair_index].start_note_id;
            if let Some(saved) = previous.iter().find(|p| {
                old.long_notes[p.pair_index].start_note_id == id
                    && (p.activated.is_some() || p.ended)
            }) {
                let index = body.pair_index;
                *body = saved.clone();
                body.pair_index = index;
                if let Some(active) = &mut body.active {
                    active.pair_index = index;
                }
            }
        }
    }
    pub(super) fn is_exhausted(&self) -> bool {
        self.bodies.iter().all(|body| body.ended)
    }

    fn ensure(&mut self, chart: &PlayableChart) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.bodies = chart
            .long_notes
            .iter()
            .enumerate()
            .filter_map(|(pair_index, pair)| {
                (pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln).then_some(
                    HlnBody {
                        pair_index,
                        active: None,
                        activated: None,
                        last: pair.start_time,
                        counter: 0,
                        held: false,
                        reactive: false,
                        since: pair.start_time,
                        sound_started: false,
                        ended: false,
                    },
                )
            })
            .collect();
    }
}

pub(super) fn is_hln_head(chart: &PlayableChart, id: NoteId) -> bool {
    chart.long_notes.iter().any(|pair| {
        pair.start_note_id == id
            && pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln
    })
}

impl JudgeEngine {
    pub fn hln_body_visuals(
        &self,
    ) -> impl Iterator<Item = crate::judge::model::HlnBodyVisualState> + '_ {
        self.hln.bodies.iter().filter(|body| !body.ended).filter_map(|body| {
            Some(crate::judge::model::HlnBodyVisualState {
                pair_index: body.pair_index,
                activated: body.activated?,
                held: body.held,
                reactive: body.reactive,
            })
        })
    }
    /// 表示状態 (押下中、再保持/失敗後、切替時刻、符号付き保持時間)。
    pub fn hln_body_state(
        &self,
        pair_index: usize,
        now: TimeUs,
    ) -> Option<(bool, bool, TimeUs, i64)> {
        let body = self.hln.bodies.iter().find(|body| body.pair_index == pair_index)?;
        (body.activated.is_some_and(|at| at <= now) && !body.ended).then_some((
            body.held,
            body.reactive,
            body.since,
            body.counter,
        ))
    }

    pub(super) fn restore_hln_seek(&mut self, chart: &PlayableChart, at: TimeUs) {
        self.hln.ensure(chart);
        for body in &mut self.hln.bodies {
            let pair = &chart.long_notes[body.pair_index];
            if pair.end_time < at {
                body.ended = true;
                self.judged_notes.insert(pair.start_note_id, Judge::PGreat);
            } else if pair.start_time < at {
                body.active =
                    make_active_long(chart, pair.start_note_id, Judge::PGreat, TimeUs(0), at);
                body.activated = Some(at);
                body.last = at;
                body.since = at;
                body.held = true;
                // Viewer は開始位置から自動入力を復元する。過去の音声は再生しない。
                body.sound_started = true;
                self.hln.held[pair.lane.index()][0] = 1;
            }
        }
    }

    pub(super) fn advance_hln(&mut self, chart: &PlayableChart, now: TimeUs) -> JudgeOutcome {
        self.hln.ensure(chart);
        let mut outcome = JudgeOutcome::default();
        for body in &mut self.hln.bodies {
            let pair = &chart.long_notes[body.pair_index];
            let idx = pair.lane.index();
            let missed_at = TimeUs(
                pair.start_time
                    .0
                    .saturating_add(
                        press_window(self.window_set, self.scratch_lane_mask[idx]).bad_slow_us,
                    )
                    .saturating_add(1),
            );
            if body.activated.is_none()
                && now >= missed_at
                && !self.judged_notes.contains_key(&pair.start_note_id)
            {
                self.judged_notes.insert(pair.start_note_id, Judge::Poor);
                outcome.events.push(JudgementEvent {
                    note_id: Some(pair.start_note_id),
                    lane: pair.lane,
                    judge: Judge::Poor,
                    side: TimingSide::Slow,
                    delta: TimeUs(missed_at.0 - pair.start_time.0),
                    time: missed_at,
                    affects_score: true,
                });
                body.activated = Some(missed_at);
                body.last = missed_at;
                body.since = missed_at;
                body.reactive = true;
                body.held = self.hln.held[idx].iter().any(|count| *count != 0);
                if body.held && missed_at < pair.end_time {
                    body.sound_started = true;
                    outcome.hold_sounds.push(HoldSoundEvent {
                        note_id: pair.start_note_id,
                        time: missed_at,
                        start_at: Some(pair.start_time),
                        audible: true,
                    });
                }
            }
            if let Some(active) = body.active {
                let deadline = active.pending_release.map(|pending| {
                    TimeUs(pending.released_at.0.saturating_add(long_release_margin_us(
                        self.window_set,
                        self.scratch_lane_mask[idx],
                    )))
                });
                let end = pair.end_time.max(active.started_at);
                let final_at = deadline.map_or(end, |deadline| deadline.min(end));
                if now >= final_at {
                    let (judge, delta) = if deadline.is_some_and(|deadline| deadline <= end) {
                        let pending = active.pending_release.expect("release deadline");
                        (pending.judge, pending.delta)
                    } else {
                        (active.start_judge, active.start_delta)
                    };
                    body.active = None;
                    if self.lanes[idx]
                        .active_long
                        .is_some_and(|current| current.pair_index == body.pair_index)
                    {
                        self.lanes[idx].active_long = None;
                    }
                    self.judged_notes.insert(pair.start_note_id, judge);
                    outcome.events.push(ln_final_event(pair.lane, active, judge, delta, final_at));
                }
            }
            if body.ended {
                continue;
            }
            if body.activated.is_some() {
                // [開始, 終点) の全経過時間を積分し、終点で残量を捨てる。
                let until = now.min(pair.end_time);
                while until > body.last {
                    let to_tick = if body.held {
                        INTERVAL_US + 1 - body.counter
                    } else {
                        INTERVAL_US + 1 + body.counter
                    };
                    let dt = (until.0 - body.last.0).min(to_tick);
                    body.counter += if body.held { dt } else { -dt };
                    body.last.0 += dt;
                    if body.counter.abs() > INTERVAL_US {
                        outcome.hold_ticks.push(HoldGaugeEvent {
                            lane: pair.lane,
                            time: body.last,
                            increase: body.held,
                        });
                        body.counter += if body.held { -INTERVAL_US } else { INTERVAL_US };
                    }
                }
            }
            if now >= pair.end_time {
                body.ended = true;
                outcome.keysounds.push(KeySoundEvent {
                    note_id: pair.end_note_id,
                    time: pair.end_time,
                    trigger: KeySoundTrigger::NoteJudged,
                });
            }
        }
        outcome.hold_ticks.sort_by_key(|event| (event.time, event.lane.index()));
        outcome.events.sort_by_key(|event| (event.time, event.lane.index()));
        for lane in Lane::ALL {
            let idx = lane.index();
            if self.lanes[idx].active_long.is_none_or(|active| active.mode == LongNoteMode::Hln) {
                self.lanes[idx].active_long = self
                    .hln
                    .bodies
                    .iter()
                    .filter_map(|body| body.active)
                    .find(|active| chart.long_notes[active.pair_index].lane == lane);
            }
        }
        outcome
    }

    pub(super) fn update_hln_input(
        &mut self,
        chart: &PlayableChart,
        input: InputEvent,
        outcome: &mut JudgeOutcome,
    ) {
        let idx = input.lane.index();
        let direction = match input.scratch_direction {
            None => 0,
            Some(ScratchDirection::Up) => 1,
            Some(ScratchDirection::Down) => 2,
        };
        let count = &mut self.hln.held[idx][direction];
        match input.kind {
            InputKind::Press => *count = count.saturating_add(1),
            InputKind::Release => *count = count.saturating_sub(1),
        }
        let mut passing = false;
        for body in &mut self.hln.bodies {
            let pair = &chart.long_notes[body.pair_index];
            if pair.lane != input.lane {
                continue;
            }
            let captured = outcome
                .events
                .iter()
                .find(|event| event.note_id == Some(pair.start_note_id) && !event.affects_score);
            if let Some(event) = captured {
                body.active = make_active_long(
                    chart,
                    pair.start_note_id,
                    event.judge,
                    event.delta,
                    input.time,
                );
                if let Some(active) = &mut body.active {
                    active.scratch_direction = input.scratch_direction;
                }
                body.activated = Some(input.time.max(pair.start_time));
                body.last = input.time.max(pair.start_time);
                body.since = body.last;
                body.held = true;
                body.reactive = event.judge == Judge::Bad;
                body.sound_started = true;
                if !outcome.keysounds.iter().any(|sound| sound.note_id == pair.start_note_id) {
                    outcome.keysounds.push(KeySoundEvent {
                        note_id: pair.start_note_id,
                        time: input.time,
                        trigger: KeySoundTrigger::NoteJudged,
                    });
                }
            }
            if let Some(mut active) = body.active {
                if captured.is_none()
                    && input.kind == InputKind::Press
                    && active.pending_release.is_some()
                {
                    active.pending_release = None;
                    body.active = Some(active);
                    outcome.consumed_input = true;
                } else if input.kind == InputKind::Release
                    && (input.scratch_direction.is_none()
                        || active.scratch_direction.is_none()
                        || input.scratch_direction == active.scratch_direction)
                {
                    let end_delta = TimeUs(input.time.0 - pair.end_time.0);
                    let (judge, delta) = if end_delta.0 >= 0 {
                        (active.start_judge, active.start_delta)
                    } else {
                        combine_ln_judgement(
                            active,
                            classify_normal_delta(
                                end_delta.0,
                                long_end_window(self.window_set, self.scratch_lane_mask[idx]),
                            )
                            .unwrap_or(Judge::Poor),
                            end_delta,
                        )
                    };
                    if end_delta.0 < 0
                        && judge == Judge::Bad
                        && long_release_margin_us(self.window_set, self.scratch_lane_mask[idx]) > 0
                    {
                        // 二重 Release で猶予を延長しない。
                        active.pending_release.get_or_insert(PendingLongRelease {
                            released_at: input.time,
                            judge,
                            delta,
                        });
                        body.active = Some(active);
                    } else {
                        body.active = None;
                        self.judged_notes.insert(pair.start_note_id, judge);
                        outcome
                            .events
                            .push(ln_final_event(pair.lane, active, judge, delta, input.time));
                    }
                    outcome.consumed_input = true;
                }
            }
            if body.ended || body.activated.is_none() {
                continue;
            }
            passing |= input.time >= pair.start_time && input.time < pair.end_time;
            let held = self.hln.held[idx].iter().any(|count| *count != 0);
            if body.held != held {
                body.held = held;
                body.since = input.time.max(pair.start_time);
                body.reactive = true;
                let start_at = (held && !body.sound_started).then_some(pair.start_time);
                body.sound_started |= held;
                outcome.hold_sounds.push(HoldSoundEvent {
                    note_id: pair.start_note_id,
                    time: input.time,
                    start_at,
                    audible: held,
                });
            }
        }
        if passing
            && input.kind == InputKind::Press
            && outcome.events.iter().all(|event| event.judge == Judge::EmptyPoor)
        {
            outcome.events.clear();
            outcome.keysounds.clear();
            outcome.consumed_input = true;
        }
        // 短い HLN の終点を過ぎて始点を捕捉した場合も、その入力内で一度だけ確定。
        append_outcome(outcome, self.advance_hln(chart, input.time));
        // 既存 HOLD timer / Auto keybeam 用のレーン代表。採点状態は pair に保持する。
        if self.lanes[idx].active_long.is_none_or(|active| active.mode == LongNoteMode::Hln) {
            self.lanes[idx].active_long = self
                .hln
                .bodies
                .iter()
                .filter_map(|body| body.active)
                .find(|active| chart.long_notes[active.pair_index].lane == input.lane);
        }
    }
}
