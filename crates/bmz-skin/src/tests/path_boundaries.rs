use super::*;

#[test]
fn json_and_lr2_includes_share_explicit_roots_and_reject_external_references() {
    let temp = tempfile::tempdir().unwrap();
    let skins = temp.path().join("skins");
    let shared = skins.join("shared");
    let package = skins.join("theme");
    let outside = temp.path().join("outside");
    for dir in [&shared, &package, &outside] {
        fs::create_dir_all(dir).unwrap();
    }
    fs::write(shared.join("part.json"), r#"{"type":5,"name":"shared"}"#).unwrap();
    fs::write(shared.join("part.csv"), "#IMAGE,shared.png\n").unwrap();
    fs::write(outside.join("part.json"), "invalid JSON: must not be read").unwrap();
    fs::write(outside.join("part.csv"), "#IMAGE,private.png\n").unwrap();
    let json = package.join("skin.json");
    let csv = package.join("skin.lr2skin");
    fs::write(&json, r#"{"include":"../shared/part.json"}"#).unwrap();
    fs::write(&csv, "#INFORMATION,5,Shared,Author\n#INCLUDE,../shared/part.csv\n").unwrap();
    let json_context = SkinPathContext::new(&json, [skins.clone()]).unwrap();
    let csv_context = SkinPathContext::new(&csv, [skins]).unwrap();
    assert_eq!(
        SkinDocument::load_beatoraja_json_with_context(&json_context, None).unwrap().name,
        "shared"
    );
    let load_csv = || {
        load_lr2_csv_skin_with_path_context(
            &csv_context,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &LuaLoadRuntimeState::default(),
        )
    };
    assert!(load_csv().is_ok());
    for target in [
        "../../outside/part.json".to_string(),
        outside.join("part.json").to_string_lossy().into_owned(),
    ] {
        fs::write(&json, serde_json::json!({"include": target}).to_string()).unwrap();
        let error = format!(
            "{:#}",
            SkinDocument::load_beatoraja_json_with_context(&json_context, None).unwrap_err()
        );
        assert!(error.contains("escapes skin root"), "{error}");
    }
    for target in ["../../outside/part.csv", "../../outside/missing-*.csv"] {
        fs::write(&csv, format!("#INFORMATION,5,Outside,Author\n#INCLUDE,{target}\n")).unwrap();
        assert!(format!("{:#}", load_csv().unwrap_err()).contains("escapes skin root"));
    }
}

#[test]
fn lr2_include_cycles_stop_in_inference_and_conversion_but_reuse_is_valid() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let entry = root.join("skin.lr2skin");
    let load = || load_lr2_csv_skin(&entry, SkinKind::Play, &BTreeMap::new(), &BTreeMap::new());
    for resolution in ["", "#RESOLUTION,0\n"] {
        fs::write(&entry, format!("#INFORMATION,0,Cycle,Author\n{resolution}#INCLUDE,a.csv\n"))
            .unwrap();
        fs::write(root.join("a.csv"), "#INCLUDE,b.csv\n").unwrap();
        fs::write(root.join("b.csv"), "#INCLUDE,a.csv\n").unwrap();
        let error = format!("{:#}", load().unwrap_err());
        assert!(error.contains("cyclic LR2 include"), "{error}");
        fs::write(root.join("b.csv"), "#IMAGE,normal.png\n").unwrap();
        fs::write(root.join("a.csv"), "#INCLUDE,b.csv\n#INCLUDE,b.csv\n").unwrap();
        assert!(load().is_ok());
    }
    fs::write(&entry, "#INFORMATION,0,Depth,Author\n#RESOLUTION,0\n#INCLUDE,d0.csv\n").unwrap();
    for i in 0..66 {
        fs::write(root.join(format!("d{i}.csv")), format!("#INCLUDE,d{}.csv\n", i + 1)).unwrap();
    }
    assert!(format!("{:#}", load().unwrap_err()).contains("include limit"));
}

#[cfg(unix)]
#[test]
fn linked_includes_and_overlay_parents_cannot_escape() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skins");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.lua"), "error('OUTSIDE EVALUATED')").unwrap();
    fs::write(outside.join("part.json"), "{}").unwrap();
    fs::write(outside.join("part.csv"), "#IMAGE,outside.png\n").unwrap();
    symlink(&outside, root.join("escape")).unwrap();
    symlink(outside.join("absent"), root.join("broken")).unwrap();
    let lua = root.join("skin.luaskin");
    fs::write(&lua, "local f = io.open('escape/new/log.txt', 'w'); if f then f:write('discarded'):close() end; assert(io.open('escape/new/log.txt', 'r') == nil); local ok = pcall(dofile, 'escape/secret.lua'); assert(not ok); return {type=5}").unwrap();
    let context = SkinPathContext::new(&lua, [root.clone()]).unwrap();
    load_lua_skin_header_value_with_path_context(&context).unwrap();
    assert!(!outside.join("new").exists());
    let json = root.join("skin.json");
    fs::write(&json, r#"{"include":"escape/part.json"}"#).unwrap();
    let json_context = SkinPathContext::new(&json, [root.clone()]).unwrap();
    assert!(SkinDocument::load_beatoraja_json_with_context(&json_context, None).is_err());
    let csv = root.join("skin.lr2skin");
    for target in
        ["escape/part.csv", "escape/missing-*.csv", "escape/new/part-*.csv", "broken/part-*.csv"]
    {
        fs::write(&csv, format!("#INFORMATION,5,Test,Author\n#INCLUDE,{target}\n")).unwrap();
        assert!(
            load_lr2_csv_skin(&csv, SkinKind::Select, &BTreeMap::new(), &BTreeMap::new()).is_err(),
            "{target}"
        );
    }
}
