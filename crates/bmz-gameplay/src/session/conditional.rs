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
    pub pending_inputs: Vec<InputEvent>,
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
pub(super) fn finalize_defaults(session: &mut GameSession, at: TimeUs) {
    let Some(program) = &session.chart.metadata.conditional else {
        return;
    };
    for (index, block) in program.blocks.iter().enumerate() {
        if !session.conditional.decisions.iter().any(|d| d.block == index) {
            session.conditional.decisions.push(BranchDecision {
                block: index,
                branch: block.conditions.len(),
                time: at.max(TimeUs(0)),
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

fn choose_decisions(
    chart: &PlayableChart,
    runtime: &ConditionalRuntime,
    at: TimeUs,
    value: impl Fn(&str) -> f64,
) -> Vec<BranchDecision> {
    let Some(program) = &chart.metadata.conditional else { return Vec::new() };
    // All blocks at this time see the same pre-boundary score/gauge snapshot.
    let mut decisions = Vec::new();
    for (i, block) in program.blocks.iter().enumerate() {
        if runtime.decisions.iter().any(|d| d.block == i) || program.evaluation_time(i, chart) != at
        {
            continue;
        }
        let choice = if let Some(replay) = &runtime.replay_decisions {
            replay
                .iter()
                .find(|d| d.block == i)
                .map_or(block.conditions.len(), |d| d.branch.min(block.conditions.len()))
        } else {
            block
                .conditions
                .iter()
                .position(|c| c.matches(value(&c.variable)))
                .unwrap_or(block.conditions.len())
        };
        decisions.push(BranchDecision { block: i, branch: choice, time: at });
    }
    decisions
}

fn evaluations(
    chart: &PlayableChart,
    runtime: &ConditionalRuntime,
    decisions: &[BranchDecision],
    value: impl Fn(&str) -> f64,
) -> Vec<bmz_core::replay::BranchEvaluation> {
    let Some(program) = &chart.metadata.conditional else { return Vec::new() };
    decisions
        .iter()
        .map(|decision| {
            let block = &program.blocks[decision.block];
            let values: std::collections::BTreeMap<_, _> = block
                .conditions
                .iter()
                .map(|condition| (condition.variable.clone(), value(&condition.variable)))
                .collect();
            let calculated_branch = block
                .conditions
                .iter()
                .position(|condition| condition.matches(values[&condition.variable]))
                .unwrap_or(block.conditions.len());
            bmz_core::replay::BranchEvaluation {
                decision: *decision,
                values,
                calculated_branch,
                from_replay: runtime.replay_decisions.is_some(),
            }
        })
        .collect()
}

pub(super) fn evaluate(session: &mut GameSession, at: TimeUs) {
    let Some(program) = session.chart.metadata.conditional.clone() else {
        return;
    };
    let reference_total =
        *session.conditional.reference_total.get_or_insert(session.scored_total_notes);
    let decisions = choose_decisions(&session.chart, &session.conditional, at, |name| {
        condition_value(
            name,
            &session.score,
            reference_total,
            session.gauge.current().value,
            session.conditional.best_lamp,
            session.conditional.replay_auto.unwrap_or_else(|| {
                session.autoplay.as_ref().is_some_and(AutoplayController::is_full)
            }),
        )
    });
    let evaluated = evaluations(&session.chart, &session.conditional, &decisions, |name| {
        condition_value(
            name,
            &session.score,
            reference_total,
            session.gauge.current().value,
            session.conditional.best_lamp,
            session.conditional.replay_auto.unwrap_or_else(|| {
                session.autoplay.as_ref().is_some_and(AutoplayController::is_full)
            }),
        )
    });
    let replay_mismatch = evaluated.iter().any(|evaluation| {
        evaluation.from_replay
            && (evaluation.calculated_branch != evaluation.decision.branch
                || session
                    .conditional
                    .replay_decisions
                    .as_ref()
                    .and_then(|decisions| {
                        decisions
                            .iter()
                            .find(|decision| decision.block == evaluation.decision.block)
                    })
                    .is_none_or(|recorded| recorded.time != evaluation.decision.time))
    });
    if replay_mismatch {
        let message =
            "Replayの記録枝または評価時刻が再計算と一致しません。記録枝で参考再生しています。";
        let provenance = &mut Arc::make_mut(&mut session.chart).metadata.kaleid;
        if !provenance.diagnostics.iter().any(|diagnostic| diagnostic == message) {
            provenance.diagnostics.push(message.into());
        }
    }
    session.conditional.evaluations.extend(evaluated);
    session.conditional.decisions.extend(decisions);
    let mut choices = vec![None; program.blocks.len()];
    for d in &session.conditional.decisions {
        choices[d.block] = Some(d.branch);
    }
    let mut chart = match program.materialize(&choices) {
        Ok(chart) => chart,
        Err(error) => {
            session.conditional.error = Some(error.to_string());
            // Keep the playable suffix and the failed-interpretation evidence.
            Arc::make_mut(&mut session.chart).metadata.kaleid.diagnostics.push(error.to_string());
            for decision in &mut session.conditional.decisions {
                if decision.time == at {
                    decision.branch = program.blocks[decision.block].conditions.len();
                }
            }
            return;
        }
    };
    chart.metadata.source_format = session.chart.metadata.source_format;
    chart.metadata.total = session.chart.metadata.total;
    for diagnostic in &session.chart.metadata.kaleid.diagnostics {
        if !chart.metadata.kaleid.diagnostics.contains(diagnostic) {
            chart.metadata.kaleid.diagnostics.push(diagnostic.clone());
        }
    }
    chart.metadata.conditional_revision = session.chart.metadata.conditional_revision + 1;
    chart.metadata.conditional = Some(program);
    retain_preloaded_sounds(&session.chart, &mut chart);
    preserve_history(&session.chart, &session.judge, &mut chart, at);
    rebind_audio(session, &chart, at);
    session.judge.rebind_chart(&session.chart, &chart);
    for lane in Lane::ALL {
        session.judge.lanes[lane.index()].next_mine_index =
            chart.lane_notes[lane.index()].partition_point(|n| n.time < at);
    }
    if let Some(auto) = &mut session.autoplay {
        auto.skip_before(&chart, at);
    }
    session.scored_total_notes =
        crate::score::scored_note_count_excluding_lanes(&chart, &session.display_only_lane_mask);
    session.timing_map =
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events);
    session.chart = Arc::new(chart);
}

pub(super) fn evaluate_opponent(opponent: &mut BattleOpponentSession, at: TimeUs) {
    let Some(program) = opponent.chart.metadata.conditional.clone() else { return };
    let reference_total =
        *opponent.conditional.reference_total.get_or_insert(opponent.scored_total_notes);
    let decisions = choose_decisions(&opponent.chart, &opponent.conditional, at, |name| {
        condition_value(
            name,
            &opponent.score,
            reference_total,
            opponent.gauge.current().value,
            opponent.conditional.best_lamp,
            opponent.conditional.replay_auto.unwrap_or_else(|| {
                opponent.autoplay.as_ref().is_some_and(AutoplayController::is_full)
            }),
        )
    });
    let evaluated = evaluations(&opponent.chart, &opponent.conditional, &decisions, |name| {
        condition_value(
            name,
            &opponent.score,
            reference_total,
            opponent.gauge.current().value,
            opponent.conditional.best_lamp,
            opponent.conditional.replay_auto.unwrap_or_else(|| {
                opponent.autoplay.as_ref().is_some_and(AutoplayController::is_full)
            }),
        )
    });
    opponent.conditional.evaluations.extend(evaluated);
    opponent.conditional.decisions.extend(decisions);
    let mut choices = vec![None; program.blocks.len()];
    for decision in &opponent.conditional.decisions {
        choices[decision.block] = Some(decision.branch);
    }
    let mut chart = match program.materialize(&choices) {
        Ok(chart) => chart,
        Err(error) => {
            opponent.conditional.error = Some(error.to_string());
            Arc::make_mut(&mut opponent.chart).metadata.kaleid.diagnostics.push(error.to_string());
            for decision in &mut opponent.conditional.decisions {
                if decision.time == at {
                    decision.branch = program.blocks[decision.block].conditions.len();
                }
            }
            return;
        }
    };
    chart.metadata.source_format = opponent.chart.metadata.source_format;
    chart.metadata.total = opponent.chart.metadata.total;
    chart.metadata.conditional_revision = opponent.chart.metadata.conditional_revision + 1;
    chart.metadata.conditional = Some(program);
    retain_preloaded_sounds(&opponent.chart, &mut chart);
    preserve_history(&opponent.chart, &opponent.judge, &mut chart, at);
    opponent.judge.rebind_chart(&opponent.chart, &chart);
    for lane in Lane::ALL {
        opponent.judge.lanes[lane.index()].next_mine_index =
            chart.lane_notes[lane.index()].partition_point(|n| n.time < at);
    }
    if let Some(auto) = &mut opponent.autoplay {
        auto.skip_before(&chart, at);
    }
    opponent.scored_total_notes = crate::score::scored_note_count(&chart);
    opponent.chart = Arc::new(chart);
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

pub(super) fn advance_before_boundary(
    session: &mut GameSession,
    at: TimeUs,
) -> Vec<JudgementEvent> {
    let mut events = super::input::process_hln_inputs(session, at);
    events.extend(advance_time(session, at));
    events
}

/// Split held-note intervals even when the owner crosses an entire LN in one wake.
pub(super) fn advance_time(session: &mut GameSession, at: TimeUs) -> Vec<JudgementEvent> {
    if session.state == PlayState::Failed {
        return Vec::new();
    }
    let previous = session.conditional.processed_time.unwrap_or(TimeUs(-1));
    if at < previous {
        return Vec::new();
    }
    let mut points = Vec::new();
    for pair in &session.chart.long_notes {
        if pair.mode.unwrap_or(session.chart.metadata.long_note_mode) == LongNoteMode::Hcn {
            points.extend([
                pair.start_time,
                TimeUs(
                    pair.start_time
                        .0
                        .saturating_add(session.judge.windows.bad_slow_us)
                        .saturating_add(1),
                ),
                pair.end_time,
            ]);
        }
    }
    points.retain(|t| *t > previous && *t < at);
    points.push(at);
    points.sort_unstable();
    points.dedup();
    let mut events = Vec::new();
    for time in points {
        // Integrate the old hold state up to this boundary, then apply its discrete events.
        apply_hcn_gauge(session, time);
        super::frame::sync_judge_windows(session, time);
        events.extend(super::input::process_mine_passes(session, time));
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
