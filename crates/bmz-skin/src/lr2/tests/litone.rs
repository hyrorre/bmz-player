use super::*;

#[test]
fn lr2_single_custom_offset_loads_as_document() {
    let root = unique_test_dir("lr2-single-custom-offset");
    let path = root.join("select.lr2skin");
    fs::write(&path, "#INFORMATION,5,test,test\n#CUSTOMOFFSET,Position,40,1,1,0,0,0,0\n").unwrap();
    let loaded = crate::load_lr2_csv_skin(
        &path,
        crate::SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(loaded.document.offset.len(), 1);
    let offset = &loaded.document.offset[0];
    assert_eq!(offset.id, 40);
    assert!(offset.x && offset.y);
    assert!(!offset.w && !offset.h && !offset.r && !offset.a);
}

#[test]
fn litone5_installed_definitions_load_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/LITONE5");
    let mut failures = Vec::new();
    for (relative, kind) in [
        ("Select/select.lr2skin", crate::SkinKind::Select),
        ("Decide/decide.lr2skin", crate::SkinKind::Decide),
        ("Result/result.lr2skin", crate::SkinKind::Result),
        ("Result/courseresult.lr2skin", crate::SkinKind::Result),
        ("Play/AC7.lr2skin", crate::SkinKind::Play),
        ("Play/CS7.lr2skin", crate::SkinKind::Play),
        ("Play/HALFWIDE7.lr2skin", crate::SkinKind::Play),
        ("Play/AC9.lr2skin", crate::SkinKind::Play),
        ("Play/AC14.lr2skin", crate::SkinKind::Play),
    ] {
        let path = root.join(relative);
        if !path.is_file() {
            eprintln!("SKIP missing {}", path.display());
            continue;
        }
        match crate::load_lr2_csv_skin(&path, kind, &BTreeMap::new(), &BTreeMap::new()) {
            Ok(loaded) => {
                eprintln!(
                    "LITONE5 {relative}: {} images, {} destinations, {} warnings",
                    loaded.document.image.len(),
                    loaded.document.destination.len(),
                    loaded.warnings.len()
                );
                for warning in &loaded.warnings {
                    eprintln!("  {}", warning.message);
                }
                assert!(loaded.document.lr2);
                assert!(!loaded.document.destination.is_empty(), "{relative}");
            }
            Err(error) => failures.push(format!("{relative}: {error:#}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
