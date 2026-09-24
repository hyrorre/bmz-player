use super::*;
use crate::snapshot::SkinBestScoreOptions;

#[test]
fn best_score_options_survive_preload_and_stale_retry_metadata() {
    let old = SkinBestScoreOptions { arrange_1p: 1, arrange_2p: 0, double_option: 0 };
    let new = SkinBestScoreOptions { arrange_1p: 2, arrange_2p: 0, double_option: 0 };
    for baseline in [None, Some(new)] {
        for cached in [None, Some(old)] {
            let mut attempt =
                SkinAttemptState { best_score_options: baseline, ..Default::default() };
            attempt
                .merge_known(SkinAttemptState { best_score_options: cached, ..Default::default() });
            assert_eq!(attempt.best_score_options, baseline);
        }
    }
}

#[test]
fn best_score_options_share_number_image_event_and_text_values() {
    let labels = [
        "NORMAL",
        "MIRROR",
        "RANDOM",
        "R-RANDOM",
        "S-RANDOM",
        "SPIRAL",
        "H-RANDOM",
        "ALL-SCR",
        "RANDOM-EX",
        "S-RANDOM-EX",
        "F-RANDOM",
        "MF-RANDOM",
    ];
    for (index, label) in labels.into_iter().enumerate() {
        let state = SkinDrawState {
            skin_attempt: SkinAttemptState {
                best_score_options: Some(SkinBestScoreOptions {
                    arrange_1p: index,
                    arrange_2p: 11 - index,
                    double_option: index % 4,
                }),
                ..Default::default()
            },
            // Current options deliberately differ from the saved best.
            select_arrange_index: 6,
            result_arrange_index: 7,
            ..Default::default()
        };
        assert!(test_skin_op(19200, &[], &state));
        for (ref_id, expected) in [(19201, index), (19202, 11 - index), (19203, index % 4)] {
            assert_eq!(skin_state_number(ref_id, &state), Some(expected as i64));
            assert_eq!(skin_image_index_number(ref_id, &state), Some(expected as i64));
            assert_eq!(skin_state_event_index(ref_id, &state), expected as i32);
        }
        let text = lua_main_state_text_values(&state, &SkinTextState::default());
        assert_eq!(text[&19201], label);
        assert_eq!(text[&19202], labels[11 - index]);
        assert_eq!(text[&19203], ["OFF", "FLIP", "BATTLE", "BATTLE AS"][index % 4]);
    }
}

#[test]
fn absent_best_score_options_are_not_normal() {
    let state = SkinDrawState::default();
    assert!(!test_skin_op(19200, &[], &state));
    assert!(test_skin_op(-19200, &[], &state));
    let text = lua_main_state_text_values(&state, &SkinTextState::default());
    for ref_id in 19201..=19203 {
        assert_eq!(skin_state_number(ref_id, &state), Some(-1));
        assert_eq!(skin_state_event_index(ref_id, &state), -1);
        assert_eq!(text[&ref_id], "");
    }
}
