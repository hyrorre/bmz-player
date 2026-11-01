use super::*;
use crate::screens::play_start::BattleTargetPlayback;

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

#[test]
fn battle_preview_never_draws_a_missing_seed_or_a_note_randomization_as_fixed_keys() {
    for arrange in ArrangeOption::VALUES {
        let mut arrangement = BattleTargetPlayback::Seed {
            arrange,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            packed_seed: None,
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
