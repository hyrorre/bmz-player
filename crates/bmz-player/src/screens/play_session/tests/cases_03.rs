use super::*;

#[test]
fn random_trainer_seed_only_overrides_fresh_7k_random() {
    let trainer_seed = Some(322);
    let normal_seed = Some(42);
    let recorded_pattern = [0, 7, 6, 5, 4, 3, 2, 1];

    assert_eq!(
        effective_arrange_seed(KeyMode::K7, ArrangeOption::Random, normal_seed, trainer_seed, None,),
        trainer_seed
    );
    assert_eq!(
        effective_arrange_seed(KeyMode::K5, ArrangeOption::Random, normal_seed, trainer_seed, None,),
        normal_seed
    );
    assert_eq!(
        effective_arrange_seed(KeyMode::K7, ArrangeOption::Mirror, normal_seed, trainer_seed, None,),
        normal_seed
    );
    assert_eq!(
        effective_arrange_seed(
            KeyMode::K7,
            ArrangeOption::Random,
            normal_seed,
            trainer_seed,
            Some(&recorded_pattern),
        ),
        normal_seed,
        "a replay or same-arrange retry pattern must take priority"
    );
}

#[test]
fn random_trainer_compatible_seed_applies_requested_7k_order() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    let seed = crate::random_trainer::seed_for_lane_order([2, 1, 4, 3, 6, 5, 7])
        .expect("known lane order must resolve");

    let applied =
        apply_arrange(&mut chart, ArrangeOption::Random, Some(i64::from(seed.value())), None);
    let pattern = applied.pattern.expect("RANDOM must record its lane permutation");

    assert_eq!(applied.seed, Some(322));
    assert_eq!(&pattern[..8], &[0, 2, 1, 4, 3, 6, 5, 7]);
    assert_eq!(pattern[Lane::Scratch.index()], Lane::Scratch.index() as u8);
    assert_eq!(
        &pattern[Lane::Key8.index()..],
        &(Lane::Key8.index() as u8..LANE_COUNT as u8).collect::<Vec<_>>()
    );
}

#[test]
fn apply_arrange_random_moves_notes_between_lanes() {
    use bmz_chart::model::{NoteEvent, NoteKind};
    use bmz_core::time::ChartTick;

    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: ChartTick(0),
        time: TimeUs(1_000_000),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });

    let applied = apply_arrange(&mut chart, ArrangeOption::Random, Some(42), None);

    assert_eq!(applied.arrange, ArrangeOption::Random);
    assert_ne!(applied.pattern, Some((0u8..LANE_COUNT as u8).collect()));
    assert!(chart.lane_notes[Lane::Key1.index()].is_empty());
    assert!(
        chart.lane_notes.iter().enumerate().any(|(lane_index, notes)| lane_index
            != Lane::Key1.index()
            && notes.iter().any(|note| note.id == NoteId(1) && note.lane.index() == lane_index))
    );
}

#[test]
fn rotate_random_uses_non_identity_lane_rotation() {
    let perm = rotate_lane_permutation(7, KeyMode::K7, false, false);
    let key_lanes: Vec<usize> = (Lane::Key1.index()..=Lane::Key7.index()).collect();
    let mapped: HashSet<_> = key_lanes.iter().map(|&lane| perm[lane]).collect();

    assert_eq!(mapped, key_lanes.iter().copied().collect());
    assert!(key_lanes.iter().any(|&lane| perm[lane] != lane));
    assert_eq!(perm[Lane::Scratch.index()], Lane::Scratch.index());
}

#[test]
fn random_ex_includes_scratch_lane() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    chart.lane_notes[Lane::Scratch.index()].push(note(1, Lane::Scratch, 1_000_000));

    let applied = apply_arrange(&mut chart, ArrangeOption::RandomEx, Some(1), None);

    assert_eq!(applied.arrange, ArrangeOption::RandomEx);
    assert!(
        chart.lane_notes.iter().enumerate().any(|(lane_index, notes)| lane_index
            != Lane::Scratch.index()
            && notes.iter().any(|note| note.id == NoteId(1) && note.lane.index() == lane_index))
    );
}

#[test]
fn random2_arranges_only_dp_second_player_lanes() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K14;
    chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
    chart.lane_notes[Lane::Key8.index()].push(note(2, Lane::Key8, 1_000_000));

    let applied = apply_arrange_pair(
        &mut chart,
        ArrangeOption::Normal,
        ArrangeOption::Mirror,
        Some(1),
        Some(2),
        false,
        SRandomScheme::Lm120HzV1,
        None,
        None,
        None,
    );

    assert_eq!(applied.arrange, ArrangeOption::Normal);
    assert_eq!(applied.seed, Some(1));
    assert_eq!(applied.seed_2p, Some(2));
    assert_eq!(applied.packed_beatoraja_seed(KeyMode::K14), Some(1 + (2 << 24)));
    assert_eq!(chart.lane_notes[Lane::Key1.index()][0].id, NoteId(1));
    assert!(chart.lane_notes[Lane::Key8.index()].is_empty());
    assert!(
        chart.lane_notes[Lane::Key14.index()]
            .iter()
            .any(|note| note.id == NoteId(2) && note.lane == Lane::Key14)
    );
}

#[test]
fn recorded_sp_pattern_does_not_gain_a_second_player_seed() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    let pattern: Vec<u8> = (0..LANE_COUNT as u8).collect();

    let applied = apply_arrange_pair(
        &mut chart,
        ArrangeOption::Random,
        ArrangeOption::Normal,
        Some(1),
        None,
        false,
        SRandomScheme::Lm120HzV1,
        None,
        None,
        Some(&pattern),
    );

    assert_eq!(applied.seed, Some(1));
    assert_eq!(applied.seed_2p, None);
    assert_eq!(applied.packed_beatoraja_seed(KeyMode::K7), Some(1));
}

#[test]
fn double_option_flip_swaps_dp_player_lanes() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K14;
    chart.lane_notes[Lane::Scratch.index()].push(note(1, Lane::Scratch, 1_000_000));
    chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, 1_000_000));
    chart.lane_notes[Lane::Scratch2.index()].push(note(3, Lane::Scratch2, 1_000_000));
    chart.lane_notes[Lane::Key8.index()].push(note(4, Lane::Key8, 1_000_000));

    apply_double_option(&mut chart, DoubleOption::Flip);

    assert!(
        chart.lane_notes[Lane::Scratch2.index()]
            .iter()
            .any(|note| note.id == NoteId(1) && note.lane == Lane::Scratch2)
    );
    assert!(
        chart.lane_notes[Lane::Key8.index()]
            .iter()
            .any(|note| note.id == NoteId(2) && note.lane == Lane::Key8)
    );
    assert!(
        chart.lane_notes[Lane::Scratch.index()]
            .iter()
            .any(|note| note.id == NoteId(3) && note.lane == Lane::Scratch)
    );
    assert!(
        chart.lane_notes[Lane::Key1.index()]
            .iter()
            .any(|note| note.id == NoteId(4) && note.lane == Lane::Key1)
    );
}

#[test]
fn double_option_battle_duplicates_sp_lanes_as_dp() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    chart.total_notes = 2;
    chart.lane_notes[Lane::Scratch.index()].push(note(1, Lane::Scratch, 1_000_000));
    chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, 1_010_000));

    apply_double_option(&mut chart, DoubleOption::Battle);

    assert_eq!(chart.metadata.key_mode, KeyMode::K14);
    assert_eq!(chart.total_notes, 4);
    assert!(
        chart.lane_notes[Lane::Scratch.index()]
            .iter()
            .any(|note| note.id == NoteId(1) && note.lane == Lane::Scratch)
    );
    assert!(
        chart.lane_notes[Lane::Scratch2.index()]
            .iter()
            .any(|note| note.id != NoteId(1) && note.lane == Lane::Scratch2)
    );
    assert!(
        chart.lane_notes[Lane::Key1.index()]
            .iter()
            .any(|note| note.id == NoteId(2) && note.lane == Lane::Key1)
    );
    assert!(
        chart.lane_notes[Lane::Key8.index()]
            .iter()
            .any(|note| note.id != NoteId(2) && note.lane == Lane::Key8)
    );
}

#[test]
fn s_random_is_reproducible_from_seed() {
    let mut first = chart_with_two_notes_same_lane();
    let mut second = chart_with_two_notes_same_lane();

    let first_applied = apply_arrange(&mut first, ArrangeOption::SRandom, Some(99), None);
    let _second_applied = apply_arrange(&mut second, ArrangeOption::SRandom, Some(99), None);

    assert_eq!(first_applied.pattern, None);
    assert_eq!(lanes_for_notes(&first), lanes_for_notes(&second));
}

#[test]
fn s_random_keeps_long_note_end_on_start_lane() {
    use bmz_chart::model::{LongNoteMode, LongNotePair, LongNoteStyle};
    use bmz_core::time::ChartTick;

    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        kind: NoteKind::LongStart,
        tick: ChartTick(0),
        ..note(1, Lane::Key1, 1_000_000)
    });
    chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
        kind: NoteKind::LongEnd,
        tick: ChartTick(48),
        ..note(2, Lane::Key1, 2_000_000)
    });
    chart.long_notes.push(LongNotePair {
        lane: Lane::Key1,
        style: LongNoteStyle::ChannelPair,
        mode: Some(LongNoteMode::Cn),
        start_note_id: NoteId(1),
        end_note_id: NoteId(2),
        start_tick: ChartTick(0),
        end_tick: ChartTick(48),
        start_time: TimeUs(1_000_000),
        end_time: TimeUs(2_000_000),
        sound: None,
    });

    apply_arrange(&mut chart, ArrangeOption::SRandom, Some(5), None);

    let start_lane = chart
        .lane_notes
        .iter()
        .flatten()
        .find(|note| note.id == NoteId(1))
        .map(|note| note.lane)
        .expect("start note");
    let end_lane = chart
        .lane_notes
        .iter()
        .flatten()
        .find(|note| note.id == NoteId(2))
        .map(|note| note.lane)
        .expect("end note");
    assert_eq!(start_lane, end_lane);
    assert_eq!(chart.long_notes[0].lane, start_lane);
}

#[test]
fn legacy_s_random_and_non_target_arranges_match_pre_lm_head_goldens() {
    let make_chart = || {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        for (id, lane, time) in [
            (1, Lane::Key1, 1_000_000),
            (2, Lane::Key1, 1_020_000),
            (3, Lane::Key1, 1_060_000),
            (4, Lane::Key3, 1_060_000),
            (5, Lane::Key2, 1_080_000),
            (6, Lane::Scratch, 1_100_000),
            (7, Lane::Key4, 1_150_000),
            (8, Lane::Key7, 1_170_000),
        ] {
            chart.lane_notes[lane.index()].push(note(id, lane, time));
        }
        chart
    };

    for (arrange, legacy_seed, expected) in [
        (
            ArrangeOption::SRandom,
            false,
            vec![
                Lane::Key3,
                Lane::Key4,
                Lane::Key6,
                Lane::Key7,
                Lane::Key1,
                Lane::Scratch,
                Lane::Key3,
                Lane::Key6,
            ],
        ),
        (
            ArrangeOption::SRandom,
            true,
            vec![
                Lane::Key1,
                Lane::Key2,
                Lane::Key4,
                Lane::Key7,
                Lane::Key2,
                Lane::Scratch,
                Lane::Key2,
                Lane::Key6,
            ],
        ),
        (
            ArrangeOption::HRandom,
            false,
            vec![
                Lane::Key3,
                Lane::Key4,
                Lane::Key2,
                Lane::Key1,
                Lane::Key6,
                Lane::Scratch,
                Lane::Key7,
                Lane::Key1,
            ],
        ),
        (
            ArrangeOption::AllScratch,
            false,
            vec![
                Lane::Scratch,
                Lane::Key2,
                Lane::Scratch,
                Lane::Key6,
                Lane::Key4,
                Lane::Key1,
                Lane::Scratch,
                Lane::Key3,
            ],
        ),
        (
            ArrangeOption::Random,
            false,
            vec![
                Lane::Key2,
                Lane::Key2,
                Lane::Key2,
                Lane::Key1,
                Lane::Key7,
                Lane::Scratch,
                Lane::Key6,
                Lane::Key5,
            ],
        ),
        (
            ArrangeOption::RRandom,
            false,
            vec![
                Lane::Key7,
                Lane::Key7,
                Lane::Key7,
                Lane::Key2,
                Lane::Key1,
                Lane::Scratch,
                Lane::Key3,
                Lane::Key6,
            ],
        ),
        (
            ArrangeOption::Mirror,
            false,
            vec![
                Lane::Key7,
                Lane::Key7,
                Lane::Key7,
                Lane::Key5,
                Lane::Key6,
                Lane::Scratch,
                Lane::Key4,
                Lane::Key1,
            ],
        ),
    ] {
        let mut chart = make_chart();
        apply_arrange_internal(
            &mut chart,
            arrange,
            Some(0x12_3456),
            None,
            legacy_seed,
            SRandomScheme::Legacy40MsV1,
            None,
        );
        let actual: Vec<_> = lanes_for_notes(&chart).into_iter().map(|(_, lane)| lane).collect();
        assert_eq!(actual, expected, "{arrange:?} legacy_rng={legacy_seed}");

        if arrange != ArrangeOption::SRandom {
            let mut with_lm_scheme = make_chart();
            apply_arrange_internal(
                &mut with_lm_scheme,
                arrange,
                Some(0x12_3456),
                None,
                legacy_seed,
                SRandomScheme::Lm120HzV1,
                None,
            );
            assert_eq!(lanes_for_notes(&with_lm_scheme), lanes_for_notes(&chart));
        }
    }
}

#[test]
fn build_game_session_enables_gauge_auto_shift_from_profile() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.gauge_auto_shift = crate::config::profile_config::GaugeAutoShiftConfig::BestClear;
    let chart = Arc::new(chart());

    let session = build_game_session(chart, &profile, PlaySessionOptions::default());

    assert!(session.gauge.auto_shift);
    assert_eq!(session.gauge.auto_shift_mode, GaugeAutoShiftMode::BestClear);
    assert_eq!(session.gauge.selected, GaugeType::Hazard);
}

#[test]
fn build_game_session_uses_hidden_cover_only_for_hidden_effects() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.lane.hidden = 400;
    profile.play.lane_effect = LaneEffectConfig::Off;
    let off = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    profile.play.lane_effect = LaneEffectConfig::Hidden;
    let hidden = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(off.hidden_cover, 0.0);
    assert_eq!(hidden.hidden_cover, 0.4);
}

#[test]
fn build_game_session_maps_cover_skin_options_from_enable_settings() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.sudden = 290;
    profile.lane.lift = 222;

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert!(!session.lanecover_enabled);
    assert!(!session.lane_cover_visible);
    assert!(session.lift_enabled);

    profile.lane.sudden = 0;
    profile.lane.lift = 0;
    let disabled = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert!(!disabled.lanecover_enabled);
    assert!(disabled.lift_enabled);

    profile.lane.lift = 222;
    profile.lane.lift_enabled = false;
    let lift_disabled =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(lift_disabled.lift, 0.0);
    assert!(!lift_disabled.lift_enabled);

    profile.play.lane_effect = LaneEffectConfig::Sudden;
    let sudden_option =
        build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert!(sudden_option.lanecover_enabled);
}

#[test]
fn build_game_session_clamps_lane_cover_to_remaining_lift_range() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    profile.lane.sudden = 900;
    profile.lane.lift = 200;

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert!((session.lane_cover - 0.8).abs() < 0.000_01);
    assert!((session.lift - 0.2).abs() < 0.000_01);
    assert!(session.lanecover_enabled);
}

#[test]
fn detail_options_survive_save_reload_and_next_play() {
    use crate::config::settings_registry::SettingsEntryId;
    use crate::select_detail_options::DetailOptionDef;
    let mut profile = ProfileConfig::new_default("detail", "Detail", 0);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.lane.sudden = 280;
    profile.lane.hidden = 140;
    profile.lane.lift = 120;
    profile.lane.lift_enabled = false;
    profile.normalize_play_mode_configs();
    for setting in [
        SettingsEntryId::SuddenEnabled,
        SettingsEntryId::HiddenEnabled,
        SettingsEntryId::LiftEnabled,
        SettingsEntryId::Constant,
        SettingsEntryId::BottomShiftableGauge,
    ] {
        assert!(DetailOptionDef::for_setting(setting).unwrap().adjust(
            &mut profile,
            Some(KeyMode::K7),
            1
        ));
    }
    let saved = toml::to_string(&profile).unwrap();
    let loaded: ProfileConfig = toml::from_str(&saved).unwrap();
    for p in [&profile, &loaded] {
        let session = build_game_session(Arc::new(chart()), p, PlaySessionOptions::default());
        assert!(session.lanecover_enabled);
        assert!(session.lift_enabled);
        assert!((session.lane_cover - 0.28).abs() < 0.00001);
        assert!((session.hidden_cover - 0.14).abs() < 0.00001);
        assert!((session.lift - 0.12).abs() < 0.00001);
        assert_eq!(session.gauge.auto_shift_mode, GaugeAutoShiftMode::Off);
        assert_eq!(
            p.play.bottom_shiftable_gauge,
            crate::config::profile_config::BottomShiftableGaugeConfig::Easy
        );
        let mut enabled = p.clone();
        enabled.play.gauge_auto_shift =
            crate::config::profile_config::GaugeAutoShiftConfig::BestClear;
        let enabled =
            build_game_session(Arc::new(chart()), &enabled, PlaySessionOptions::default());
        assert_eq!(enabled.gauge.bottom_shiftable_gauge, GaugeType::Easy);
    }
}

#[test]
fn build_game_session_clamps_profile_misslayer_duration() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.misslayer_duration_ms = 12_000;

    let session = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

    assert_eq!(session.poor_bga_duration_us, 5_000_000);
}
