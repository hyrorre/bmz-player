use crate::judge::model::HoldGaugeEvent;

/// HCN passing 中レーンの表示タイマー状態。
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
    for (index, pair) in session.judge.hold_timer_pairs_at(&session.chart, audio_now) {
        let idx = pair.lane.index();
        if next[idx].is_some() {
            continue;
        }
        let Some(timer) = hcn_timer_for_pair(
            &session.chart,
            &session.judge,
            index,
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
                .judge
                .note_by_id(&session.chart, pair.start_note_id)
                .is_some_and(|note| note.sounds().next().is_some())
        {
            let muted = !inclease;
            // Upstream updates HCN volume before playing newly missed heads.
            // Keep that first playback, then apply the normal passing mute
            // on the next update. Do not mark it as already applied here.
            let just_played_missed_head = muted
                && session.pending_keysounds.iter().any(|event| {
                    event.trigger == KeySoundTrigger::Miss && event.note_id == pair.start_note_id
                });
            if just_played_missed_head {
                next_muted[idx] = None;
                continue;
            }
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
                if let Some(note) = session.judge.note_by_id(&session.chart, pair.start_note_id) {
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
    index: usize,
    pair: &bmz_chart::model::LongNotePair,
    keyon: Option<TimeUs>,
    previous: Option<HcnLaneTimer>,
    now: TimeUs,
) -> Option<HcnLaneTimer> {
    if chart.is_free_zone_inner(pair.start_note_id) || chart.is_dummy_long(pair) {
        return None;
    }
    if pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln {
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
    for (index, pair) in opponent.judge.hold_timer_pairs_at(&opponent.chart, now) {
        let idx = pair.lane.index();
        if next[idx].is_some() {
            continue;
        }
        if let Some(timer) = hcn_timer_for_pair(
            &opponent.chart,
            &opponent.judge,
            index,
            pair,
            opponent.lane_keyon_started_at[idx],
            opponent.lane_hcn_timer[idx],
            now,
        ) {
            next[idx] = Some(timer);
        }
    }
    opponent.lane_hcn_timer = next;
    opponent.last_hcn_gauge_at = next.iter().any(Option::is_some).then_some(now);
}

/// Merge each lane's 200ms crossings in chart-time order. Gauge caps and depletion
/// make the order of a recovery and damage on different lanes observable.
fn hcn_ticks(
    chart: &PlayableChart,
    judge: &JudgeEngine,
    timers: &mut [Option<HcnLaneTimer>; LANE_COUNT],
    previous: TimeUs,
    now: TimeUs,
) -> impl Iterator<Item = HoldGaugeEvent> + use<> {
    use std::cmp::Reverse;
    let mut next = std::collections::BinaryHeap::new();
    let delta = now.0.saturating_sub(previous.0).max(0);
    let mut hln_lanes = [false; LANE_COUNT];
    for (index, pair) in judge.hold_timer_pairs_at(chart, previous) {
        if pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln
            && previous < pair.end_time
            && (pair.start_time <= previous || judge.hln_body_state(index, previous).is_some())
        {
            hln_lanes[pair.lane.index()] = true;
        }
    }
    for (lane, timer) in timers.iter_mut().enumerate() {
        let Some(timer) = timer else { continue };
        if hln_lanes[lane] {
            continue;
        }
        let count = timer.passing_count_us;
        if advance_hcn_counter(timer, delta) != 0 {
            let progress = if timer.inclease { count } else { -count };
            let first = TimeUs(previous.0 + (HCN_UPDATE_US + 1 - progress));
            next.push(Reverse((first, lane, timer.inclease)));
        }
    }
    std::iter::from_fn(move || {
        let Reverse((at, lane, held)) = next.pop()?;
        if let Some(next_at) = at.0.checked_add(HCN_UPDATE_US).map(TimeUs).filter(|&t| t <= now) {
            next.push(Reverse((next_at, lane, held)));
        }
        Some(HoldGaugeEvent { time: at, lane: Lane::ALL[lane], increase: held })
    })
}

/// Include HLN body events up to each HCN crossing. The common outcome handler
/// orders simultaneous ticks by lane and stops on the first fatal effect.
fn next_hold_outcome(
    ticks: &mut std::iter::Peekable<impl Iterator<Item = HoldGaugeEvent>>,
    judge: &mut JudgeEngine,
    chart: &PlayableChart,
) -> Option<JudgeOutcome> {
    let time = ticks.peek()?.time;
    let mut outcome = judge.advance_hln(chart, time);
    while let Some(tick) = ticks.next_if(|tick| tick.time == time) {
        outcome.hold_ticks.push(tick);
    }
    Some(outcome)
}

pub(super) fn apply_battle_opponent_hcn_gauge(
    opponent: &mut BattleOpponentSession,
    now: TimeUs,
) -> Vec<DisplayJudgementEvent> {
    let mut events = Vec::new();
    if let Some(previous) = opponent.last_hcn_gauge_at
        && now > previous
    {
        opponent.last_hcn_gauge_at = Some(now);
        let mut ticks = hcn_ticks(
            &opponent.chart,
            &opponent.judge,
            &mut opponent.lane_hcn_timer,
            previous,
            now,
        )
        .peekable();
        while !opponent.gauge.current_closes_play_on_zero()
            && let Some(outcome) =
                next_hold_outcome(&mut ticks, &mut opponent.judge, &opponent.chart)
        {
            events.extend(super::frame::apply_battle_opponent_outcome(opponent, outcome));
        }
    }
    events
}

/// beatoraja の HCN ゲージ増減判定 (passing ベース)。
/// `lane_hcn_timer` (HCN 区間内かつ始端判定済みのレーン) を参照し、
/// `mpassingcount` 相当の符号付きカウンタにフレーム経過時間を加減算して、
/// ±200ms を超えるたびに GREAT/BAD × rate 0.5 でゲージを 1 tick 更新する。
/// 始端を見逃しても途中から押し直せば回復する。
pub fn apply_hcn_gauge(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let mut events = Vec::new();
    if session.lane_hcn_timer.iter().all(Option::is_none) {
        session.last_hcn_gauge_at = None;
        return events;
    }

    let previous = session.last_hcn_gauge_at.unwrap_or(audio_now);
    if audio_now.0 <= previous.0 {
        session.last_hcn_gauge_at.get_or_insert(audio_now);
        return events;
    }
    session.last_hcn_gauge_at = Some(audio_now);

    let mut ticks =
        hcn_ticks(&session.chart, &session.judge, &mut session.lane_hcn_timer, previous, audio_now)
            .peekable();
    while session.state != PlayState::Failed
        && let Some(outcome) = next_hold_outcome(&mut ticks, &mut session.judge, &session.chart)
    {
        events.extend(apply_judge_outcome(session, outcome));
    }
    events
}
use super::*;
