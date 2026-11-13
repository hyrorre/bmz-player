use super::*;

pub(super) struct NoteArrangeEngine {
    pub(super) arrange: ArrangeOption,
    pub(super) rng: ArrangeRng,
    pub(super) groups: Vec<NoteArrangeGroup>,
    pub(super) s_random_scheme: SRandomScheme,
    pub(super) h_random_threshold_ms: Option<u32>,
}

impl NoteArrangeEngine {
    pub(super) fn new(
        arrange: ArrangeOption,
        seed: i64,
        groups: &[Vec<usize>],
        legacy_seed: bool,
        s_random_scheme: SRandomScheme,
        h_random_threshold_ms: Option<u32>,
    ) -> Self {
        Self {
            arrange,
            rng: ArrangeRng::new(seed, legacy_seed),
            groups: groups.iter().map(|lanes| NoteArrangeGroup::new(lanes)).collect(),
            s_random_scheme,
            h_random_threshold_ms,
        }
    }

    pub(super) fn arrange_timeline(
        &mut self,
        notes: &mut [NoteEvent],
        start_to_end: &std::collections::HashMap<bmz_core::ids::NoteId, bmz_core::ids::NoteId>,
        end_to_start: &std::collections::HashMap<bmz_core::ids::NoteId, bmz_core::ids::NoteId>,
    ) {
        let time = notes.first().map(|note| note.time).unwrap_or(TimeUs(0));
        for group in &mut self.groups {
            let map = group.randomize(
                notes,
                time,
                self.arrange,
                self.s_random_scheme,
                self.h_random_threshold_ms,
                &mut self.rng,
            );
            for note in notes.iter_mut() {
                let source = note.lane.index();
                let Some(&dest) = map.get(&source) else {
                    continue;
                };
                note.lane = Lane::ALL[dest];
                if matches!(note.kind, NoteKind::LongStart | NoteKind::DummyLongStart) {
                    if start_to_end.contains_key(&note.id) {
                        group.active_ln.insert(source, dest);
                    }
                } else if matches!(note.kind, NoteKind::LongEnd | NoteKind::DummyLongEnd)
                    && end_to_start.contains_key(&note.id)
                {
                    group.active_ln.remove(&source);
                }
            }
        }
    }
}

pub(super) struct NoteArrangeGroup {
    pub(super) lanes: Vec<usize>,
    pub(super) last_note_time: std::collections::HashMap<usize, TimeUs>,
    pub(super) active_ln: std::collections::HashMap<usize, usize>,
    pub(super) spiral_increment: usize,
    pub(super) spiral_head: usize,
    pub(super) scratch_lanes: Vec<usize>,
    pub(super) scratch_index: usize,
    pub(super) lane_history: [LaneHistory; LANE_COUNT],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct LaneHistory {
    pub(super) last_time: Option<TimeUs>,
    pub(super) rapid_streak: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LmCandidateClass {
    Safe,
    FourPlusWithin8F,
    ThreePlusWithin7F,
    TwoPlusWithin6F,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LmCandidate {
    lane: usize,
    gap: i64,
}

pub(super) fn lm_interval_frames_120(previous_time: TimeUs, current_time: TimeUs) -> i64 {
    let elapsed_us = (i128::from(current_time.0) - i128::from(previous_time.0)).max(0);
    // LMの譜面時刻は整数msとして扱われ、その経過時間を120 Hzフレームへ切り下げる。
    // 絶対時刻を個別にフレーム化してから引くと端数位相で切り上がり、BPM 255/260の
    // 16分が6F補正から外れるため、経過時間を先に整数msへ切り下げる。
    let elapsed_ms = elapsed_us.div_euclid(1_000);
    (elapsed_ms * 120).div_euclid(1_000) as i64
}

pub(super) fn next_lane_history(history: LaneHistory, current_time: TimeUs) -> (LaneHistory, i64) {
    let Some(last_time) = history.last_time else {
        return (LaneHistory { last_time: Some(current_time), rapid_streak: 1 }, i64::MAX);
    };

    debug_assert!(current_time >= last_time, "arrange timelines must be time-sorted");
    let gap = lm_interval_frames_120(last_time, current_time);
    let rapid_streak = if gap > 8 { 1 } else { history.rapid_streak.saturating_add(1) };
    (LaneHistory { last_time: Some(current_time), rapid_streak }, gap)
}

pub(super) fn classify_lm_candidate(
    history: LaneHistory,
    current_time: TimeUs,
) -> LmCandidateClass {
    let (next, gap) = next_lane_history(history, current_time);
    if gap <= 6 && next.rapid_streak >= 2 {
        LmCandidateClass::TwoPlusWithin6F
    } else if gap <= 7 && next.rapid_streak >= 3 {
        LmCandidateClass::ThreePlusWithin7F
    } else if gap <= 8 && next.rapid_streak >= 4 {
        LmCandidateClass::FourPlusWithin8F
    } else {
        LmCandidateClass::Safe
    }
}

impl NoteArrangeGroup {
    pub(super) fn new(lanes: &[usize]) -> Self {
        let scratch_lanes: Vec<usize> = lanes
            .iter()
            .copied()
            .filter(|&lane| lane == Lane::Scratch.index() || lane == Lane::Scratch2.index())
            .collect();
        Self {
            lanes: lanes.to_vec(),
            last_note_time: lanes.iter().copied().map(|lane| (lane, TimeUs(-10_000_000))).collect(),
            active_ln: std::collections::HashMap::new(),
            spiral_increment: 0,
            spiral_head: 0,
            scratch_lanes,
            scratch_index: 0,
            lane_history: [LaneHistory::default(); LANE_COUNT],
        }
    }

    pub(super) fn randomize(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        arrange: ArrangeOption,
        s_random_scheme: SRandomScheme,
        h_random_threshold_ms: Option<u32>,
        rng: &mut ArrangeRng,
    ) -> std::collections::HashMap<usize, usize> {
        if self.lanes.is_empty() {
            return std::collections::HashMap::new();
        }
        if arrange == ArrangeOption::Spiral {
            return self.spiral_map(rng);
        }

        if matches!(arrange, ArrangeOption::SRandom | ArrangeOption::SRandomEx)
            && s_random_scheme == SRandomScheme::Lm120HzV1
        {
            return self.lm_120hz_shuffle(notes, time, rng);
        }

        let mut changeable = self.changeable_lanes();
        let mut assignable = self.assignable_lanes();
        let mut map = std::collections::HashMap::new();
        map.extend(self.active_ln.iter().map(|(&source, &dest)| (source, dest)));

        if arrange == ArrangeOption::AllScratch {
            self.assign_all_scratch(notes, time, rng, &mut changeable, &mut assignable, &mut map);
        }

        let s_random_threshold = match s_random_scheme {
            // beatoraja S_RANDOM_NO_THRESHOLD / LR2: any later note may repeat.
            SRandomScheme::NoThresholdV1 => TimeUs(0),
            SRandomScheme::Legacy40MsV1 | SRandomScheme::Lm120HzV1 => TimeUs(40_000),
        };
        let threshold = match arrange {
            ArrangeOption::SRandom | ArrangeOption::SRandomEx => s_random_threshold,
            ArrangeOption::HRandom | ArrangeOption::AllScratch => TimeUs(
                i64::from(h_random_threshold_ms.unwrap_or(LEGACY_H_RANDOM_THRESHOLD_MS)) * 1_000,
            ),
            _ => TimeUs(40_000),
        };
        map.extend(self.time_based_shuffle(notes, time, threshold, rng, changeable, assignable));
        map
    }

    /// LM approximation v1: 120 Hz logical-frame correction with 8F/7F/6F buckets.
    ///
    /// The exact RNG call order is replay data for this scheme. Do not change selection or
    /// shuffle calls without introducing a new S-RANDOM scheme.
    pub(super) fn lm_120hz_shuffle(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        rng: &mut ArrangeRng,
    ) -> std::collections::HashMap<usize, usize> {
        let changeable = self.changeable_lanes();
        let assignable = self.assignable_lanes();
        let mut map = std::collections::HashMap::new();
        map.extend(self.active_ln.iter().map(|(&source, &dest)| (source, dest)));

        let mut note_lanes = Vec::new();
        let mut empty_lanes = Vec::new();
        for lane in changeable {
            if notes.iter().any(|note| note.lane.index() == lane && affects_arrange_history(note)) {
                note_lanes.push(lane);
            } else {
                empty_lanes.push(lane);
            }
        }

        // A mine-only timeline must not advance the arrange RNG or correction history. A cloned
        // RNG still gives its objects a deterministic shuffled mapping while active long-note
        // reservations and the following normal-note arrangement remain unchanged.
        if note_lanes.is_empty() {
            let mut mine_destinations = assignable;
            let mut mine_rng = rng.clone();
            shuffle_lm_lanes(&mut mine_destinations, &mut mine_rng);
            map.extend(empty_lanes.into_iter().zip(mine_destinations));
            return map;
        }

        let mut safe = Vec::new();
        let mut four_plus = Vec::new();
        let mut three_plus = Vec::new();
        let mut two_plus = Vec::new();
        for lane in assignable.iter().copied() {
            let history = self.lane_history[lane];
            let (_, gap) = next_lane_history(history, time);
            let candidate = LmCandidate { lane, gap };
            match classify_lm_candidate(history, time) {
                LmCandidateClass::Safe => safe.push(candidate),
                LmCandidateClass::FourPlusWithin8F => four_plus.push(candidate),
                LmCandidateClass::ThreePlusWithin7F => three_plus.push(candidate),
                LmCandidateClass::TwoPlusWithin6F => two_plus.push(candidate),
            }
        }

        let mut selected = Vec::with_capacity(note_lanes.len());
        while selected.len() < note_lanes.len() && !safe.is_empty() {
            let index = rng.next_usize(safe.len());
            selected.push(safe.remove(index).lane);
        }
        for bucket in [&mut four_plus, &mut three_plus, &mut two_plus] {
            select_lm_violation_candidates(
                bucket,
                note_lanes.len().saturating_sub(selected.len()),
                rng,
                &mut selected,
            );
        }
        debug_assert_eq!(selected.len(), note_lanes.len());

        shuffle_lm_lanes(&mut selected, rng);
        for (&source, &dest) in note_lanes.iter().zip(&selected) {
            map.insert(source, dest);
        }

        let mut selected_destinations = [false; LANE_COUNT];
        for &dest in &selected {
            selected_destinations[dest] = true;
        }
        let mut remaining_destinations: Vec<usize> =
            assignable.into_iter().filter(|&lane| !selected_destinations[lane]).collect();
        shuffle_lm_lanes(&mut remaining_destinations, rng);
        debug_assert_eq!(empty_lanes.len(), remaining_destinations.len());
        map.extend(empty_lanes.into_iter().zip(remaining_destinations));

        // Timeline-start history was frozen until the mapping became complete. A source lane is
        // represented once here even if it contains multiple objects at the same timeline.
        for source in note_lanes {
            if let Some(&dest) = map.get(&source) {
                self.lane_history[dest] = next_lane_history(self.lane_history[dest], time).0;
            }
        }
        map
    }

    pub(super) fn changeable_lanes(&self) -> Vec<usize> {
        self.lanes.iter().copied().filter(|lane| !self.active_ln.contains_key(lane)).collect()
    }

    pub(super) fn assignable_lanes(&self) -> Vec<usize> {
        self.lanes
            .iter()
            .copied()
            .filter(|lane| !self.active_ln.values().any(|active| active == lane))
            .collect()
    }

    pub(super) fn time_based_shuffle(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        threshold: TimeUs,
        rng: &mut ArrangeRng,
        changeable: Vec<usize>,
        assignable: Vec<usize>,
    ) -> std::collections::HashMap<usize, usize> {
        let mut note_lane = Vec::new();
        let mut empty_lane = Vec::new();
        for lane in changeable {
            if notes.iter().any(|note| note.lane.index() == lane && affects_arrange_history(note)) {
                note_lane.push(lane);
            } else {
                empty_lane.push(lane);
            }
        }

        let mut primary_lane = Vec::new();
        let mut inferior_lane = Vec::new();
        for lane in assignable {
            let last = self.last_note_time.get(&lane).copied().unwrap_or(TimeUs(-10_000_000));
            if time.0 - last.0 > threshold.0 {
                primary_lane.push(lane);
            } else {
                inferior_lane.push(lane);
            }
        }

        let mut map = std::collections::HashMap::new();
        while !note_lane.is_empty() && !primary_lane.is_empty() {
            let index = rng.next_usize(primary_lane.len());
            map.insert(note_lane.remove(0), primary_lane.remove(index));
        }
        while !note_lane.is_empty() && !inferior_lane.is_empty() {
            let min_time = inferior_lane
                .iter()
                .filter_map(|lane| self.last_note_time.get(lane))
                .map(|time| time.0)
                .min()
                .unwrap_or(-10_000_000);
            let candidates: Vec<usize> = inferior_lane
                .iter()
                .copied()
                .filter(|lane| {
                    self.last_note_time.get(lane).map(|time| time.0).unwrap_or(-10_000_000)
                        == min_time
                })
                .collect();
            let dest = candidates[rng.next_usize(candidates.len())];
            map.insert(note_lane.remove(0), dest);
            inferior_lane.retain(|&lane| lane != dest);
        }

        primary_lane.extend(inferior_lane);
        while !empty_lane.is_empty() && !primary_lane.is_empty() {
            let index = rng.next_usize(primary_lane.len());
            map.insert(empty_lane.remove(0), primary_lane.remove(index));
        }

        for (&source, &dest) in &map {
            if notes.iter().any(|note| note.lane.index() == source && affects_arrange_history(note))
            {
                self.last_note_time.insert(dest, time);
            }
        }
        map
    }

    pub(super) fn spiral_map(
        &mut self,
        rng: &mut ArrangeRng,
    ) -> std::collections::HashMap<usize, usize> {
        if self.lanes.len() < 2 {
            return std::collections::HashMap::new();
        }
        if self.spiral_increment == 0 {
            self.spiral_increment = rng.next_usize(self.lanes.len() - 1) + 1;
        }
        let changeable = self.changeable_lanes();
        if changeable.len() == self.lanes.len() {
            self.spiral_head = (self.spiral_head + self.spiral_increment) % self.lanes.len();
        }
        let mut map = std::collections::HashMap::new();
        map.extend(self.active_ln.iter().map(|(&source, &dest)| (source, dest)));
        for (index, &lane) in self.lanes.iter().enumerate() {
            if changeable.contains(&lane) {
                map.insert(lane, self.lanes[(index + self.spiral_head) % self.lanes.len()]);
            }
        }
        map
    }

    pub(super) fn assign_all_scratch(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        _rng: &mut ArrangeRng,
        changeable: &mut Vec<usize>,
        assignable: &mut Vec<usize>,
        map: &mut std::collections::HashMap<usize, usize>,
    ) {
        if self.scratch_lanes.is_empty() {
            return;
        }
        let scratch = self.scratch_lanes[self.scratch_index];
        let last = self.last_note_time.get(&scratch).copied().unwrap_or(TimeUs(-10_000_000));
        if !assignable.contains(&scratch) || time.0 - last.0 <= 40_000 {
            return;
        }
        let Some(source) = changeable.iter().copied().find(|&lane| {
            notes.iter().any(|note| note.lane.index() == lane && affects_arrange_history(note))
        }) else {
            return;
        };
        map.insert(source, scratch);
        changeable.retain(|&lane| lane != source);
        assignable.retain(|&lane| lane != scratch);
        self.last_note_time.insert(scratch, time);
        self.scratch_index = (self.scratch_index + 1) % self.scratch_lanes.len();
    }
}

fn affects_arrange_history(note: &NoteEvent) -> bool {
    !note.kind.is_dummy()
        && !matches!(note.kind, NoteKind::Mine | NoteKind::Potion | NoteKind::FreeZoneEnd)
}

fn select_lm_violation_candidates(
    candidates: &mut Vec<LmCandidate>,
    limit: usize,
    rng: &mut ArrangeRng,
    selected: &mut Vec<usize>,
) {
    if limit == 0 || candidates.is_empty() {
        return;
    }
    candidates.sort_by(|left, right| right.gap.cmp(&left.gap).then(left.lane.cmp(&right.lane)));
    let mut remaining = limit.min(candidates.len());
    while remaining > 0 {
        let gap = candidates[0].gap;
        let equal_count = candidates.iter().take_while(|candidate| candidate.gap == gap).count();
        if equal_count <= remaining {
            selected.extend(candidates.drain(..equal_count).map(|candidate| candidate.lane));
            remaining -= equal_count;
        } else {
            let mut available = equal_count;
            for _ in 0..remaining {
                let index = rng.next_usize(available);
                selected.push(candidates.remove(index).lane);
                available -= 1;
            }
            remaining = 0;
        }
    }
}

fn shuffle_lm_lanes(lanes: &mut [usize], rng: &mut ArrangeRng) {
    for index in (1..lanes.len()).rev() {
        let other = rng.next_usize(index + 1);
        lanes.swap(index, other);
    }
}
