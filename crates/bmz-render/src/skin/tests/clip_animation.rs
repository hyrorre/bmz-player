use super::*;

fn destination(value: serde_json::Value) -> SkinDestinationDef {
    serde_json::from_value(value).unwrap()
}

fn clip_at(destination: &SkinDestinationDef, time: i32) -> Option<Rect> {
    resolve_destination_frame(destination, time, &[], &SkinDrawState::default())?
        .take_clip(100, 100)
}

fn assert_rect(actual: Rect, expected: [f32; 4]) {
    for (actual, expected) in
        [actual.x, actual.y, actual.width, actual.height].into_iter().zip(expected)
    {
        assert!(approx_eq(actual, expected), "{actual} != {expected}");
    }
}

#[test]
fn destination_clip_inherits_each_field_and_waits_for_complete_start() {
    let destination = destination(serde_json::json!({"id":"image", "loop":300, "dst":[
        {"time":0,"clip_x":10,"clip_y":20},
        {"time":100,"clip_w":30},
        {"time":200,"clip_h":40},
        {"time":300,"clip_x":20,"clip_y":-2147483648}
    ]}));
    for time in [0, 50, 100, 150, 199] {
        assert_eq!(clip_at(&destination, time), None);
    }
    assert_rect(clip_at(&destination, 200).unwrap(), [0.1, 0.4, 0.3, 0.4]);
    assert_rect(clip_at(&destination, 250).unwrap(), [0.15, 0.4, 0.3, 0.4]);
    assert_rect(clip_at(&destination, 400).unwrap(), [0.2, 0.4, 0.3, 0.4]);
}

#[test]
fn destination_clip_uses_float_interpolation_and_common_acc_loop_time() {
    for (acc, x) in [(0, 0.005), (1, 0.0025), (2, 0.0075), (3, 0.0)] {
        let destination = destination(serde_json::json!({"id":"image", "loop":0, "dst":[
            {"time":0,"clip_x":0,"clip_y":0,"clip_w":50,"clip_h":50,"acc":acc},
            {"time":100,"clip_x":1}
        ]}));
        assert_rect(clip_at(&destination, 50).unwrap(), [x, 0.5, 0.5, 0.5]);
        assert_eq!(clip_at(&destination, 150), clip_at(&destination, 50));
    }
    let destination = destination(serde_json::json!({"id":"image", "loop":-1,"dst":[
        {"time":100,"clip_x":0,"clip_y":0,"clip_w":50,"clip_h":50},
        {"time":200,"clip_w":0}
    ]}));
    assert_eq!(clip_at(&destination, 99), None);
    assert_rect(clip_at(&destination, 150).unwrap(), [0.0, 0.5, 0.25, 0.5]);
    assert_eq!(clip_at(&destination, 200), None, "zero size disables clip");
    assert_eq!(clip_at(&destination, 201), None, "the destination has ended");
}

#[test]
fn destination_clip_offsets_keep_fractional_centers_and_relative_position() {
    let destination = destination(serde_json::json!({"id":"image","offsets":[42,42],"dst":[
        {"clip_x":10,"clip_y":20,"clip_w":30,"clip_h":40,"angle":90}
    ]}));
    let mut state = SkinDrawState::default();
    state.skin_offsets.set(
        42,
        crate::skin_offset::SkinOffsetValue { x: 2, y: 3, w: 3, h: 5, r: 45, ..Default::default() },
    );
    let original = resolve_destination_frame(&destination, 0, &[], &state).unwrap();
    let mut ordinary = original;
    apply_skin_offset_to_frame(&destination, &mut ordinary, &state, false);
    assert_rect(ordinary.take_clip(100, 100).unwrap(), [0.105, 0.345, 0.33, 0.45]);
    assert_eq!(ordinary.take_clip(100, 100), None, "clip is removed before geometry caching");
    let mut relative = original;
    apply_skin_offset_to_frame_relative(&destination, &mut relative, &state);
    assert_rect(relative.take_clip(100, 100).unwrap(), [0.1, 0.35, 0.33, 0.45]);
}

#[test]
fn destination_clip_condition_frames_and_disabled_extents_follow_current_state() {
    let destination = destination(serde_json::json!({"id":"image","dst":[
        {"clip_x":10,"clip_y":20,"clip_w":-1,"clip_h":40},
        {"if":[920],"value":{"clip_w":30}}
    ]}));
    assert_eq!(clip_at(&destination, 0), None);
    let mut frame =
        resolve_destination_frame(&destination, 0, &[920], &SkinDrawState::default()).unwrap();
    assert_rect(frame.take_clip(100, 100).unwrap(), [0.1, 0.4, 0.3, 0.4]);
    let mut zero =
        resolve_destination_frame(&destination, 0, &[], &SkinDrawState::default()).unwrap();
    zero.clip.offset(0.0, 0.0, 2.0, 0.0);
    assert_rect(zero.take_clip(100, 100).unwrap(), [0.1, 0.4, 0.01, 0.4]);
}

#[test]
fn destination_clip_global_offset_preserves_axis_alignment_and_balanced_commands() {
    let mut state = SkinDrawState::default();
    state.skin_offsets.set(
        OFFSET_ALL,
        crate::skin_offset::SkinOffsetValue {
            x: 10,
            y: 20,
            w: 50,
            h: -50,
            r: 45,
            ..Default::default()
        },
    );
    let clip = Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    let item = apply_all_offset_to_render_item(SkinRenderItem::PushClip { rect: clip }, &state);
    let SkinRenderItem::PushClip { rect } = item else { panic!("expected clip") };
    assert_eq!(rect, apply_all_offset_to_rect(clip, 1.5, 0.5, 0.1, 0.2));
    let items = wrap_skin_destination_clip(
        vec![SkinRenderItem::Rect {
            rect: clip,
            color: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
        }],
        Some(clip),
    );
    let mut commands = Vec::new();
    append_skin_render_items(&mut commands, &items);
    assert!(matches!(
        commands.as_slice(),
        [DrawCommand::PushClip { .. }, DrawCommand::Rect { .. }, DrawCommand::PopClip]
    ));
}
