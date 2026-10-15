use super::*;

/// Structural lookups only; visibility, clocks and callbacks are evaluated live.
/// Keep every object category: a matching object may yield no item and fall
/// through to another category with the same ID.
#[derive(Debug, Default)]
pub(in crate::skin) struct DestinationObjectIndices {
    pub(in crate::skin) lr2_charts: Option<usize>,
    pub(in crate::skin) judge: Option<usize>,
    pub(in crate::skin) panel: Option<usize>,
    pub(in crate::skin) hiterror_visualizer: Option<usize>,
    pub(in crate::skin) timingvisualizer: Option<usize>,
    pub(in crate::skin) timingdistributiongraph: Option<usize>,
    pub(in crate::skin) gaugegraph: Option<usize>,
    pub(in crate::skin) judgegraph: Option<usize>,
    pub(in crate::skin) bpmgraph: Option<usize>,
    pub(in crate::skin) pmchara: Option<usize>,
    pub(in crate::skin) graph: Option<usize>,
    pub(in crate::skin) text: Option<usize>,
    pub(in crate::skin) slider: Option<usize>,
    pub(in crate::skin) lift_cover: Option<usize>,
    pub(in crate::skin) hidden_cover: Option<usize>,
    pub(in crate::skin) imageset: Option<usize>,
    pub(in crate::skin) search_text: Option<usize>,
}

impl DestinationObjectIndices {
    pub(in crate::skin) fn new(document: &SkinDocument, destination: &SkinDestinationDef) -> Self {
        Self {
            lr2_charts: document.lr2_charts.iter().position(|item| item.id == destination.id),
            judge: document.judge.iter().position(|item| item.id == destination.id),
            panel: document.panel.iter().position(|item| item.id == destination.id),
            hiterror_visualizer: document
                .hiterror_visualizer
                .iter()
                .position(|item| item.id == destination.id),
            timingvisualizer: document
                .timingvisualizer
                .iter()
                .position(|item| item.id == destination.id),
            timingdistributiongraph: document
                .timingdistributiongraph
                .iter()
                .position(|item| item.id == destination.id),
            gaugegraph: document.gaugegraph.iter().position(|item| item.id == destination.id),
            judgegraph: document.judgegraph.iter().position(|item| item.id == destination.id),
            bpmgraph: document.bpmgraph.iter().position(|item| item.id == destination.id),
            pmchara: document.pmchara.iter().position(|item| item.id == destination.id),
            graph: document.graph.iter().position(|item| item.id == destination.id),
            text: document.text.iter().position(|item| item.id == destination.id),
            slider: document.slider.iter().position(|item| item.id == destination.id),
            lift_cover: document.lift_cover.iter().position(|item| item.id == destination.id),
            hidden_cover: document.hidden_cover.iter().position(|item| item.id == destination.id),
            imageset: document.imageset.iter().position(|item| item.id == destination.id),
            search_text: document
                .text
                .iter()
                .position(|item| item.ref_id == 30 && item.id == destination.id),
        }
    }
}

/// Some(None) is a cached absence; do not fall back to a linear search.
#[inline]
pub(in crate::skin) fn indexed_destination_object<T>(
    items: &[T],
    index: Option<Option<usize>>,
    matches: impl Fn(&T) -> bool,
) -> Option<&T> {
    match index {
        Some(index) => index.and_then(|index| items.get(index)),
        None => items.iter().find(|item| matches(item)),
    }
}
