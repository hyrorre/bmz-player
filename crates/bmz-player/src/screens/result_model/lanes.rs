use bmz_chart::model::PlayableChart;
use bmz_core::clear::ClearType;
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::scored_notes_by_lane;

use crate::config::profile_config::KeyModeConversionConfig;
use crate::screens::play_session::{
    AppliedArrange, flipped_dp_lane, seven_to_nine_replay_lane_projection,
};
use crate::select_options::{ArrangeOption, DoubleOption};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultLaneSource {
    Fixed(Lane),
    PerNote,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultLaneStats {
    pub notes: u32,
    pub ex_score: u32,
    pub bp: u32,
    pub auto_scratch: bool,
    pub source: ResultLaneSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultLaneDetails {
    pub key_mode: KeyMode,
    pub arrange: [ArrangeOption; 2],
    pub lanes: [ResultLaneStats; LANE_COUNT],
}

impl ResultLaneDetails {
    pub(crate) fn new(
        chart: &PlayableChart,
        result: &PlayResult,
        arrange: &AppliedArrange,
        key_mode: KeyMode,
        auto_scratch_lanes: &[bool; LANE_COUNT],
    ) -> Self {
        let counts = scored_notes_by_lane(chart);
        let projection = (arrange.key_mode_conversion == KeyModeConversionConfig::SevenToNine)
            .then(|| {
                seven_to_nine_replay_lane_projection(
                    arrange.seven_to_nine_pattern,
                    arrange.seven_to_nine_type,
                )
                .record_to_source
            });
        Self {
            key_mode,
            arrange: [arrange.arrange, arrange.arrange_2p],
            lanes: Lane::ALL.map(|lane| {
                let index = lane.index();
                let score = result.score.lane_score(lane);
                let notes = counts[index];
                let unprocessed = if result.clear_type == ClearType::Failed {
                    notes.saturating_sub(score.past_notes)
                } else {
                    0
                };
                let source = match &projection {
                    Some(projection) => projection[index]
                        .map_or(ResultLaneSource::Unknown, |lane| {
                            lane_source(arrange, lane, KeyMode::K7)
                        }),
                    None => lane_source(arrange, lane, key_mode),
                };
                ResultLaneStats {
                    notes,
                    ex_score: score.ex_score,
                    bp: score.bp.saturating_add(unprocessed),
                    auto_scratch: auto_scratch_lanes[index],
                    source,
                }
            }),
        }
    }
}

fn lane_source(applied: &AppliedArrange, lane: Lane, key_mode: KeyMode) -> ResultLaneSource {
    let is_double = matches!(key_mode, KeyMode::K10 | KeyMode::K14);
    let arrange = if is_double && lane.index() >= Lane::Key8.index() {
        applied.arrange_2p
    } else {
        applied.arrange
    };
    if matches!(
        applied.key_mode_conversion,
        KeyModeConversionConfig::SevenToSix | KeyModeConversionConfig::SevenToEight
    ) {
        return ResultLaneSource::PerNote;
    }
    let scratch = matches!(lane, Lane::Scratch | Lane::Scratch2);
    if (applied.pattern.is_none() || applied.pattern_is_partial)
        && match arrange {
            ArrangeOption::SRandom | ArrangeOption::Spiral | ArrangeOption::HRandom => !scratch,
            ArrangeOption::AllScratch | ArrangeOption::SRandomEx => true,
            _ => false,
        }
    {
        return ResultLaneSource::PerNote;
    }
    let original = if let Some(pattern) = &applied.pattern {
        pattern.get(lane.index()).and_then(|&source| Lane::ALL.get(source as usize)).copied()
    } else if scratch || arrange == ArrangeOption::Normal {
        // With no fixed permutation, these lanes keep their placement.
        Some(lane)
    } else {
        None
    };
    original.map_or(ResultLaneSource::Unknown, |source| {
        ResultLaneSource::Fixed(if is_double && applied.double_option == DoubleOption::Flip {
            flipped_dp_lane(source)
        } else {
            source
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_flip_looks_up_fixed_sources_and_marks_only_the_shuffled_side() {
        let mut pattern: Vec<u8> = (0..LANE_COUNT as u8).collect();
        pattern.swap(Lane::Key1.index(), Lane::Key4.index());
        let applied = AppliedArrange {
            arrange: ArrangeOption::Random,
            arrange_2p: ArrangeOption::SRandom,
            double_option: DoubleOption::Flip,
            pattern: Some(pattern),
            pattern_is_partial: true,
            ..Default::default()
        };
        assert_eq!(
            lane_source(&applied, Lane::Key1, KeyMode::K14),
            ResultLaneSource::Fixed(Lane::Key11)
        );
        assert_eq!(lane_source(&applied, Lane::Key8, KeyMode::K14), ResultLaneSource::PerNote);
        assert_eq!(
            lane_source(&applied, Lane::Scratch2, KeyMode::K14),
            ResultLaneSource::Fixed(Lane::Scratch)
        );
    }
}
