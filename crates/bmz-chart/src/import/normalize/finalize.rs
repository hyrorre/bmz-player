use super::*;
use crate::model::LongNoteMode;
use bmz_core::time::ChartTick;

fn compress_import_tick(tick: ChartTick) -> ChartTick {
    ChartTick(tick.0 / crate::timing::IMPORT_TICK_SCALE)
}

pub(super) fn compress_import_ticks(draft: &mut PlayableChartDraft) {
    for zone in &mut draft.metadata.free_zones {
        zone.start_tick = compress_import_tick(zone.start_tick);
        zone.end_tick = compress_import_tick(zone.end_tick);
    }
    for notes in &mut draft.lane_notes {
        for note in notes {
            note.tick = compress_import_tick(note.tick);
        }
    }
    for pair in &mut draft.long_notes {
        pair.start_tick = compress_import_tick(pair.start_tick);
        pair.end_tick = compress_import_tick(pair.end_tick);
    }
    for event in &mut draft.bgm_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.bga_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.timing_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.scroll_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.speed_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.judge_rank_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.bgm_volume_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.key_volume_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.text_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.bga_opacity_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.bga_argb_events {
        event.tick = compress_import_tick(event.tick);
    }
    for event in &mut draft.bga_keybound_events {
        event.tick = compress_import_tick(event.tick);
    }
    for line in &mut draft.bar_lines {
        line.tick = compress_import_tick(line.tick);
    }
}

pub(super) fn finalize_playable_chart(mut draft: PlayableChartDraft) -> PlayableChart {
    for lane_notes in &mut draft.lane_notes {
        lane_notes.sort_by_key(|note| note.time);
    }
    draft.long_notes.sort_by_key(|pair| pair.start_time);
    draft.bgm_events.sort_by_key(|event| event.time);
    draft.bga_events.sort_by_key(|event| event.time);
    draft.timing_events.sort_by_key(|event| event.time);
    draft.scroll_events.sort_by_key(|event| event.time);
    draft.speed_events.sort_by_key(|event| event.time);
    draft.judge_rank_events.sort_by_key(|event| event.time);
    draft.bgm_volume_events.sort_by_key(|event| event.time);
    draft.key_volume_events.sort_by_key(|event| event.time);
    draft.text_events.sort_by_key(|event| event.time);
    draft.bga_opacity_events.sort_by_key(|event| event.time);
    draft.bga_argb_events.sort_by_key(|event| event.time);
    draft.bga_keybound_events.sort_by_key(|event| event.time);
    draft.bar_lines.sort_by_key(|line| line.time);

    let inner: std::collections::HashSet<_> = draft
        .metadata
        .free_zones
        .iter()
        .flat_map(|zone| zone.inner_notes.iter().copied())
        .collect();
    draft.total_notes = draft
        .lane_notes
        .iter()
        .flatten()
        .filter(|n| n.kind.is_scored_head() && !inner.contains(&n.id))
        .count() as u32;
    let beatoraja_total_notes = beatoraja_total_note_count(
        draft.total_notes,
        draft
            .long_notes
            .iter()
            .filter(|pair| {
                !inner.contains(&pair.start_note_id)
                    && draft.lane_notes[pair.lane.index()]
                        .iter()
                        .any(|n| n.id == pair.start_note_id && !n.kind.is_dummy())
            })
            .map(|pair| pair.mode),
        draft.metadata.long_note_mode,
    );
    if draft.metadata.difficulty_name.trim().is_empty()
        || draft.metadata.difficulty_name.trim() == "0"
    {
        draft.metadata.difficulty_name = infer_beatoraja_difficulty_name(
            &draft.metadata.title,
            &draft.metadata.subtitle,
            beatoraja_total_notes,
        )
        .to_string();
    }
    if draft.total_is_bmson_percent {
        draft.metadata.total =
            Some(bmson_total_on_bms_scale(draft.metadata.total, beatoraja_total_notes));
    }
    draft.end_time = compute_end_time(&draft);

    PlayableChart {
        identity: draft.identity,
        metadata: draft.metadata,
        lane_notes: draft.lane_notes,
        long_notes: draft.long_notes,
        bgm_events: draft.bgm_events,
        bga_events: draft.bga_events,
        timing_events: draft.timing_events,
        scroll_events: draft.scroll_events,
        speed_events: draft.speed_events,
        judge_rank_events: draft.judge_rank_events,
        bgm_volume_events: draft.bgm_volume_events,
        key_volume_events: draft.key_volume_events,
        text_events: draft.text_events,
        bga_opacity_events: draft.bga_opacity_events,
        bga_argb_events: draft.bga_argb_events,
        swbga_definitions: draft.swbga_definitions,
        bga_keybound_events: draft.bga_keybound_events,
        bga_asset_by_bmp_key: draft.bga_asset_by_bmp_key,
        bar_lines: draft.bar_lines,
        sounds: draft.sounds,
        bga_assets: draft.bga_assets,
        total_notes: draft.total_notes,
        end_time: draft.end_time,
    }
}

fn infer_beatoraja_difficulty_name(title: &str, subtitle: &str, total_notes: u32) -> &'static str {
    let subtitle = subtitle.to_lowercase();
    if let Some(difficulty) = difficulty_name_in_text(&subtitle) {
        return difficulty;
    }

    let full_title = format!("{title}{subtitle}").to_lowercase();
    if let Some(difficulty) = difficulty_name_in_text(&full_title) {
        return difficulty;
    }

    match total_notes {
        0..250 => "BEGINNER",
        250..600 => "NORMAL",
        600..1000 => "HYPER",
        1000..2000 => "ANOTHER",
        _ => "INSANE",
    }
}

fn beatoraja_total_note_count(
    total_notes: u32,
    long_note_modes: impl IntoIterator<Item = Option<LongNoteMode>>,
    default_mode: LongNoteMode,
) -> u32 {
    long_note_modes.into_iter().fold(total_notes, |count, mode| {
        count.saturating_add(u32::from(matches!(
            mode.unwrap_or(default_mode),
            LongNoteMode::Cn | LongNoteMode::Hcn
        )))
    })
}

fn beatoraja_default_total(total_notes: u32) -> f64 {
    let notes = f64::from(total_notes);
    260.0_f64.max(7.605 * notes / (0.01 * notes + 6.5))
}

fn bmson_total_on_bms_scale(raw_total: Option<f64>, total_notes: u32) -> f64 {
    let default_total = beatoraja_default_total(total_notes);
    raw_total
        .filter(|total| *total > 0.0)
        .map_or(default_total, |total| total / 100.0 * default_total)
}

fn difficulty_name_in_text(text: &str) -> Option<&'static str> {
    if text.contains("beginner") {
        Some("BEGINNER")
    } else if text.contains("normal") {
        Some("NORMAL")
    } else if text.contains("hyper") {
        Some("HYPER")
    } else if text.contains("another") {
        Some("ANOTHER")
    } else if text.contains("insane") || text.contains("leggendaria") {
        Some("INSANE")
    } else {
        None
    }
}

pub(super) fn compute_end_time(draft: &PlayableChartDraft) -> TimeUs {
    let lane_end = draft
        .lane_notes
        .iter()
        .flat_map(|notes| notes.iter().map(|note| note.time.0))
        .max()
        .unwrap_or(0);
    let long_end = draft.long_notes.iter().map(|pair| pair.end_time.0).max().unwrap_or(0);
    let playable_end = lane_end.max(long_end);
    if playable_end > 0 {
        return TimeUs(playable_end);
    }

    let bgm_end = draft.bgm_events.iter().map(|event| event.time.0).max().unwrap_or(0);
    TimeUs(bgm_end)
}

impl PlayableChartDraft {
    pub(super) fn new(
        identity: bmz_core::chart::ChartIdentity,
        metadata: ChartMetadata,
        sounds: Vec<SoundAssetRef>,
        bga_assets: Vec<BgaAssetRef>,
    ) -> Self {
        Self {
            identity,
            metadata,
            total_is_bmson_percent: false,
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),
            scroll_events: Vec::new(),
            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: HashMap::new(),
            bar_lines: Vec::new(),
            sounds,
            bga_assets,
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }
}

#[cfg(test)]
mod difficulty_tests {
    use super::{
        beatoraja_total_note_count, bmson_total_on_bms_scale, infer_beatoraja_difficulty_name,
    };
    use crate::model::LongNoteMode;

    #[test]
    fn infers_difficulty_with_beatoraja_text_priority() {
        assert_eq!(infer_beatoraja_difficulty_name("Song Another", "", 1), "ANOTHER");
        assert_eq!(infer_beatoraja_difficulty_name("Song Another", "Hyper", 1), "HYPER");
        assert_eq!(infer_beatoraja_difficulty_name("Song", "LEGGENDARIA", 1), "INSANE");
    }

    #[test]
    fn infers_stronger_difficulties_from_playable_note_counts() {
        let title = "STRONGER (あたしは、もっと強く)";
        for (base_notes, ln, cn, hcn, expected_notes, expected_name) in [
            (299, 4, 8, 16, 323, "NORMAL"),
            (590, 0, 54, 16, 660, "HYPER"),
            (1_238, 0, 54, 16, 1_308, "ANOTHER"),
            (1_593, 0, 53, 28, 1_674, "ANOTHER"),
        ] {
            let modes = std::iter::repeat_n(Some(LongNoteMode::Ln), ln)
                .chain(std::iter::repeat_n(Some(LongNoteMode::Cn), cn))
                .chain(std::iter::repeat_n(Some(LongNoteMode::Hcn), hcn));
            let difficulty_notes = beatoraja_total_note_count(base_notes, modes, LongNoteMode::Cn);
            assert_eq!(difficulty_notes, expected_notes);
            assert_eq!(infer_beatoraja_difficulty_name(title, "", difficulty_notes), expected_name);
        }
    }

    #[test]
    fn counts_cn_and_hcn_ends_for_beatoraja_difficulty() {
        let modes = [Some(LongNoteMode::Ln), Some(LongNoteMode::Cn), Some(LongNoteMode::Hcn), None];
        assert_eq!(beatoraja_total_note_count(590, modes, LongNoteMode::Cn), 593);
        assert_eq!(beatoraja_total_note_count(590, modes, LongNoteMode::Ln), 592);
    }

    #[test]
    fn converts_bmson_total_percent_to_beatoraja_bms_scale() {
        assert_eq!(bmson_total_on_bms_scale(Some(100.0), 100), 260.0);
        assert_eq!(bmson_total_on_bms_scale(Some(200.0), 100), 520.0);
        assert_eq!(bmson_total_on_bms_scale(Some(0.0), 100), 260.0);

        let expected = 7.605 * 1_000.0 / (10.0 + 6.5);
        assert!((bmson_total_on_bms_scale(Some(100.0), 1_000) - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn infers_difficulty_at_beatoraja_note_count_boundaries() {
        for (total_notes, expected) in [
            (249, "BEGINNER"),
            (250, "NORMAL"),
            (599, "NORMAL"),
            (600, "HYPER"),
            (999, "HYPER"),
            (1_000, "ANOTHER"),
            (1_999, "ANOTHER"),
            (2_000, "INSANE"),
        ] {
            assert_eq!(infer_beatoraja_difficulty_name("Song", "", total_notes), expected);
        }
    }
}
