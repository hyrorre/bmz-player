use super::*;

#[test]
fn battle_presentation_uses_opponent_arrangement_on_2p_lanes() {
    let mut primary = chart();
    primary.metadata.key_mode = KeyMode::K7;
    primary.lane_notes[Lane::Key1.index()].push(NoteEvent {
        id: NoteId(1),
        lane: Lane::Key1,
        kind: NoteKind::Tap,
        tick: bmz_core::time::ChartTick(0),
        time: TimeUs(0),
        sound: None,
        layered_sounds: Vec::new(),
        damage: None,
    });
    let mut opponent = primary.clone();
    opponent.lane_notes[Lane::Key2.index()] =
        std::mem::take(&mut opponent.lane_notes[Lane::Key1.index()]);
    for note in &mut opponent.lane_notes[Lane::Key2.index()] {
        note.lane = Lane::Key2;
    }

    apply_battle_opponent_chart(&mut primary, &opponent);

    assert_eq!(primary.metadata.key_mode, KeyMode::K14);
    assert_eq!(primary.total_notes, 1);
    assert!(primary.lane_notes[Lane::Key8.index()].is_empty());
    assert_eq!(primary.lane_notes[Lane::Key9.index()].len(), 1);
    assert_eq!(primary.lane_notes[Lane::Key9.index()][0].lane, Lane::Key9);
}

#[test]
fn build_game_session_applies_dx_9key_pop_rules() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Dx;
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K9;

    let session = build_game_session(Arc::new(chart), &profile, PlaySessionOptions::default());

    assert_eq!(session.base_judge_window.pgreat_us, 25_000);
    assert_eq!(session.base_judge_window.good_us, 87_500);
    assert_eq!(session.judge.window_set.long_note_end.good_us, 217_000);
    assert_eq!(session.judge.window_set.long_note_release_margin_us, 200_000);
    assert!(session.score.empty_poor_breaks_combo);
    let normal = session
        .gauge
        .gauges
        .iter()
        .find(|g| g.definition.gauge_type == GaugeType::Normal)
        .expect("Normal gauge present");
    assert_eq!((normal.definition.min, normal.definition.max), (2.0, 120.0));
    assert_eq!((normal.definition.init, normal.definition.border), (30.0, 85.0));
    assert_eq!(normal.definition.values[3..], [-1.992, -5.976, -5.976]);
}

#[test]
fn dx_9key_replay_judgements_apply_current_pop_damage() {
    use bmz_core::judge::Judge;
    use bmz_core::replay::ReplayEvent;
    use bmz_gameplay::session::{process_misses, process_replay_inputs};

    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    profile.play.rule_mode = RuleMode::Dx;
    for (input_time, expected_judge, expected_gauge) in [
        (Some(900_000), Judge::Bad, 28.008),
        (Some(800_000), Judge::EmptyPoor, 24.024),
        (None, Judge::Poor, 24.024),
    ] {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K9;
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
        let mut session = build_game_session(
            Arc::new(chart),
            &profile,
            PlaySessionOptions {
                gauge_override: Some(GaugeType::Normal),
                replay_player: Some(ReplayPlayer {
                    events: input_time
                        .map(|time| ReplayEvent {
                            lane: Lane::Key1,
                            kind: InputKind::Press,
                            time: TimeUs(time),
                            device_kind: bmz_core::input::InputDeviceKind::Keyboard,
                            scratch_direction: None,
                        })
                        .into_iter()
                        .collect(),
                    ..ReplayPlayer::default()
                }),
                ..PlaySessionOptions::default()
            },
        );

        let judgements = if let Some(time) = input_time {
            process_replay_inputs(&mut session, TimeUs(time))
        } else {
            process_misses(&mut session, TimeUs(1_200_000))
        };
        assert_eq!(judgements.len(), 1);
        assert_eq!(judgements[0].judge, expected_judge);
        assert!((session.gauge.current().value - expected_gauge).abs() < 0.000_1);
    }
}

#[test]
fn build_game_session_sets_empty_poor_combo_policy_from_keymode() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let mut chart_k5 = chart();
    chart_k5.metadata.key_mode = KeyMode::K5;
    let mut chart_k7 = chart();
    chart_k7.metadata.key_mode = KeyMode::K7;

    let session_k5 =
        build_game_session(Arc::new(chart_k5), &profile, PlaySessionOptions::default());
    let session_k7 =
        build_game_session(Arc::new(chart_k7), &profile, PlaySessionOptions::default());

    assert!(session_k5.score.empty_poor_breaks_combo);
    assert!(!session_k7.score.empty_poor_breaks_combo);
}

#[test]
fn mirror_permutation_k9_reverses_all_nine_keys() {
    let perm = mirror_permutation(KeyMode::K9);
    assert_eq!(perm[Lane::Key1 as usize], Lane::Key9 as usize);
    assert_eq!(perm[Lane::Key9 as usize], Lane::Key1 as usize);
    assert_eq!(perm[Lane::Key5 as usize], Lane::Key5 as usize);
}

#[test]
fn arrange_lane_groups_cover_no_scratch_keymodes() {
    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        let expected: Vec<usize> =
            key_mode.active_lanes().iter().map(|&lane| lane.index()).collect();

        assert_eq!(arrange_lane_groups(key_mode, false), vec![expected.clone()]);
        assert_eq!(arrange_lane_groups(key_mode, true), vec![expected]);
    }
}

#[test]
fn mirror_permutation_reverses_no_scratch_keymodes() {
    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        let perm = mirror_permutation(key_mode);
        let active = key_mode.active_lanes();

        for (source, dest) in active.iter().zip(active.iter().rev()) {
            assert_eq!(
                perm[source.index()],
                dest.index(),
                "mirror should reverse {} lane {:?}",
                key_mode.as_str(),
                source
            );
        }
    }
}

#[test]
fn random_lane_permutation_k9_preserves_active_lanes() {
    let perm = random_lane_permutation(42, KeyMode::K9, false, false);
    let active: HashSet<_> = KeyMode::K9.active_lanes().iter().map(|&lane| lane as usize).collect();
    let mapped: HashSet<_> =
        KeyMode::K9.active_lanes().iter().map(|&lane| perm[lane as usize]).collect();
    assert_eq!(active, mapped);
}

#[test]
fn random_permutations_preserve_no_scratch_active_lanes() {
    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        let active: HashSet<_> = key_mode.active_lanes().iter().map(|&lane| lane.index()).collect();
        for perm in [
            random_lane_permutation(42, key_mode, false, false),
            random_lane_permutation(42, key_mode, true, false),
            rotate_lane_permutation(42, key_mode, false, false),
            rotate_lane_permutation(42, key_mode, true, false),
        ] {
            let mapped: HashSet<_> =
                key_mode.active_lanes().iter().map(|&lane| perm[lane.index()]).collect();
            assert_eq!(
                active,
                mapped,
                "random permutation should stay inside {} active lanes",
                key_mode.as_str()
            );
        }
    }
}

#[test]
fn f_random_groups_keep_odd_center_lane_fixed() {
    assert_eq!(
        f_random_lane_groups(KeyMode::K7),
        vec![
            vec![Lane::Key1.index(), Lane::Key2.index(), Lane::Key3.index()],
            vec![Lane::Key5.index(), Lane::Key6.index(), Lane::Key7.index()],
        ]
    );
    assert_eq!(
        f_random_lane_groups(KeyMode::K5),
        vec![
            vec![Lane::Key1.index(), Lane::Key2.index()],
            vec![Lane::Key4.index(), Lane::Key5.index()],
        ]
    );
    assert_eq!(
        f_random_lane_groups(KeyMode::K9),
        vec![
            vec![Lane::Key1.index(), Lane::Key2.index(), Lane::Key3.index(), Lane::Key4.index(),],
            vec![Lane::Key6.index(), Lane::Key7.index(), Lane::Key8.index(), Lane::Key9.index(),],
        ]
    );
}

#[test]
fn f_random_groups_split_even_key_modes_into_halves() {
    assert_eq!(
        f_random_lane_groups(KeyMode::K4),
        vec![
            vec![Lane::Key1.index(), Lane::Key2.index()],
            vec![Lane::Key3.index(), Lane::Key4.index()],
        ]
    );
    assert_eq!(
        f_random_lane_groups(KeyMode::K8),
        vec![
            vec![Lane::Key1.index(), Lane::Key2.index(), Lane::Key3.index(), Lane::Key4.index(),],
            vec![Lane::Key5.index(), Lane::Key6.index(), Lane::Key7.index(), Lane::Key8.index(),],
        ]
    );
}

#[test]
fn f_random_keeps_7k_center_lane_in_place() {
    let mut chart = chart();
    chart.metadata.key_mode = KeyMode::K7;
    chart.lane_notes[Lane::Key4.index()].push(note(1, Lane::Key4, 1_000_000));

    let applied = apply_arrange(&mut chart, ArrangeOption::FRandom, Some(42), None);

    assert_eq!(applied.arrange, ArrangeOption::FRandom);
    assert_eq!(applied.seed, Some(42));
    assert_eq!(chart.lane_notes[Lane::Key4.index()][0].lane, Lane::Key4);
    assert_eq!(chart.lane_notes[Lane::Key4.index()][0].id, NoteId(1));
}

#[test]
fn mf_random_applies_mirror_after_f_random() {
    let f_random = f_random_lane_permutation(42, KeyMode::K7, ArrangeOption::FRandom, false);
    let mf_random = f_random_lane_permutation(42, KeyMode::K7, ArrangeOption::MFRandom, false);
    let mirror = mirror_permutation(KeyMode::K7);

    assert_eq!(mf_random, compose_lane_permutations(&f_random, &mirror));
    assert_eq!(mf_random[Lane::Key4.index()], Lane::Key4.index());
}

#[test]
fn scratch_required_arrange_falls_back_to_normal_without_scratch_lane() {
    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        for arrange in
            [ArrangeOption::AllScratch, ArrangeOption::RandomEx, ArrangeOption::SRandomEx]
        {
            let mut chart = chart();
            chart.metadata.key_mode = key_mode;
            chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
            let before = lanes_for_notes(&chart);

            let applied = apply_arrange(&mut chart, arrange, Some(7), None);

            assert_eq!(applied.arrange, ArrangeOption::Normal);
            assert_eq!(applied.seed, Some(7));
            assert_eq!(applied.pattern, None);
            assert_eq!(lanes_for_notes(&chart), before);
        }
    }
}

#[test]
fn scratch_required_arrange_ignores_replay_pattern_without_scratch_lane() {
    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        let mut chart = chart();
        chart.metadata.key_mode = key_mode;
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
        let before = lanes_for_notes(&chart);

        let mut pattern: Vec<u8> = (0u8..LANE_COUNT as u8).collect();
        pattern[Lane::Key1.index()] = Lane::Key2.index() as u8;
        pattern[Lane::Key2.index()] = Lane::Key1.index() as u8;

        let applied = apply_arrange(&mut chart, ArrangeOption::RandomEx, Some(7), Some(&pattern));

        assert_eq!(applied.arrange, ArrangeOption::Normal);
        assert_eq!(applied.seed, Some(7));
        assert_eq!(applied.pattern, None);
        assert_eq!(lanes_for_notes(&chart), before);
    }
}

#[test]
fn note_arrange_keeps_no_scratch_modes_inside_active_lanes() {
    for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
        for arrange in [ArrangeOption::SRandom, ArrangeOption::Spiral, ArrangeOption::HRandom] {
            let mut chart = chart();
            chart.metadata.key_mode = key_mode;
            for (index, &lane) in key_mode.active_lanes().iter().enumerate() {
                chart.lane_notes[lane.index()].push(note(
                    (index + 1) as u32,
                    lane,
                    1_000_000 + index as i64 * 1_000,
                ));
            }

            apply_arrange(&mut chart, arrange, Some(7), None);

            let active: HashSet<_> =
                key_mode.active_lanes().iter().map(|&lane| lane.index()).collect();
            for note in chart.lane_notes.iter().flatten() {
                assert!(
                    active.contains(&note.lane.index()),
                    "{arrange:?} should keep {} note {:?} inside active lanes",
                    key_mode.as_str(),
                    note.id
                );
            }
        }
    }
}

#[test]
fn splitmix64_matches_known_seed_zero_outputs() {
    let mut rng = SplitMix64::new(0);

    assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
    assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
}

#[test]
fn random_lane_shuffle_matches_beatoraja_java_fixture() {
    // java.util.Random(42) + LaneRandomShuffleModifier's remove-at-index loop.
    let lanes = vec![0, 1, 2, 3, 4, 5, 6];
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = ArrangeRng::new(42, false);

    shuffle_lane_group(&mut rng, &lanes, &mut perm, false);

    assert_eq!(&perm[..7], &[1, 4, 5, 0, 2, 6, 3]);
}
