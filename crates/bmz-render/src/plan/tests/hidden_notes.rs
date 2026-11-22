use super::*;
use crate::skin_offset::SkinOffsetValue;

fn coincident_hidden_note_and_mine() -> RenderSnapshot {
    let mut snapshot = RenderSnapshot { show_hidden_notes: true, ..Default::default() };
    let lane = Lane::Key1;
    let time = TimeUs(2_000_000);
    snapshot.visible_notes[lane.index()].push(VisibleNote {
        lane,
        time,
        y: 0.5,
        alpha: 0.5,
        kind: NoteVisualKind::Invisible,
        processed_judge: None,
        pms_miss_fall: None,
    });
    snapshot.visible_mines[lane.index()].push(VisibleMine {
        lane,
        time,
        y: 0.5,
        alpha: 1.0,
        damage: 1.0,
    });
    snapshot.visible_mines[lane.index()].push(VisibleMine {
        lane,
        time: TimeUs(2_100_000),
        y: 0.75,
        alpha: 1.0,
        damage: 1.0,
    });
    snapshot
}

fn document_skin() -> SkinContext {
    let doc = serde_json::from_str(
        r#"{
        "type": 0, "w": 100, "h": 100,
        "note": {"id":"notes","dst":[{"x":10,"y":20,"w":5,"h":60}]},
        "destination": [{"id":"notes","offset":30}]
    }"#,
    )
    .unwrap();
    SkinContext::from_manifest_and_document(SkinManifest::default(), doc, [])
}

fn document_plan(global: SkinOffsetValue, notes: SkinOffsetValue) -> DrawPlan {
    let mut snapshot = coincident_hidden_note_and_mine();
    snapshot.skin_offsets.set(10, global);
    snapshot.skin_offsets.set(30, notes);
    DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &document_skin(),
        &mut crate::skin::DynamicTimerRuntime::default(),
    )
}

fn mines(plan: &DrawPlan) -> impl Iterator<Item = (usize, Rect)> + '_ {
    plan.commands.iter().enumerate().filter_map(|(i, command)| match command {
        DrawCommand::Image { texture, rect, .. } if *texture == DEFAULT_MINE_NOTE_TEXTURE => {
            Some((i, *rect))
        }
        _ => None,
    })
}

fn borders(plan: &DrawPlan) -> Vec<(usize, Rect)> {
    let orange = Color::rgba(1.0, 165.0 / 255.0, 0.0, 0.5);
    let borders: Vec<_> = plan
        .commands
        .iter()
        .enumerate()
        .filter_map(|(i, command)| match command {
            DrawCommand::Rect { rect, color } if *color == orange => Some((i, *rect)),
            _ => None,
        })
        .collect();
    assert_eq!(borders.len(), 4);
    borders
}

#[test]
fn hidden_note_follows_global_translation_and_scale() {
    let plan = document_plan(
        SkinOffsetValue { x: 10, y: 5, w: 50, h: -25, ..Default::default() },
        Default::default(),
    );
    let (_, target) = mines(&plan).next().unwrap();
    assert!(approx_eq(target.x, 0.25));
    let edges = borders(&plan);
    let bounds = |edge: fn(Rect) -> f32, op: fn(f32, f32) -> f32| {
        edges.iter().map(|(_, rect)| edge(*rect)).reduce(op).unwrap()
    };
    assert!(approx_eq(bounds(|r| r.x, f32::min), target.x));
    assert!(approx_eq(bounds(|r| r.y, f32::min), target.y));
    assert!(approx_eq(bounds(|r| r.x + r.width, f32::max), target.x + target.width));
    assert!(approx_eq(bounds(|r| r.y + r.height, f32::max), target.y + target.height));
}

#[test]
fn hidden_note_keeps_lane_geometry_under_notes_destination_offset() {
    let baseline = document_plan(Default::default(), Default::default());
    let offset = document_plan(
        Default::default(),
        SkinOffsetValue { x: 8, y: 6, w: 4, h: 2, ..Default::default() },
    );
    assert_ne!(mines(&baseline).next().unwrap().1, mines(&offset).next().unwrap().1);
    assert_eq!(borders(&baseline), borders(&offset));
}

#[test]
fn hidden_note_draws_over_coincident_mine_in_both_playfields() {
    let default = DrawPlan::from_scene(&AppSceneSnapshot::Play(coincident_hidden_note_and_mine()));
    for plan in [default, document_plan(Default::default(), Default::default())] {
        let mut images = mines(&plan);
        let (same_time, _) = images.next().unwrap();
        let (later, _) = images.next().unwrap();
        assert!(borders(&plan).iter().all(|(index, _)| same_time < *index && *index < later));
    }
}

#[test]
fn hidden_note_fade_covers_each_border_pixel_once() {
    let plan = DrawPlan::from_scene(&AppSceneSnapshot::Play(coincident_hidden_note_and_mine()));
    let (_, rect) = mines(&plan).next().unwrap();
    let edges = borders(&plan);
    // Sample the centres of the original 32 x 8 sprite's pixels.
    for row in 0..8 {
        for column in 0..32 {
            let x = rect.x + (column as f32 + 0.5) * rect.width / 32.0;
            let y = rect.y + (row as f32 + 0.5) * rect.height / 8.0;
            let layers = edges
                .iter()
                .filter(|(_, edge)| {
                    x >= edge.x
                        && x < edge.x + edge.width
                        && y >= edge.y
                        && y < edge.y + edge.height
                })
                .count();
            let on_border = !(2..30).contains(&column) || !(2..6).contains(&row);
            assert_eq!(layers, usize::from(on_border), "pixel ({column}, {row})");
        }
    }
}

#[test]
#[ignore = "requires an offscreen GPU adapter"]
fn hidden_note_gpu_blends_the_translated_outline_once_over_an_opaque_mine() {
    use crate::renderer::{InternalResolutionMode, Renderer, SurfaceSize};
    let mut renderer = Renderer::default();
    renderer.set_skin_context(document_skin());
    renderer.set_internal_resolution_mode(InternalResolutionMode::Native);
    renderer.attach_offscreen(SurfaceSize { width: 1000, height: 1000 }).unwrap();
    renderer.upsert_rgba_texture_ref(DEFAULT_MINE_NOTE_TEXTURE, 1, 1, &[0, 0, 255, 255]).unwrap();
    let mut snapshot = coincident_hidden_note_and_mine();
    snapshot.skin_offsets.set(10, SkinOffsetValue { x: 10, ..Default::default() });
    renderer.render_scene(AppSceneSnapshot::Play(snapshot)).unwrap();
    let pixels = renderer.read_offscreen_rgba().unwrap();
    let pixel = |x: usize, y: usize| &pixels[(1000 * y + x) * 4..(1000 * y + x + 1) * 4];
    let top = pixel(225, 483);
    assert!(
        (127..=129).contains(&top[0])
            && (81..=84).contains(&top[1])
            && (126..=129).contains(&top[2]),
        "{top:?}"
    );
    assert_eq!(pixel(201, 483), top, "corner");
    assert_eq!(pixel(201, 490), top, "left edge");
    assert_eq!(pixel(225, 490), &[0, 0, 255, 255], "mine inside outline");
}
