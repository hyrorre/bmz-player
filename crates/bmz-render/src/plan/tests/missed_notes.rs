use super::*;

fn default_missed_note_image(
    fall: f32,
    lift: f32,
    mark_processed_note: bool,
) -> Option<(Rect, Color)> {
    let mut snapshot =
        RenderSnapshot { key_mode: KeyMode::K9, lift, mark_processed_note, ..Default::default() };
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(0),
        y: -fall,
        alpha: 0.6,
        kind: NoteVisualKind::Tap,
        processed_judge: Some(Judge::Poor),
        pms_miss_fall: Some(fall),
    });
    DrawPlan::from_scene(&AppSceneSnapshot::Play(snapshot)).commands.iter().find_map(|command| {
        match command {
            DrawCommand::Image { texture, rect, tint, .. } if *texture == DEFAULT_NOTE_TEXTURE => {
                Some((*rect, *tint))
            }
            _ => None,
        }
    })
}

#[test]
fn default_pms_notes_fall_below_judge_line_and_disappear() {
    for mark in [false, true] {
        for (lift, judge, distance) in [(0.0, 0.824, 0.774), (0.4, 0.464, 0.414)] {
            let (waiting, tint) = default_missed_note_image(0.0, lift, mark).unwrap();
            assert!(approx_eq(waiting.y + waiting.height, judge));
            assert!(approx_eq(tint.a, 0.6), "misses keep normal sprites and note alpha");
            let (falling, _) = default_missed_note_image(0.1, lift, mark).unwrap();
            assert!(approx_eq(falling.y, waiting.y + 0.1 * distance));
            assert!(falling.y > waiting.y);
            // ノーツ下端がレーン下端0.95を越えたら、8小節のsnapshot上限を待たず消える。
            assert!(default_missed_note_image(2.0, lift, mark).is_none());
            assert!(default_missed_note_image(8.0, lift, mark).is_none());
        }
    }
}

#[test]
fn default_pms_notes_disappear_after_wait_when_lift_closes_the_lane() {
    for lift in [0.86, 1.0] {
        assert!(default_missed_note_image(0.0, lift, false).is_some());
        assert!(default_missed_note_image(0.1, lift, false).is_none());
    }
}
