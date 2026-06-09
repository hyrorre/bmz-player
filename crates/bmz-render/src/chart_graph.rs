use bmz_chart::model::{PlayableChart, TimingEventKind};
use bmz_core::lane::KeyMode;

#[derive(Debug, Clone, PartialEq)]
pub struct BpmGraphSegment {
    pub start_ratio: f32,
    pub end_ratio: f32,
    pub bpm: f32,
    pub is_stop: bool,
}

pub fn build_judge_graph_density(chart: &PlayableChart) -> Vec<u8> {
    let seconds = (chart.end_time.0 / 1_000_000).max(0) as usize + 1;
    let mut density = vec![0u8; seconds.max(1)];
    for lane in &chart.lane_notes {
        for note in lane {
            if matches!(
                note.kind,
                bmz_chart::model::NoteKind::Invisible | bmz_chart::model::NoteKind::Mine
            ) {
                continue;
            }
            let sec = (note.time.0 / 1_000_000).max(0) as usize;
            if let Some(slot) = density.get_mut(sec) {
                *slot = slot.saturating_add(1);
            }
        }
    }
    density
}

pub fn build_bpm_graph_segments(chart: &PlayableChart) -> Vec<BpmGraphSegment> {
    let duration_us = chart.end_time.0.max(1) as f32;
    let mut segments = Vec::new();
    let mut current_bpm = chart.metadata.initial_bpm as f32;
    let mut segment_start_us = 0_i64;
    for event in &chart.timing_events {
        let event_us = event.time.0;
        if event_us <= segment_start_us {
            // セグメント開始以前（または同時刻）のイベントは BPM 更新のみ行い、
            // STOP 区間内のイベントも BPM を先読みする。
            if let TimingEventKind::BpmChange { bpm } = event.kind {
                current_bpm = bpm as f32;
            }
            continue;
        }
        if let TimingEventKind::Stop { duration_us: stop_dur } = event.kind {
            // STOP 直前の区間を現在 BPM のセグメントとして追加。
            segments.push(BpmGraphSegment {
                start_ratio: segment_start_us as f32 / duration_us,
                end_ratio: event_us as f32 / duration_us,
                bpm: current_bpm,
                is_stop: false,
            });
            // STOP 区間 [event_us, event_us + stop_dur] を追加。
            let stop_end_us = event_us + stop_dur;
            segments.push(BpmGraphSegment {
                start_ratio: event_us as f32 / duration_us,
                end_ratio: (stop_end_us as f32 / duration_us).min(1.0),
                bpm: 0.0,
                is_stop: true,
            });
            // 次のセグメントは STOP 終了時点から。
            segment_start_us = stop_end_us;
        } else {
            segments.push(BpmGraphSegment {
                start_ratio: segment_start_us as f32 / duration_us,
                end_ratio: event_us as f32 / duration_us,
                bpm: current_bpm,
                is_stop: false,
            });
            if let TimingEventKind::BpmChange { bpm } = event.kind {
                current_bpm = bpm as f32;
            }
            segment_start_us = event_us;
        }
    }
    if segment_start_us < chart.end_time.0 {
        segments.push(BpmGraphSegment {
            start_ratio: segment_start_us as f32 / duration_us,
            end_ratio: 1.0,
            bpm: current_bpm,
            is_stop: false,
        });
    }
    segments
}

pub fn compute_adjusted_cover_progress(
    hidden_enabled: bool,
    lane_cover: f32,
    lift: f32,
    hsfix_index: i32,
    now_bpm: f32,
    max_bpm: f32,
    main_bpm: f32,
) -> Option<f32> {
    if !hidden_enabled {
        return None;
    }
    let visible = (1.0 - (lane_cover + lift)).clamp(0.0, 1.0);
    match hsfix_index {
        2 if max_bpm > 0.0 => Some((visible * (1.0 - now_bpm / max_bpm)).clamp(0.0, 1.0)),
        3 if main_bpm > 0.0 => {
            Some((visible * (1.0 - (now_bpm / main_bpm).min(1.0))).clamp(0.0, 1.0))
        }
        _ => None,
    }
}

pub fn compute_adjusted_rate(
    hidden_enabled: bool,
    lanecover_enabled: bool,
    hsfix_index: i32,
    now_bpm: f32,
    max_bpm: f32,
    main_bpm: f32,
) -> Option<f32> {
    if !hidden_enabled && !lanecover_enabled {
        return None;
    }
    match hsfix_index {
        2 if max_bpm > 0.0 => Some((now_bpm / max_bpm).clamp(0.0, 1.0)),
        3 if main_bpm > 0.0 => Some((now_bpm / main_bpm).min(1.0).clamp(0.0, 1.0)),
        _ => None,
    }
}

pub fn rm_skin_fs_threshold_ms(judge_rank: Option<i32>, key_mode: KeyMode) -> i32 {
    let is_5_or_7 = matches!(key_mode, KeyMode::K5 | KeyMode::K7);
    if is_5_or_7 {
        match judge_rank {
            Some(183) => 20,
            Some(182) => 15,
            Some(181) => 10,
            Some(180) => 5,
            _ => 25,
        }
    } else {
        20
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmz_chart::model::{ChartMetadata, NoteEvent, TimingEvent};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    fn empty_chart() -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata {
                initial_bpm: 120.0,
                key_mode: KeyMode::K7,
                ..ChartMetadata::default()
            },
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
            bga_asset_by_bmp_key: Default::default(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(3_000_000),
        }
    }

    #[test]
    fn build_judge_graph_density_counts_notes_per_second() {
        let mut chart = empty_chart();
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
            damage: None,
        });
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Tap,
            tick: ChartTick(192),
            time: TimeUs(1_000_000),
            sound: None,
            damage: None,
        });
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(3),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Tap,
            tick: ChartTick(384),
            time: TimeUs(1_000_000),
            sound: None,
            damage: None,
        });
        let density = build_judge_graph_density(&chart);
        assert_eq!(density[0], 1);
        assert_eq!(density[1], 2);
    }

    #[test]
    fn build_judge_graph_density_excludes_invisible_and_mine_notes() {
        let mut chart = empty_chart();
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: None,
            damage: None,
        });
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(2),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Invisible,
            tick: ChartTick(96),
            time: TimeUs(0),
            sound: None,
            damage: None,
        });
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(3),
            lane: Lane::Key1,
            kind: bmz_chart::model::NoteKind::Mine,
            tick: ChartTick(192),
            time: TimeUs(0),
            sound: None,
            damage: Some(200),
        });
        let density = build_judge_graph_density(&chart);
        assert_eq!(density[0], 1);
    }

    #[test]
    fn compute_adjusted_rate_uses_hsfix_main_mode() {
        let rate = compute_adjusted_rate(true, false, 3, 90.0, 180.0, 120.0).unwrap();
        assert!((rate - 0.75).abs() < 1e-5);
    }

    #[test]
    fn rm_skin_fs_threshold_ms_maps_judge_rank_options() {
        assert_eq!(rm_skin_fs_threshold_ms(Some(180), KeyMode::K7), 5);
        assert_eq!(rm_skin_fs_threshold_ms(None, KeyMode::K10), 20);
    }

    #[test]
    fn build_bpm_graph_segments_emits_stop_segment() {
        let mut chart = empty_chart();
        chart.timing_events.push(TimingEvent {
            tick: ChartTick(192),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::Stop { duration_us: 500_000 },
        });
        let segments = build_bpm_graph_segments(&chart);
        assert!(segments.iter().any(|segment| segment.is_stop));
    }

    /// STOP セグメントは「STOP 直前」ではなく「STOP 区間 [stop_time, stop_time+dur]」でなければならない。
    /// 修正前は [0, stop_time] が is_stop=true になるバグがあった。
    #[test]
    fn build_bpm_graph_segments_stop_covers_correct_interval() {
        // end_time = 3s、STOP は 1s から 0.5s 間
        let mut chart = empty_chart(); // end_time = 3_000_000 us
        chart.timing_events.push(TimingEvent {
            tick: ChartTick(192),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::Stop { duration_us: 500_000 },
        });
        let segments = build_bpm_graph_segments(&chart);
        // pre-stop: [0, 1s) bpm=120 is_stop=false
        // stop:     [1s, 1.5s) bpm=0 is_stop=true
        // post-stop: [1.5s, 3s] bpm=120 is_stop=false
        let stop_seg = segments.iter().find(|s| s.is_stop).expect("stop segment");
        let duration_us = 3_000_000_f32;
        assert!(
            (stop_seg.start_ratio - 1_000_000.0 / duration_us).abs() < 1e-4,
            "stop start_ratio should be at stop event time"
        );
        assert!(
            (stop_seg.end_ratio - 1_500_000.0 / duration_us).abs() < 1e-4,
            "stop end_ratio should be at stop_time + duration"
        );
        // STOP 直前のセグメントは is_stop=false でなければならない。
        let pre_stop = segments.iter().find(|s| !s.is_stop && s.end_ratio <= stop_seg.start_ratio + 1e-4);
        assert!(pre_stop.is_some_and(|s| !s.is_stop), "pre-stop segment must not be a stop");
    }
}
