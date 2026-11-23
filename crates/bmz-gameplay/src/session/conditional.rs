use super::*;
use bmz_core::replay::BranchDecision;

#[derive(Debug, Default)]
pub struct ConditionalRuntime {
    pub decisions: Vec<BranchDecision>,
    pub evaluations: Vec<bmz_core::replay::BranchEvaluation>,
    pub replay_decisions: Option<Vec<BranchDecision>>,
    pub reference_total: Option<u32>,
    /// BMZ ClearType index, snapshotted before starting this play.
    pub best_lamp: u8,
    pub replay_auto: Option<bool>,
    /// Human inputs received before their offset-adjusted judgement time, on every chart.
    pub pending_inputs: PendingInputs,
    pub error: Option<String>,
    pub(super) processed_time: Option<TimeUs>,
}

impl ConditionalRuntime {
    pub fn from_player(player: Option<&ReplayPlayer>) -> Self {
        let mut runtime =
            Self::with_replay(player.map(|p| p.branch_decisions.clone().unwrap_or_default()));
        if let Some(evidence) = player.and_then(|p| p.kaleid.as_ref()) {
            runtime.reference_total = Some(evidence.reference_total);
            runtime.best_lamp = evidence.preplay_lamp;
            runtime.replay_auto =
                evidence.options.get("autoplay").and_then(|value| value.as_bool());
        }
        runtime
    }
    pub fn with_replay(decisions: Option<Vec<BranchDecision>>) -> Self {
        Self { replay_decisions: decisions, ..Self::default() }
    }
}

pub fn next_evaluation(session: &GameSession) -> Option<TimeUs> {
    next_evaluation_for(&session.chart, &session.conditional)
}

pub(super) fn uses_ordered_timeline(chart: &PlayableChart) -> bool {
    chart.metadata.conditional.is_some()
        || !chart.metadata.free_zones.is_empty()
        || chart.metadata.kaleid.effective.potion
}

pub(super) fn next_evaluation_for(
    chart: &PlayableChart,
    runtime: &ConditionalRuntime,
) -> Option<TimeUs> {
    let program = chart.metadata.conditional.as_ref()?;
    (0..program.blocks.len())
        .filter(|index| !runtime.decisions.iter().any(|d| d.block == *index))
        .map(|i| program.evaluation_time(i, chart))
        .min()
}

/// At an early terminal state the still-visible DEFAULT suffix becomes final.
pub(super) fn finalize_defaults(
    chart: &PlayableChart,
    runtime: &mut ConditionalRuntime,
    at: TimeUs,
) {
    let Some(program) = &chart.metadata.conditional else {
        return;
    };
    if runtime.decisions.len() == program.blocks.len() {
        return;
    }
    // A fatal hold tick can predate a branch already selected on the audio clock.
    let time = at.max(runtime.decisions.last().map_or(TimeUs(0), |decision| decision.time));
    for (index, block) in program.blocks.iter().enumerate() {
        if !runtime.decisions.iter().any(|d| d.block == index) {
            runtime.decisions.push(BranchDecision {
                block: index,
                branch: block.conditions.len(),
                time,
            });
        }
    }
}

fn condition_value(
    name: &str,
    s: &ScoreState,
    reference_total: u32,
    gauge: f32,
    best_lamp: u8,
    autoplay: bool,
) -> f64 {
    let j = &s.judges;
    match name {
        "SCORE" => f64::from(s.ex_score()),
        "RATE" => {
            if reference_total == 0 {
                100.0
            } else {
                f64::from(s.ex_score()) * 50.0 / f64::from(reference_total)
            }
        }
        "COMBO" => f64::from(s.combo),
        "MAXCOMBO" => f64::from(s.max_combo),
        "PGREAT" => f64::from(j.fast_pgreat + j.slow_pgreat),
        "GREAT" => f64::from(s.total_great()),
        "GOOD" => f64::from(s.total_good()),
        "BAD" => f64::from(j.fast_bad + j.slow_bad),
        "POOR" => f64::from(j.fast_poor + j.slow_poor),
        "MISS" => f64::from(s.bp()),
        "GAUGE" => f64::from(gauge),
        "LAMP" => f64::from(best_lamp),
        "AUTO" => f64::from(autoplay),
        _ => 0.0,
    }
}

/// Both players stop at the same scoring position before changing the chart. Audio
/// determines when the choice is due; it never extends the scoring interval.
pub(super) struct ScoringStep {
    pub until: TimeUs,
    pub input_limit: usize,
    pub branch: Option<TimeUs>,
}

pub(super) fn next_scoring_step(
    chart: &PlayableChart,
    runtime: &ConditionalRuntime,
    replay: Option<&ReplayPlayer>,
    audio_now: TimeUs,
    score_now: TimeUs,
    cutoff: Option<TimeUs>,
) -> ScoringStep {
    let mut step = ScoringStep { until: score_now, input_limit: usize::MAX, branch: None };
    let Some(at) = next_evaluation_for(chart, runtime) else { return step };
    let recorded = replay
        .and_then(|player| player.kaleid.as_ref())
        .and_then(|evidence| evidence.evaluations.iter().find(|e| e.decision.time == at));
    // An unevaluated DEFAULT suffix is also saved at a stop. Only an evaluation
    // proves the branch took place beyond the last scoring update.
    if cutoff.is_some_and(|end| at > end) && recorded.is_none() {
        return step;
    }
    step.until = step.until.min(TimeUs(at.0.saturating_sub(1)));
    step.branch = (at <= audio_now).then_some(at);
    if let Some(position) = recorded.and_then(|e| e.score_position) {
        step.until = step.until.min(position.time);
        step.input_limit = position.input_count;
        if score_now < position.time {
            step.branch = None;
        }
    }
    step
}

fn select_chart(
    old: &mut Arc<PlayableChart>,
    runtime: &mut ConditionalRuntime,
    judge: &mut JudgeEngine,
    autoplay: &mut Option<AutoplayController>,
    score: &ScoreState,
    gauge: f32,
    total: u32,
    at: TimeUs,
    input_count: usize,
) -> Option<PlayableChart> {
    let program = old.metadata.conditional.clone()?;
    let reference_total = *runtime.reference_total.get_or_insert(total);
    let auto = runtime
        .replay_auto
        .unwrap_or_else(|| autoplay.as_ref().is_some_and(AutoplayController::is_full));
    let score_position = runtime
        .processed_time
        .map(|time| bmz_core::replay::ReplayScorePosition { time, input_count });
    let mut mismatch = false;
    let mut choices = vec![None; program.blocks.len()];
    for decision in &runtime.decisions {
        choices[decision.block] = Some(decision.branch);
    }
    for (i, block) in program.blocks.iter().enumerate() {
        if choices[i].is_some() || program.evaluation_time(i, old) != at {
            continue;
        }
        let value =
            |name| condition_value(name, score, reference_total, gauge, runtime.best_lamp, auto);
        let values: std::collections::BTreeMap<_, _> = block
            .conditions
            .iter()
            .map(|condition| (condition.variable.clone(), value(&condition.variable)))
            .collect();
        let default = block.conditions.len();
        let calculated_branch =
            block.conditions.iter().position(|c| c.matches(values[&c.variable])).unwrap_or(default);
        let mut branch = calculated_branch;
        if let Some(decisions) = &runtime.replay_decisions {
            let recorded = decisions.iter().find(|d| d.block == i);
            branch = recorded.map_or(default, |d| d.branch.min(default));
            mismatch |= branch != calculated_branch || recorded.is_none_or(|d| d.time != at);
        }
        let decision = BranchDecision { block: i, branch, time: at };
        choices[i] = Some(branch);
        runtime.decisions.push(decision);
        runtime.evaluations.push(bmz_core::replay::BranchEvaluation {
            decision,
            score_position,
            values,
            calculated_branch,
            from_replay: runtime.replay_decisions.is_some(),
        });
    }
    if mismatch {
        let message =
            "Replayの記録枝または評価時刻が再計算と一致しません。記録枝で参考再生しています。";
        let diagnostics = &mut Arc::make_mut(old).metadata.kaleid.diagnostics;
        if !diagnostics.iter().any(|s| s == message) {
            diagnostics.push(message.into());
        }
    }
    let mut chart = match program.materialize(&choices) {
        Ok(chart) => chart,
        Err(error) => {
            runtime.error = Some(error.to_string());
            Arc::make_mut(old).metadata.kaleid.diagnostics.push(error.to_string());
            for decision in runtime.decisions.iter_mut().filter(|d| d.time == at) {
                decision.branch = program.blocks[decision.block].conditions.len();
            }
            return None;
        }
    };
    chart.metadata.source_format = old.metadata.source_format;
    chart.metadata.total = old.metadata.total;
    chart.metadata.bmson_total_percent = old.metadata.bmson_total_percent;
    for diagnostic in &old.metadata.kaleid.diagnostics {
        if !chart.metadata.kaleid.diagnostics.contains(diagnostic) {
            chart.metadata.kaleid.diagnostics.push(diagnostic.clone());
        }
    }
    chart.metadata.conditional_revision = old.metadata.conditional_revision + 1;
    chart.metadata.conditional = Some(program);
    retain_preloaded_sounds(old, &mut chart);
    preserve_history(old, judge, &mut chart, at);
    let mine_cursors = judge.lanes.each_ref().map(|lane| lane.next_mine_index);
    judge.rebind_chart(old, &chart);
    // The original prefix is still in order. Resume at the first unprocessed
    // mine there, even if the branch has already scheduled audio beyond it.
    for lane in Lane::ALL {
        judge.lanes[lane.index()].next_mine_index = mine_cursors[lane.index()]
            .min(chart.notes_for_lane(lane).partition_point(|n| n.time < at));
    }
    if let Some(auto) = autoplay {
        let next = runtime
            .processed_time
            .map_or(TimeUs(i64::MIN), |time| TimeUs(time.0.saturating_add(1)));
        auto.skip_before(&chart, next.min(at));
    }
    Some(chart)
}

pub(super) fn evaluate(session: &mut GameSession, at: TimeUs) {
    let input_count = session
        .replay_player
        .as_ref()
        .map_or(session.replay_recorder.events.len(), |p| p.next_index);
    let Some(chart) = select_chart(
        &mut session.chart,
        &mut session.conditional,
        &mut session.judge,
        &mut session.autoplay,
        &session.score,
        session.gauge.current().value,
        session.scored_total_notes,
        at,
        input_count,
    ) else {
        return;
    };
    rebind_audio(session, &chart, at);
    session.scored_total_notes =
        crate::score::scored_note_count_excluding_lanes(&chart, &session.display_only_lane_mask);
    session.timing_map =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    if session.result_wait_end_time.is_some() {
        session.result_wait_end_time = Some(chart.last_placement_time(true));
    }
    session.chart = Arc::new(chart);
}

pub(super) fn evaluate_opponent(opponent: &mut BattleOpponentSession, at: TimeUs) {
    let input_count = opponent.replay_player.as_ref().map_or(0, |p| p.next_index);
    if let Some(chart) = select_chart(
        &mut opponent.chart,
        &mut opponent.conditional,
        &mut opponent.judge,
        &mut opponent.autoplay,
        &opponent.score,
        opponent.gauge.current().value,
        opponent.scored_total_notes,
        at,
        input_count,
    ) {
        opponent.scored_total_notes = crate::score::scored_note_count(&chart);
        opponent.chart = Arc::new(chart);
    }
}

fn retain_preloaded_sounds(old: &PlayableChart, new: &mut PlayableChart) {
    let mut sounds: std::collections::BTreeMap<_, _> =
        old.sounds.iter().map(|s| (s.id.0, s.clone())).collect();
    sounds.extend(new.sounds.iter().map(|s| (s.id.0, s.clone())));
    new.sounds = sounds.into_values().collect();
}

fn preserve_history(old: &PlayableChart, judge: &JudgeEngine, new: &mut PlayableChart, at: TimeUs) {
    let mut protected = std::collections::HashSet::new();
    for note in old.lane_notes.iter().flatten() {
        if note.time < at || judge.is_note_started(note.id) {
            protected.insert(note.id);
        }
    }
    for zone in &old.metadata.free_zones {
        if zone.start_time < at || protected.contains(&zone.note_id) {
            protected.insert(zone.note_id);
            protected.extend(zone.inner_notes.iter().copied());
            let mut removed = std::collections::HashSet::new();
            new.metadata.free_zones.retain(|z| {
                let overlaps = z.lane == zone.lane
                    && z.start_time <= zone.end_time
                    && z.end_time >= zone.start_time;
                if overlaps {
                    removed.insert(z.note_id);
                }
                !overlaps
            });
            for notes in &mut new.lane_notes {
                notes.retain(|n| !removed.contains(&n.id));
            }
            new.metadata.free_zones.push(zone.clone());
        }
    }
    for pair in &old.long_notes {
        if protected.contains(&pair.start_note_id) || protected.contains(&pair.end_note_id) {
            protected.insert(pair.start_note_id);
            protected.insert(pair.end_note_id);
            // A started LN owns its lane interval, including an endpoint the new branch removed.
            let mut removed = std::collections::HashSet::new();
            new.long_notes.retain(|p| {
                let overlaps = p.lane == pair.lane
                    && p.start_time <= pair.end_time
                    && p.end_time >= pair.start_time;
                if overlaps {
                    removed.insert(p.start_note_id);
                    removed.insert(p.end_note_id);
                }
                !overlaps
            });
            for notes in &mut new.lane_notes {
                notes.retain(|n| !removed.contains(&n.id));
            }
            new.long_notes.push(pair.clone());
            new.lane_notes[pair.lane.index()].retain(|n| {
                n.time < pair.start_time || n.time > pair.end_time || protected.contains(&n.id)
            });
        }
    }
    let mut removed = std::collections::HashSet::new();
    new.long_notes.retain(|p| {
        let replaces_tap = (protected.contains(&p.start_note_id)
            || protected.contains(&p.end_note_id))
            && !old.long_notes.iter().any(|old| {
                old.start_note_id == p.start_note_id && old.end_note_id == p.end_note_id
            });
        if replaces_tap {
            removed.insert(p.start_note_id);
            removed.insert(p.end_note_id);
        }
        !replaces_tap
    });
    for lane in Lane::ALL {
        let notes = &mut new.lane_notes[lane.index()];
        notes.retain(|n| !protected.contains(&n.id) && !removed.contains(&n.id) && n.time >= at);
        notes.extend(
            old.lane_notes[lane.index()].iter().filter(|n| protected.contains(&n.id)).cloned(),
        );
        notes.sort_by_key(|n| (n.time, n.id.0));
    }
    new.total_notes = new
        .lane_notes
        .iter()
        .flatten()
        .filter(|n| n.kind.is_scored_head() && !new.is_free_zone_inner(n.id))
        .count() as u32;
    new.long_notes.sort_by_key(|p| (p.start_tick, p.start_note_id.0));
    // Protected notes retain the audio region used at their first judgement.
    let protected_sounds: std::collections::HashSet<_> = old
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| protected.contains(&note.id))
        .flat_map(|note| note.sounds())
        .collect();
    for asset in &mut new.sounds {
        if protected_sounds.contains(&asset.id)
            && let Some(previous) = old.sounds.iter().find(|s| s.id == asset.id)
        {
            *asset = previous.clone();
        }
    }
    new.end_time = new
        .end_time
        .max(new.lane_notes.iter().flatten().map(|n| n.time).max().unwrap_or(TimeUs(0)));
}

fn rebind_audio(session: &mut GameSession, new: &PlayableChart, at: TimeUs) {
    let old = &session.chart;
    let mut scheduled = std::collections::HashSet::new();
    for (i, key) in old.metadata.conditional_bgm_keys.iter().enumerate() {
        if i < session.bgm_scheduler.next_index || session.bgm_scheduler.skip.contains(&i) {
            scheduled.insert(*key);
        }
    }
    session.bgm_scheduler = BgmScheduler::starting_at(new, at);
    for (i, key) in new.metadata.conditional_bgm_keys.iter().enumerate() {
        if scheduled.contains(key) {
            session.bgm_scheduler.skip.insert(i);
        }
    }
    let mut sounds = std::mem::take(&mut session.auto_keysound_scheduler.skip);
    for lane in Lane::ALL {
        for note in old.lane_notes[lane.index()]
            .iter()
            .take(session.auto_keysound_scheduler.next_note_index[lane.index()])
        {
            for sound in note.sounds() {
                sounds.insert((note.id, sound));
            }
        }
    }
    session.auto_keysound_scheduler = AutoKeysoundScheduler::starting_at(new, at);
    session.auto_keysound_scheduler.skip = sounds;
}

/// Split held-note intervals even when the owner crosses an entire LN in one wake.
pub(super) fn advance_time(session: &mut GameSession, at: TimeUs) -> Vec<JudgementEvent> {
    if session.state == PlayState::Failed || session.play_end_reached() {
        return Vec::new();
    }
    let previous = session.conditional.processed_time.unwrap_or(at.min(TimeUs(-1)));
    if at < previous {
        return Vec::new();
    }
    let mut points = Vec::new();
    let reflections = session.timing_map.bpm_reflections();
    let start = reflections.partition_point(|point| point.time <= previous);
    for point in reflections[start..].iter().take_while(|point| point.time <= at) {
        // Preserve due judgements on both sides even after a long wake.
        points.push(TimeUs(point.time.0 - 1));
        points.push(point.time);
    }
    session.judge.append_hcn_boundaries(&session.chart, previous, at, &mut points);
    points.retain(|t| *t > previous && *t < at);
    points.sort_unstable();
    points.dedup();
    let mut events = Vec::new();
    for time in points.into_iter().chain(std::iter::once(at)) {
        if session.play_end_reached() {
            break;
        }
        // Integrate the old hold state up to this boundary, then apply its discrete events.
        events.extend(apply_hcn_gauge(session, time));
        if session.state == PlayState::Failed {
            break;
        }
        super::frame::sync_judge_windows(session, time);
        events.extend(super::input::process_mine_passes(session, time));
        if session.state == PlayState::Failed {
            break;
        }
        events.extend(super::input::process_misses(session, time));
        update_hcn_lane_timers(session, time);
        if session.lane_hcn_timer.iter().any(Option::is_some) {
            session.last_hcn_gauge_at = Some(time);
        }
        session.conditional.processed_time = Some(time);
        super::judgement::update_failed_state_from_gauge(session);
        if session.state == PlayState::Failed {
            break;
        }
    }
    events
}
