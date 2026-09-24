/// HCN passing 中レーンの表示タイマー状態。
use super::judgement::{update_gauge_increase_timer, update_gauge_increase_timer_state};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HcnLaneTimer {
    /// 押下中 (回復中) なら true、離している (減衰中) なら false。
    pub inclease: bool,
    /// 現在の inclease 状態が始まった時刻。タイマー経過時間の起点。
    pub since: TimeUs,
    /// beatoraja `mpassingcount`: inclease 中は加算、離している間は減算する
    /// 符号付き経過時間 (us)。±200ms を超えるたびにゲージを 1 tick 更新する。
    /// inclease の反転ではリセットせず (相殺される)、passing 終了でリセットする。
    pub passing_count_us: i64,
}

/// beatoraja の TIMER_HCN_ACTIVE / TIMER_HCN_DAMAGE 切り替えと
/// HCN キー音の音量制御に対応する。
/// 「HCN の始端〜終端の区間内 (passing) かつ始端ノート判定済み」のレーンで、
/// inclease（押下中、または終端を PG/GR/GD で判定済み）に応じて
/// active/damage を切り替える。状態が反転したらタイマー起点 (`since`) を
/// リセットする。
/// キー音は beatoraja 同様、終端が BAD 以下で判定済み（早離し）の passing HCN
/// に限り、離している間は音量 0、押し直したら元の音量に戻す。
pub fn update_hcn_lane_timers(session: &mut GameSession, audio_now: TimeUs) {
    let mut next: [Option<HcnLaneTimer>; LANE_COUNT] = [None; LANE_COUNT];
    let mut next_muted: [Option<bool>; LANE_COUNT] = [None; LANE_COUNT];
    for pair in &session.chart.long_notes {
        let idx = pair.lane.index();
        if next[idx].is_some() {
            continue;
        }
        let Some(timer) = hcn_timer_for_pair(
            &session.chart,
            &session.judge,
            pair,
            session.lane_keyon_started_at[idx],
            session.lane_hcn_timer[idx],
            audio_now,
        ) else {
            continue;
        };
        let end_judge = session.judge.judged_notes.get(&pair.end_note_id).copied();
        let inclease = timer.inclease;
        next[idx] = Some(timer);

        // beatoraja: passing.getPair().getState() > 3 (終端 BAD 以下で判定済み)
        // のときのみキー音音量を制御する。
        if matches!(end_judge, Some(Judge::Bad | Judge::Poor | Judge::EmptyPoor))
            && session
                .chart
                .note_by_id(pair.start_note_id)
                .is_some_and(|note| note.sounds().next().is_some())
        {
            let muted = !inclease;
            if !session.display_only_lane_mask[idx]
                && session.lane_hcn_keysound_muted[idx] != Some(muted)
            {
                let volume = if muted {
                    0.0
                } else {
                    let chart_volume = bmz_chart::volume::chart_channel_volume_factor(
                        bmz_chart::volume::chart_volume_at_time(
                            &session.chart.key_volume_events,
                            pair.start_time,
                        ),
                    );
                    (session.audio_mix.master_volume
                        * session.audio_mix.effective_normalization_gain()
                        * session.audio_mix.key_volume
                        * chart_volume)
                        .clamp(0.0, 1.0)
                };
                if let Some(note) = session.chart.note_by_id(pair.start_note_id) {
                    session
                        .pending_keysound_volumes
                        .extend(note.sounds().map(|sound_id| (sound_id, volume)));
                }
            }
            next_muted[idx] = Some(muted);
        }
    }
    session.lane_hcn_timer = next;
    session.lane_hcn_keysound_muted = next_muted;
}

/// beatoraja `JudgeManager` の `hcnmduration` (200ms)。
const HCN_UPDATE_US: i64 = 200_000;

fn hcn_timer_for_pair(
    chart: &PlayableChart,
    judge: &JudgeEngine,
    pair: &bmz_chart::model::LongNotePair,
    keyon: Option<TimeUs>,
    previous: Option<HcnLaneTimer>,
    now: TimeUs,
) -> Option<HcnLaneTimer> {
    if pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln {
        let index =
            chart.long_notes.iter().position(|other| other.start_note_id == pair.start_note_id)?;
        let (held, _, since, counter) = judge.hln_body_state(index, now)?;
        return Some(HcnLaneTimer { inclease: held, since, passing_count_us: counter });
    }
    if pair.mode.unwrap_or(chart.metadata.long_note_mode) != LongNoteMode::Hcn
        || now < pair.start_time
        || now >= pair.end_time
        || !judge.judged_notes.contains_key(&pair.start_note_id)
    {
        return None;
    }
    let inclease = keyon.is_some()
        || matches!(
            judge.judged_notes.get(&pair.end_note_id),
            Some(Judge::PGreat | Judge::Great | Judge::Good)
        );
    Some(HcnLaneTimer {
        inclease,
        since: previous.filter(|prev| prev.inclease == inclease).map_or(now, |prev| prev.since),
        passing_count_us: previous.map_or(0, |prev| prev.passing_count_us),
    })
}

fn advance_hcn_counter(timer: &mut HcnLaneTimer, delta_us: i64) -> i64 {
    timer.passing_count_us += if timer.inclease { delta_us } else { -delta_us };
    let ticks = if timer.passing_count_us > HCN_UPDATE_US {
        (timer.passing_count_us - 1) / HCN_UPDATE_US
    } else if timer.passing_count_us < -HCN_UPDATE_US {
        (timer.passing_count_us + 1) / HCN_UPDATE_US
    } else {
        0
    };
    timer.passing_count_us -= ticks * HCN_UPDATE_US;
    ticks
}

pub(super) fn update_battle_opponent_hcn(opponent: &mut BattleOpponentSession, now: TimeUs) {
    let mut next = [None; LANE_COUNT];
    for pair in &opponent.chart.long_notes {
        let idx = pair.lane.index();
        if next[idx].is_some() {
            continue;
        }
        if let Some(timer) = hcn_timer_for_pair(
            &opponent.chart,
            &opponent.judge,
            pair,
            opponent.lane_keyon_started_at[idx],
            opponent.lane_hcn_timer[idx],
            now,
        ) {
            next[idx] = Some(timer);
        }
    }
    opponent.lane_hcn_timer = next;
    if next.iter().all(Option::is_none) {
        opponent.last_hcn_gauge_at = None;
        return;
    }
    let previous = opponent.last_hcn_gauge_at.replace(now).unwrap_or(now);
    let delta = now.0.saturating_sub(previous.0).max(0);
    for (idx, timer) in opponent.lane_hcn_timer.iter_mut().enumerate() {
        if hln_passing(&opponent.chart, idx, now) {
            continue;
        }
        let Some(timer) = timer else {
            continue;
        };
        let ticks = advance_hcn_counter(timer, delta);
        for _ in 0..ticks {
            let previous_gauge = opponent.gauge.current().value;
            opponent.gauge.apply_hcn_hold();
            let current = opponent.gauge.current();
            update_gauge_increase_timer_state(
                &mut opponent.gauge_increase_started_at,
                previous_gauge,
                current.value,
                current.definition.max,
                now,
            );
        }
        for _ in 0..-ticks {
            opponent.gauge.apply_hcn_drain();
        }
    }
}

/// beatoraja の HCN ゲージ増減判定 (passing ベース)。
/// `lane_hcn_timer` (HCN 区間内かつ始端判定済みのレーン) を参照し、
/// `mpassingcount` 相当の符号付きカウンタにフレーム経過時間を加減算して、
/// ±200ms を超えるたびに GREAT/BAD × rate 0.5 でゲージを 1 tick 更新する。
/// 始端を見逃しても途中から押し直せば回復する。
pub fn apply_hcn_gauge(session: &mut GameSession, audio_now: TimeUs) {
    if session.lane_hcn_timer.iter().all(Option::is_none) {
        session.last_hcn_gauge_at = None;
        return;
    }

    let previous = session.last_hcn_gauge_at.unwrap_or(audio_now);
    session.last_hcn_gauge_at = Some(audio_now);
    if audio_now.0 <= previous.0 {
        return;
    }

    let delta_us = audio_now.0 - previous.0;
    for idx in 0..LANE_COUNT {
        let hold_time =
            if session.chart.metadata.conditional.is_some() { previous } else { audio_now };
        if hln_passing(&session.chart, idx, hold_time) {
            continue;
        }
        let Some(mut timer) = session.lane_hcn_timer[idx] else {
            continue;
        };
        let ticks = advance_hcn_counter(&mut timer, delta_us);
        if ticks > 0 {
            for _ in 0..ticks {
                if session.display_only_lane_mask[idx] {
                    if let Some(gauge) = &mut session.opponent_gauge {
                        let previous_gauge = gauge.current().value;
                        gauge.apply_hcn_hold();
                        let current = gauge.current();
                        update_gauge_increase_timer_state(
                            &mut session.opponent_gauge_increase_started_at,
                            previous_gauge,
                            current.value,
                            current.definition.max,
                            audio_now,
                        );
                    }
                } else {
                    let previous_gauge = session.gauge.current().value;
                    session.gauge.apply_hcn_hold();
                    update_gauge_increase_timer(session, previous_gauge, audio_now);
                }
            }
        } else {
            for _ in 0..-ticks {
                if session.display_only_lane_mask[idx] {
                    if let Some(gauge) = &mut session.opponent_gauge {
                        gauge.apply_hcn_drain();
                    }
                } else {
                    session.gauge.apply_hcn_drain();
                }
            }
        }
        session.lane_hcn_timer[idx] = Some(timer);
    }
}
fn hln_passing(chart: &PlayableChart, lane: usize, now: TimeUs) -> bool {
    chart.long_notes.iter().any(|pair| {
        pair.lane.index() == lane
            && pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln
            && pair.start_time <= now
            && now < pair.end_time
    })
}
use super::*;
