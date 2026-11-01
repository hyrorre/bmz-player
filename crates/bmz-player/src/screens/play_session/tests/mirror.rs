use super::*;

const MODES: [KeyMode; 8] = [
    KeyMode::K4,
    KeyMode::K5,
    KeyMode::K6,
    KeyMode::K7,
    KeyMode::K8,
    KeyMode::K9,
    KeyMode::K10,
    KeyMode::K14,
];

fn is_double(mode: KeyMode) -> bool {
    matches!(mode, KeyMode::K10 | KeyMode::K14)
}

/// Launches a chart with one note per lane and returns the original lane on each lane.
fn launched(
    mode: KeyMode,
    sides: [(ArrangeOption, Option<i64>); 2],
    double: DoubleOption,
    legacy: bool,
    pattern: Option<&[u8]>,
) -> (Vec<u32>, AppliedArrange) {
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
    let applied = apply_arrange_pair(
        &mut source,
        sides[0].0,
        sides[1].0,
        sides[0].1,
        sides[1].1,
        legacy,
        SRandomScheme::default(),
        None,
        None,
        pattern,
    );
    (source.lane_notes.iter().map(|notes| notes[0].id.0).collect(), applied)
}

/// What each lane shows once the keys of every side are mirrored; scratches stay.
fn mirrored(mode: KeyMode, lanes: &[u32]) -> Vec<u32> {
    let mut mirror: Vec<usize> = (0..LANE_COUNT).collect();
    for side in [ArrangeSide::P1, ArrangeSide::P2] {
        merge_lane_permutation(&mut mirror, &mirror_lane_permutation_for_side(mode, side));
    }
    mirror.iter().map(|&lane| lanes[lane]).collect()
}

fn expected_option(mode: KeyMode, arrange: ArrangeOption) -> ArrangeOption {
    match arrange {
        ArrangeOption::Normal => ArrangeOption::Mirror,
        ArrangeOption::Mirror => ArrangeOption::Normal,
        ArrangeOption::FRandom => ArrangeOption::MFRandom,
        ArrangeOption::MFRandom => ArrangeOption::FRandom,
        // Without a scratch, RANDOM-EX plays as NORMAL.
        ArrangeOption::RandomEx if !key_mode_has_scratch(mode) => ArrangeOption::Mirror,
        other => other,
    }
}

#[test]
fn mirrored_copies_reproduce_the_mirrored_lanes_with_an_ordinary_seed() {
    for mode in MODES {
        for arrange in [
            ArrangeOption::Normal,
            ArrangeOption::Mirror,
            ArrangeOption::Random,
            ArrangeOption::RRandom,
            ArrangeOption::RandomEx,
            ArrangeOption::FRandom,
            ArrangeOption::MFRandom,
        ] {
            let doubles: &[DoubleOption] = if is_double(mode) {
                &[DoubleOption::Off, DoubleOption::Flip]
            } else {
                &[DoubleOption::Off]
            };
            for &double in doubles {
                for legacy in [false, true] {
                    for (seed, seed_2p) in [(12_345, 9_876), (777_777, 4_242)] {
                        let sides = [(arrange, Some(seed)), (arrange, Some(seed_2p))];
                        let (source, _) = launched(mode, sides, double, legacy, None);
                        let result = mirror_copied_arrangement(
                            mode,
                            &CopiedArrangement {
                                arrange,
                                arrange_2p: arrange,
                                double_option: double,
                                seed: Some(seed),
                                seed_2p: Some(seed_2p),
                                legacy_seed: legacy,
                                pattern: None,
                            },
                        )
                        .unwrap();
                        let case = format!("{mode:?} {arrange:?} {double:?} legacy={legacy}");
                        assert_eq!(result.arrange, expected_option(mode, arrange), "{case}");
                        if is_double(mode) {
                            assert_eq!(result.arrange_2p, expected_option(mode, arrange), "{case}");
                        }
                        // The mirrored chart comes from the option and seed alone, as a
                        // score or replay would reproduce it.
                        let sides =
                            [(result.arrange, result.seed), (result.arrange_2p, result.seed_2p)];
                        let (lanes, _) = launched(mode, sides, double, false, None);
                        assert_eq!(lanes, mirrored(mode, &source), "{case}");
                    }
                }
            }
        }
    }
}

#[test]
fn mirrored_copies_use_the_lanes_stored_with_a_replay() {
    for (mode, double) in [(KeyMode::K7, DoubleOption::Off), (KeyMode::K14, DoubleOption::Flip)] {
        for arrange in [ArrangeOption::Random, ArrangeOption::FRandom, ArrangeOption::RandomEx] {
            let sides = [(arrange, Some(31_337)), (ArrangeOption::RRandom, Some(2_024))];
            let (source, applied) = launched(mode, sides, double, false, None);
            let pattern = applied.pattern.unwrap();
            // Only the stored lanes describe this chart.
            let result = mirror_copied_arrangement(
                mode,
                &CopiedArrangement {
                    arrange,
                    arrange_2p: ArrangeOption::RRandom,
                    double_option: double,
                    seed: None,
                    seed_2p: None,
                    legacy_seed: false,
                    pattern: Some(&pattern),
                },
            )
            .unwrap();
            let sides = [(result.arrange, result.seed), (result.arrange_2p, result.seed_2p)];
            let (lanes, _) = launched(mode, sides, double, false, None);
            assert_eq!(lanes, mirrored(mode, &source), "{mode:?} {arrange:?}");
        }
    }
}

#[test]
fn mirrored_copies_keep_note_randomization_and_missing_seeds() {
    let copy = |mode, arrange, arrange_2p, double_option, seed: Option<i64>, legacy_seed| {
        mirror_copied_arrangement(
            mode,
            &CopiedArrangement {
                arrange,
                arrange_2p,
                double_option,
                seed,
                seed_2p: seed.map(|seed| seed + 1),
                legacy_seed,
                pattern: None,
            },
        )
    };
    // Per-note arrangements are played as copied, without mirroring.
    for arrange in [
        ArrangeOption::SRandom,
        ArrangeOption::Spiral,
        ArrangeOption::HRandom,
        ArrangeOption::AllScratch,
        ArrangeOption::SRandomEx,
    ] {
        assert_eq!(
            copy(KeyMode::K7, arrange, ArrangeOption::Normal, DoubleOption::Off, Some(5), false),
            None
        );
    }
    // In DP, each side is mirrored on its own.
    let result = copy(
        KeyMode::K14,
        ArrangeOption::Random,
        ArrangeOption::SRandom,
        DoubleOption::Off,
        Some(5),
        false,
    )
    .unwrap();
    assert_eq!(result.arrange, ArrangeOption::Random);
    assert_ne!(result.seed, Some(5));
    assert_eq!((result.arrange_2p, result.seed_2p), (ArrangeOption::SRandom, Some(6)));
    // A legacy 2P seed cannot be kept beside a current 1P seed.
    assert_eq!(
        copy(
            KeyMode::K14,
            ArrangeOption::Random,
            ArrangeOption::SRandom,
            DoubleOption::Off,
            Some(5),
            true
        ),
        None
    );
    // Without the rival's seed only NORMAL and MIRROR, which need none, are mirrored.
    assert_eq!(
        copy(
            KeyMode::K7,
            ArrangeOption::Random,
            ArrangeOption::Normal,
            DoubleOption::Off,
            None,
            false
        ),
        None
    );
    assert_eq!(
        copy(
            KeyMode::K7,
            ArrangeOption::Normal,
            ArrangeOption::Normal,
            DoubleOption::Off,
            None,
            false
        )
        .map(|result| result.arrange),
        Some(ArrangeOption::Mirror)
    );
    // BATTLE builds the 2P side from the 1P chart, so it is not mirrored per side.
    assert_eq!(
        copy(
            KeyMode::K14,
            ArrangeOption::Mirror,
            ArrangeOption::Mirror,
            DoubleOption::Battle,
            Some(5),
            false
        ),
        None
    );
}
