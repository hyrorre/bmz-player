use super::*;

#[test]
fn lua_destination_clip_keeps_partial_keyframes_through_conversion() {
    let root = unique_test_dir("destination-clip");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("skin.lua");
    fs::write(
        &path,
        r#"return {
        type=5, destination={{id='image', dst={
            {time=0, clip_x=10.0, clip_y=20.0, clip_w=30.0, clip_h=40.0},
            {time=100, clip_x=11.0},
            {time=200, clip_w=0}
        }}}
    }"#,
    )
    .unwrap();
    let loaded = load_lua_skin_with_runtime_state(
        &path,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap();
    let bmz_skin_document::DestinationListEntry::Single(destination) =
        &loaded.document.destination[0]
    else {
        panic!("expected destination");
    };
    let frames: Vec<_> = destination
        .dst
        .iter()
        .map(|entry| {
            let bmz_skin_document::SkinDstEntry::Frame(frame) = entry else {
                panic!("expected frame");
            };
            *frame
        })
        .collect();
    assert_eq!(
        [frames[0].clip_x, frames[0].clip_y, frames[0].clip_w, frames[0].clip_h],
        [Some(10), Some(20), Some(30), Some(40)]
    );
    assert_eq!(frames[1].clip_x, Some(11));
    assert_eq!(frames[1].clip_w, None, "inheritance is evaluated with the destination timeline");
    assert_eq!(frames[2].clip_w, Some(0));
    fs::remove_dir_all(root).unwrap();
}
