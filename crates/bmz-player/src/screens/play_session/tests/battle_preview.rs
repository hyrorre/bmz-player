use super::*;
use crate::screens::play_start::{BattleTargetPlayback, SeedScheme};

#[test]
fn battle_preview_fixed_patterns_match_the_actual_arrangement_for_each_mode_and_seed_scheme() {
    for mode in [
        KeyMode::K4,
        KeyMode::K5,
        KeyMode::K6,
        KeyMode::K7,
        KeyMode::K8,
        KeyMode::K9,
        KeyMode::K10,
        KeyMode::K14,
    ] {
        for arrange in [
            ArrangeOption::Normal,
            ArrangeOption::Mirror,
            ArrangeOption::Random,
            ArrangeOption::RRandom,
            ArrangeOption::RandomEx,
            ArrangeOption::FRandom,
            ArrangeOption::MFRandom,
        ] {
            for legacy in [false, true] {
                for double in [DoubleOption::Off, DoubleOption::Flip] {
                    let mut arrangement = BattleTargetPlayback::Seed {
                        arrange,
                        arrange_2p: ArrangeOption::Mirror,
                        double_option: double,
                        packed_seed: None,
                        seed_scheme: SeedScheme::Beatoraja24,
                    }
                    .arrangement();
                    arrangement.arrange_seed = Some(12345);
                    arrangement.arrange_seed_2p = Some(9876);
                    arrangement.legacy_arrange_seed = legacy;
                    let mut source = chart();
                    source.metadata.key_mode = mode;
                    for (i, notes) in source.lane_notes.iter_mut().enumerate() {
                        notes.push(NoteEvent {
                            id: NoteId(i as u32),
                            lane: Lane::ALL[i],
                            tick: bmz_core::time::ChartTick(0),
                            time: TimeUs(1_000_000),
                            kind: NoteKind::Tap,
                            sound: None,
                            layered_sounds: Vec::new(),
                            damage: None,
                        });
                    }
                    apply_double_option(&mut source, double);
                    apply_arrange_pair(
                        &mut source,
                        arrange,
                        ArrangeOption::Mirror,
                        Some(12345),
                        Some(9876),
                        legacy,
                        SRandomScheme::default(),
                        None,
                        None,
                        None,
                    );
                    let expected: Vec<u8> =
                        source.lane_notes.iter().map(|notes| notes[0].id.0 as u8).collect();
                    assert_eq!(
                        preview_arrange_pattern(mode, &arrangement),
                        Some(expected),
                        "{mode:?} {arrange:?} legacy={legacy} {double:?}"
                    );
                }
            }
        }
    }
}

fn lr2_arrangement(
    arrange: ArrangeOption,
    arrange_2p: ArrangeOption,
    seed: i64,
) -> crate::screens::play_start::BattleTargetArrangement {
    BattleTargetPlayback::Seed {
        arrange,
        arrange_2p,
        double_option: DoubleOption::Off,
        packed_seed: Some(seed),
        seed_scheme: SeedScheme::Lr2,
    }
    .arrangement()
}

/// Asserts that `pattern` sends key `c` of `keys` to key `table[c - 1]`, as LR2 writes its
/// RANDOM table. A pattern lists the source lane of each destination lane.
fn assert_lr2_keys(pattern: &[u8], keys: &[Lane], table: &[usize]) {
    for (source, &destination) in table.iter().enumerate() {
        assert_eq!(
            usize::from(pattern[keys[destination - 1].index()]),
            keys[source].index(),
            "key {} of {keys:?}",
            source + 1
        );
    }
}

#[test]
fn battle_preview_draws_lr2_random_lanes_from_a_fresh_sequence() {
    let p1_keys =
        [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key6, Lane::Key7];
    let p2_keys =
        [Lane::Key8, Lane::Key9, Lane::Key10, Lane::Key11, Lane::Key12, Lane::Key13, Lane::Key14];

    let arrangement = lr2_arrangement(ArrangeOption::Random, ArrangeOption::Normal, 2179);
    let pattern = preview_arrange_pattern(KeyMode::K7, &arrangement).unwrap();
    assert_lr2_keys(&pattern, &p1_keys, &[1, 5, 2, 6, 3, 7, 4]);
    assert_eq!(pattern[Lane::Scratch.index()], 0);
    // Applying the pattern moves the notes the same way.
    let mut source = chart();
    source.metadata.key_mode = KeyMode::K7;
    for (i, notes) in source.lane_notes.iter_mut().enumerate() {
        notes.push(NoteEvent {
            id: NoteId(i as u32),
            lane: Lane::ALL[i],
            tick: bmz_core::time::ChartTick(0),
            time: TimeUs(1_000_000),
            kind: NoteKind::Tap,
            sound: None,
            layered_sounds: Vec::new(),
            damage: None,
        });
    }
    apply_arrange(&mut source, ArrangeOption::Random, None, Some(pattern.as_slice()));
    assert_eq!(source.lane_notes[Lane::Key5.index()][0].id, NoteId(Lane::Key2.index() as u32));
    assert_eq!(source.lane_notes[Lane::Key7.index()][0].id, NoteId(Lane::Key6.index() as u32));

    let identity = lr2_arrangement(ArrangeOption::Random, ArrangeOption::Normal, 391);
    let pattern = preview_arrange_pattern(KeyMode::K7, &identity).unwrap();
    assert_eq!(pattern, (0..LANE_COUNT as u8).collect::<Vec<_>>());

    // DP: 1P draws first and 2P continues the same sequence.
    let double = lr2_arrangement(ArrangeOption::Random, ArrangeOption::Random, 2179);
    let pattern = preview_arrange_pattern(KeyMode::K14, &double).unwrap();
    assert_lr2_keys(&pattern, &p1_keys, &[1, 5, 2, 6, 3, 7, 4]);
    assert_lr2_keys(&pattern, &p2_keys, &[4, 6, 5, 2, 1, 7, 3]);
    // A 2P side that does not draw leaves the 1P lanes as they were.
    let one_side = lr2_arrangement(ArrangeOption::Mirror, ArrangeOption::Random, 2179);
    let pattern = preview_arrange_pattern(KeyMode::K14, &one_side).unwrap();
    assert_lr2_keys(&pattern, &p1_keys, &[7, 6, 5, 4, 3, 2, 1]);
    assert_lr2_keys(&pattern, &p2_keys, &[1, 5, 2, 6, 3, 7, 4]);

    let five = lr2_arrangement(ArrangeOption::Random, ArrangeOption::Normal, 2179);
    let pattern = preview_arrange_pattern(KeyMode::K5, &five).unwrap();
    assert_lr2_keys(&pattern, &p1_keys[..5], &[1, 4, 2, 5, 3]);

    // Out-of-range LR2 seeds and per-note options are not drawn as fixed keys.
    let out_of_range = lr2_arrangement(ArrangeOption::Random, ArrangeOption::Normal, 0x7fff);
    assert_eq!(out_of_range.lr2_seed, None);
    assert!(preview_arrange_pattern(KeyMode::K7, &out_of_range).is_none());
    let s_random = lr2_arrangement(ArrangeOption::SRandom, ArrangeOption::Normal, 2179);
    assert!(preview_arrange_pattern(KeyMode::K7, &s_random).is_none());
}

#[test]
fn battle_preview_draws_the_p1_seed_of_a_d_battle_row_on_an_sp_chart() {
    let seed_row = |double_option, packed_seed| {
        BattleTargetPlayback::Seed {
            arrange: ArrangeOption::Random,
            arrange_2p: ArrangeOption::Random,
            double_option,
            packed_seed: Some(packed_seed),
            seed_scheme: SeedScheme::Beatoraja24,
        }
        .arrangement()
    };
    let expected =
        preview_arrange_pattern(KeyMode::K7, &seed_row(DoubleOption::Off, 0x123456)).unwrap();
    assert_ne!(expected, (0..LANE_COUNT as u8).collect::<Vec<_>>());

    // D-BATTLE packs both sides like DP even though the row's chart is 7K.
    for double_option in [DoubleOption::Battle, DoubleOption::BattleAutoScratch] {
        assert_eq!(
            preview_arrange_pattern(KeyMode::K7, &seed_row(double_option, 0x654321_123456)),
            Some(expected.clone()),
            "{double_option:?}"
        );
    }
    // Other rows keep the SP format, where a value above 24 bits is invalid.
    assert!(
        preview_arrange_pattern(KeyMode::K7, &seed_row(DoubleOption::Off, 0x654321_123456))
            .is_none()
    );
}

#[test]
fn battle_preview_never_draws_a_missing_seed_or_a_note_randomization_as_fixed_keys() {
    for arrange in ArrangeOption::VALUES {
        let mut arrangement = BattleTargetPlayback::Seed {
            arrange,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            packed_seed: None,
            seed_scheme: SeedScheme::Beatoraja24,
        }
        .arrangement();
        assert_eq!(
            preview_arrange_pattern(KeyMode::K7, &arrangement).is_some(),
            matches!(arrange, ArrangeOption::Normal | ArrangeOption::Mirror)
        );
        arrangement.packed_seed = Some(0);
        assert_eq!(
            preview_arrange_pattern(KeyMode::K7, &arrangement).is_some(),
            matches!(
                arrange,
                ArrangeOption::Normal
                    | ArrangeOption::Mirror
                    | ArrangeOption::Random
                    | ArrangeOption::RRandom
                    | ArrangeOption::RandomEx
                    | ArrangeOption::FRandom
                    | ArrangeOption::MFRandom
            )
        );
        arrangement.arrange_pattern = Some(vec![0; 16]);
        assert!(preview_arrange_pattern(KeyMode::K7, &arrangement).is_none());
    }
}
