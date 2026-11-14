use super::*;
use bmz_chart::model::LongNotePair;
use bmz_chart::timing::{TickTimePoint, TimingMap};
use std::sync::Arc;

/// Own the immutable chart so pointer identity cannot be recycled. A different
/// chart (including Arc::make_mut edits) safely uses the uncached path.
#[derive(Debug)]
pub(super) struct ChartIndex {
    chart: Arc<PlayableChart>,
    notes: HashMap<NoteId, (usize, usize)>,
    hcn: Vec<usize>,
    hcn_prefix_end: Vec<TimeUs>,
    hcn_sorted: bool,
    result_end_time: TimeUs,
    bpm_reflections: Vec<TickTimePoint>,
}

impl JudgeEngine {
    /// Build during loading, after transforms and before timed input.
    pub fn with_chart_index(mut self, chart: Arc<PlayableChart>) -> Self {
        let count = chart.lane_notes.iter().map(Vec::len).sum();
        let mut notes = HashMap::with_capacity(count);
        for (lane, events) in chart.lane_notes.iter().enumerate() {
            for (offset, note) in events.iter().enumerate() {
                notes.entry(note.id).or_insert((lane, offset));
            }
        }
        let hcn: Vec<_> = chart
            .long_notes
            .iter()
            .enumerate()
            .filter_map(|(i, pair)| {
                (pair.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hcn)
                    .then_some(i)
            })
            .collect();
        let hcn_sorted = hcn.windows(2).all(|pair| {
            chart.long_notes[pair[0]].start_time <= chart.long_notes[pair[1]].start_time
        });
        let mut end = TimeUs(i64::MIN);
        let hcn_prefix_end = hcn
            .iter()
            .map(|&i| {
                end = end.max(chart.long_notes[i].end_time);
                end
            })
            .collect();
        self.judged_notes.reserve(count.saturating_sub(self.judged_notes.len()));
        let result_end_time = result_end_time(&chart);
        let bpm_reflections = chart_reflections(&chart);
        self.chart_index = Some(Arc::new(ChartIndex {
            chart,
            notes,
            hcn,
            hcn_prefix_end,
            hcn_sorted,
            result_end_time,
            bpm_reflections,
        }));
        self
    }

    fn matching_index(&self, chart: &PlayableChart) -> Option<&ChartIndex> {
        self.chart_index.as_deref().filter(|index| std::ptr::eq(index.chart.as_ref(), chart))
    }

    /// Last event that can affect scoring or gauges. Invisible notes only change keysounds.
    /// Keep the presentation end separately, and retain its fallback for unscored charts.
    pub(crate) fn result_end_time(&self, chart: &PlayableChart) -> TimeUs {
        self.matching_index(chart)
            .map_or_else(|| result_end_time(chart), |index| index.result_end_time)
    }

    pub(crate) fn bpm_reflections<'a>(
        &'a self,
        chart: &PlayableChart,
    ) -> std::borrow::Cow<'a, [TickTimePoint]> {
        match self.matching_index(chart) {
            Some(index) => std::borrow::Cow::Borrowed(&index.bpm_reflections),
            None => std::borrow::Cow::Owned(chart_reflections(chart)),
        }
    }

    pub fn note_by_id<'a>(&self, chart: &'a PlayableChart, id: NoteId) -> Option<&'a NoteEvent> {
        if let Some(index) = self.matching_index(chart) {
            let &(lane, offset) = index.notes.get(&id)?;
            Some(&chart.lane_notes[lane][offset])
        } else {
            chart.note_by_id(id)
        }
    }

    pub(crate) fn hcn_pairs_at<'a>(
        &'a self,
        chart: &'a PlayableChart,
        now: TimeUs,
    ) -> impl Iterator<Item = &'a LongNotePair> + 'a {
        let (indices, range) = match self.matching_index(chart) {
            Some(index) if index.hcn_sorted => {
                let start = index.hcn_prefix_end.partition_point(|&end| end <= now);
                let end = index.hcn.partition_point(|&i| chart.long_notes[i].start_time <= now);
                (Some(index.hcn.as_slice()), start.min(end)..end)
            }
            Some(index) => (Some(index.hcn.as_slice()), 0..index.hcn.len()),
            None => (None, 0..chart.long_notes.len()),
        };
        range.map(move |i| &chart.long_notes[indices.map_or(i, |indices| indices[i])])
    }
}

fn chart_reflections(chart: &PlayableChart) -> Vec<TickTimePoint> {
    if chart.timing_events.iter().any(|event| event.kind.is_bpm_reflection()) {
        TimingMap::from_chart_timing_events(chart.metadata.initial_bpm, &chart.timing_events)
            .bpm_reflections()
            .to_vec()
    } else {
        Vec::new()
    }
}

fn result_end_time(chart: &PlayableChart) -> TimeUs {
    chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|note| note.kind != NoteKind::Invisible)
        .map(|note| note.time)
        // Missed/early-released HCNs can keep changing the gauge until their tail.
        .chain(chart.long_notes.iter().map(|pair| pair.end_time))
        .max()
        .unwrap_or(chart.end_time)
}
